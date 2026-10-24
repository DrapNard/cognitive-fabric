//! Bounded completeness checks for earlier typed dispatches; never replay them.
use super::module_history::Pins;
use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

struct Dispatch {
    identity: (String, String, String, String),
    returned: Option<String>,
    process_intent: bool,
    launched: bool,
    process_returned: Option<String>,
}
fn text(value: &Value, field: &str) -> Result<String> {
    value[field]
        .as_str()
        .filter(|s| s.len() <= 128)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Invalid typed continuation audit field: {field}"))
}
fn digest(value: &Value, field: &str) -> Result<String> {
    let value = text(value, field)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        bail!("Invalid typed continuation audit digest")
    }
    Ok(value)
}
fn identity(value: &Value, pins: &Pins) -> Result<(String, String, String, String)> {
    let id = text(value, "module_id")?;
    let version = text(value, "module_version")?;
    let capability = text(value, "capability")?;
    let pinned = pins.iter().any(|(route, candidates)| {
        route.as_str() == capability
            && candidates.candidates().iter().any(|pin| {
                let descriptor = pin.descriptor();
                descriptor.id == id && descriptor.version == version
            })
    });
    if value["format_version"] != 1 || !pinned {
        bail!("Typed dispatch differs from original pinned registry")
    }
    Ok((id, version, capability, digest(value, "request_sha256")?))
}

pub(super) fn validate(
    db: &rusqlite::Connection,
    task: &str,
    pins: &Pins,
    broker: &str,
    snapshot_id: i64,
) -> Result<usize> {
    let mut statement = db.prepare(
        "SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,
            CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END,id
         FROM events WHERE task_id=?1 AND kind IN
         ('tool_reservation','module_dispatch_intent','module_dispatch_outcome',
          'module_process_dispatch_intent','module_process_dispatch_outcome',
          'module_process_launch','module_process_owned','module_process_stopped',
          'expert_provider_snapshot') ORDER BY id LIMIT 1025",
    )?;
    let mut rows = statement.query([task])?;
    let mut calls = BTreeMap::<String, Dispatch>::new();
    let mut reservations = Vec::<(u64, String, bool)>::new();
    let mut pids = BTreeSet::new();
    let mut launched = 0usize;
    let mut owned = 0usize;
    let mut reaped = 0usize;
    let mut total_bytes = 0usize;
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        count += 1;
        let kind: String = row.get(0)?;
        let source: String = row.get(1)?;
        let raw: String = row.get(2)?;
        if kind != "tool_reservation" && row.get::<_, i64>(3)? <= snapshot_id {
            bail!("Typed dispatch precedes its original registry snapshot")
        }
        total_bytes = total_bytes
            .checked_add(raw.len())
            .ok_or_else(|| anyhow!("Typed history size overflow"))?;
        if count > 1024 || total_bytes > 4 * 1024 * 1024 {
            bail!("Typed continuation history exceeds bounds")
        }
        let value = crate::strict_json::value(raw.as_bytes(), 16384)?;
        if kind == "tool_reservation" {
            let invocation = value["invocation"]
                .as_u64()
                .filter(|n| (1..=64).contains(n))
                .ok_or_else(|| anyhow!("Invalid original tool reservation"))?;
            if source != "security-kernel"
                || value["format_version"] != 1
                || value["actor"] != "jev-plus"
                || reservations.iter().any(|r| r.0 == invocation)
            {
                bail!("Invalid or duplicate original tool reservation")
            }
            reservations.push((invocation, text(&value, "capability")?, false));
            continue;
        }
        if kind == "module_process_owned" || kind == "module_process_stopped" {
            let pid = value["pid"]
                .as_u64()
                .filter(|pid| *pid > 0 && *pid <= i32::MAX as u64)
                .ok_or_else(|| anyhow!("Invalid typed child identity"))?;
            if source != "fabric" {
                bail!("Invalid typed child audit source")
            }
            if kind == "module_process_owned" {
                if value["sandboxed"] != false || !pids.insert(pid) || owned >= launched {
                    bail!("Unbound typed process ownership")
                }
                owned += 1;
            } else {
                if value["reaped"] != true
                    || value["forced"] != false
                    || value["exit_code"] != 0
                    || !pids.remove(&pid)
                {
                    bail!("Typed child outcome is unresolved or unsuccessful")
                }
                reaped += 1;
            }
            continue;
        }
        if kind == "expert_provider_snapshot" {
            let capability = text(&value, "required_capability")?;
            let pin = pins
                .iter()
                .find(|(cap, _)| cap.as_str() == capability)
                .and_then(|(_, candidates)| {
                    candidates.exact(
                        value["module_id"].as_str()?,
                        value["module_version"].as_str()?,
                    )
                })
                .ok_or_else(|| anyhow!("Expert provider has no original module pin"))?;
            let descriptor = pin.descriptor();
            let capabilities = value["capabilities"]
                .as_array()
                .ok_or_else(|| anyhow!("Invalid provider capabilities"))?;
            let expected = descriptor
                .capabilities
                .iter()
                .map(|cap| Value::String(cap.as_str().into()))
                .collect::<Vec<_>>();
            if source != "jev-plus"
                || value["format_version"] != 1
                || value["broker_id"] != broker
                || value["module_id"] != descriptor.id
                || value["module_version"] != descriptor.version
                || *capabilities != expected
                || value["provider_permissions_granted"] != false
                || value["task_grant_created"] != false
            {
                bail!("Expert provider snapshot differs from original registry")
            }
            digest(&value, "manifest_sha256")?;
            continue;
        }
        let correlation = text(&value, "correlation_id")?;
        uuid::Uuid::parse_str(&correlation)?;
        let current = identity(&value, pins)?;
        let expected_source = if kind == "module_process_launch" {
            "security-kernel"
        } else {
            "jev-plus"
        };
        if source != expected_source {
            bail!("Typed dispatch audit has an invalid actor")
        }
        if kind == "module_dispatch_intent" {
            if calls.len() >= 64
                || value["module_granted_permissions"] != false
                || value["request_bytes"].as_u64().is_none_or(|n| n > 32000)
                || value["deadline_ms"]
                    .as_u64()
                    .is_none_or(|n| !(1..=300000).contains(&n))
            {
                bail!("Invalid typed dispatch intent bounds")
            }
            let reservation = reservations
                .iter_mut()
                .rev()
                .find(|r| !r.2 && r.1 == current.2)
                .ok_or_else(|| anyhow!("Typed dispatch has no unused original tool reservation"))?;
            reservation.2 = true;
            if calls
                .insert(
                    correlation,
                    Dispatch {
                        identity: current,
                        returned: None,
                        process_intent: false,
                        launched: false,
                        process_returned: None,
                    },
                )
                .is_some()
            {
                bail!("Duplicate original typed dispatch")
            }
            continue;
        }
        let call = calls
            .get_mut(&correlation)
            .filter(|call| call.identity == current)
            .ok_or_else(|| anyhow!("Typed outcome has no matching original intent"))?;
        match kind.as_str() {
            "module_dispatch_outcome" => {
                if value["status"] != "returned"
                    || value["provider_stop_proven"] != false
                    || value["response_bytes"].as_u64().is_none_or(|n| n > 16384)
                    || call
                        .returned
                        .replace(digest(&value, "response_sha256")?)
                        .is_some()
                {
                    bail!("Typed dispatch outcome is uncertain, invalid or duplicated")
                }
            }
            "module_process_dispatch_intent" => {
                if call.process_intent
                    || call.returned.is_some()
                    || value["permissions_granted"] != false
                {
                    bail!("Invalid typed process intent")
                }
                call.process_intent = true;
            }
            "module_process_launch" => {
                if !call.process_intent
                    || call.launched
                    || call.returned.is_some()
                    || value["sandboxed"] != false
                    || value["calls_granted"] != 1
                {
                    bail!("Invalid typed process launch")
                }
                call.launched = true;
                launched += 1;
            }
            "module_process_dispatch_outcome" => {
                if !call.launched
                    || call.returned.is_some()
                    || value["status"] != "returned"
                    || value["sandboxed"] != false
                    || call
                        .process_returned
                        .replace(digest(&value, "response_sha256")?)
                        .is_some()
                {
                    bail!("Invalid typed process outcome")
                }
            }
            _ => bail!("Unknown typed continuation audit"),
        }
    }
    if !pids.is_empty() || launched != owned || owned != reaped {
        bail!("Typed process ownership is not completely reaped")
    }
    for call in calls.values() {
        if call.returned.is_none()
            || (call.process_intent && (!call.launched || call.process_returned != call.returned))
        {
            bail!("Unresolved typed dispatch cannot be replayed or silently discarded")
        }
    }
    Ok(calls.len())
}
