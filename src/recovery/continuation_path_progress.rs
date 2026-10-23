//! Reconstruct shortest-path progress from the original marked task history.
use crate::{config::Config, reasoning::progress::PathHistory, store::sha256};
use anyhow::{bail, Result};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const POLICY: &str = "shortest-path-progress/v1";
const CRITERION: &str = "shortest_path/v1";
const MAX_EVENTS: usize = 209;
const MAX_EVENT_BYTES: usize = 262_144;
const MAX_HISTORY_BYTES: usize = 8_000_000;

/// Validate route progress when the immutable task and context markers opt in.
/// An unmarked historical task remains on the old path, but cannot contain
/// path-progress records or snapshots that would make the missing marker ambiguous.
pub(super) fn validate_before(
    db: &rusqlite::Connection,
    task: &str,
    query: &str,
    cfg: &Config,
    boundary: Option<&Value>,
    before_id: Option<i64>,
    before_boundary_id: Option<i64>,
) -> Result<Option<PathHistory>> {
    let criterion = crate::verification::Criterion::from_query(query)?;
    let shortest_path = criterion
        .as_ref()
        .is_some_and(|criterion| criterion.name() == CRITERION);
    let eligible_configuration = shortest_path && cfg.progress_stopping.is_some();

    let settings_raw: Option<String> = db
        .query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 1",
            [task],
            |row| row.get(0),
        )
        .optional()?;
    let settings_marker = settings_raw
        .as_deref()
        .map(|raw| crate::strict_json::value(raw.as_bytes(), MAX_EVENT_BYTES))
        .transpose()?
        .and_then(|payload| payload.get("progress_policy").cloned());

    let mut context_statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning_context' ORDER BY id LIMIT 18",
    )?;
    let context_rows = context_statement.query_map([task], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut context_markers = Vec::new();
    for row in context_rows {
        let (id, source, raw) = row?;
        let source =
            source.ok_or_else(|| anyhow::anyhow!("Reasoning context source exceeds bounds"))?;
        let raw = raw.ok_or_else(|| anyhow::anyhow!("Reasoning context exceeds bounds"))?;
        if source != "jev-r" {
            bail!("Invalid shortest-path progress context source")
        }
        let payload = crate::strict_json::value(raw.as_bytes(), MAX_EVENT_BYTES)?;
        context_markers.push((id, payload.get("progress_policy").cloned()));
        if context_markers.len() > 17 {
            bail!("Shortest-path progress context history exceeds bounds")
        }
    }
    drop(context_statement);
    let context_anchor = context_markers.first().map(|(id, _)| *id);

    let path_marker_present =
        settings_marker.is_some() || context_markers.iter().any(|(_, marker)| marker.is_some());
    let path_snapshot_present =
        boundary.is_some_and(|value| value.get("path_progress_state").is_some());
    let path_audit_exists = if shortest_path {
        let mut statement = db.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('reasoning_progress_provider_veto','reasoning_progress') ORDER BY id LIMIT 34",
        )?;
        let rows = statement.query_map([task], |row| row.get::<_, Option<String>>(0))?;
        let mut found = false;
        let mut count = 0usize;
        for row in rows {
            count += 1;
            if count > 33 {
                bail!("Shortest-path progress audit history exceeds bounds")
            }
            let raw = row?.ok_or_else(|| anyhow::anyhow!("Progress audit exceeds bounds"))?;
            let payload = crate::strict_json::value(raw.as_bytes(), MAX_EVENT_BYTES)?;
            if payload.get("criterion").and_then(Value::as_str) != Some(CRITERION) {
                bail!("Shortest-path task contains a malformed or mismatched progress audit")
            }
            found = true;
        }
        found
    } else {
        false
    };
    let active = eligible_configuration
        && settings_marker.as_ref() == Some(&json!(POLICY))
        && context_markers
            .first()
            .is_some_and(|(_, marker)| marker.as_ref() == Some(&json!(POLICY)))
        && context_markers
            .iter()
            .all(|(_, marker)| marker.as_ref() == Some(&json!(POLICY)));

    if path_marker_present && !active {
        bail!("Shortest-path progress markers do not match the original configuration and context")
    }
    if !active {
        if path_audit_exists || path_snapshot_present {
            bail!("Unmarked legacy history contains shortest-path progress metadata")
        }
        return Ok(None);
    }

    // Marker fields are exact opt-ins: no non-string/null variant is accepted.
    if settings_marker != Some(json!(POLICY))
        || context_markers.is_empty()
        || context_markers
            .iter()
            .any(|(_, marker)| marker != &Some(json!(POLICY)))
        || boundary.is_some_and(|value| value.get("progress_state").is_some())
    {
        bail!("Malformed shortest-path progress opt-in markers")
    }

    if let Some(boundary_id) = before_boundary_id {
        let mut statement = db.prepare(
            "SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND id>?2 AND kind IN ('reasoning','verification','verification_provider_result','reasoning_progress','reasoning_progress_provider_veto') ORDER BY id LIMIT 66",
        )?;
        let rows = statement.query_map(rusqlite::params![task, boundary_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut count = 0usize;
        for row in rows {
            count += 1;
            if count > 65 {
                bail!("Post-boundary path progress history exceeds bounds")
            }
            let (kind, source, raw) = row?;
            let raw = raw.ok_or_else(|| anyhow::anyhow!("Post-boundary event exceeds bounds"))?;
            let payload = crate::strict_json::value(raw.as_bytes(), MAX_EVENT_BYTES)?;
            match kind.as_str() {
                "reasoning_progress" | "reasoning_progress_provider_veto"
                    if payload["criterion"] == CRITERION =>
                {
                    bail!("Future shortest-path progress assessment follows the saved boundary")
                }
                "verification_provider_result" => {
                    if payload["criterion"] != CRITERION {
                        continue;
                    }
                    bail!("Future shortest-path provider result follows the saved boundary")
                }
                "verification" if payload["criterion"] == CRITERION => {
                    bail!("Future shortest-path verification follows the saved boundary")
                }
                "reasoning"
                    if payload["node"]["role"] == "solver"
                        && payload["node"]["cycle"].as_u64().unwrap_or(0) > 0 =>
                {
                    if source.as_deref() != Some("jev-r") {
                        bail!("Invalid future shortest-path solver source")
                    }
                    bail!("Future whole-goal solver result follows the saved boundary")
                }
                _ => {}
            }
        }
    }

    let mut statement = db.prepare(
        "SELECT id,kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('reasoning','verification','verification_provider_result','reasoning_progress','reasoning_progress_provider_veto') AND (?2 IS NULL OR id<?2) AND (?3 IS NULL OR id<?3) ORDER BY id LIMIT 210",
    )?;
    let rows = statement.query_map(
        rusqlite::params![task, before_id, before_boundary_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    let mut solvers = BTreeMap::<u32, (i64, u64, String, String)>::new();
    let mut cycle_zero_solvers = Vec::<(i64, u64, String, String)>::new();
    let mut cycle_zero_solver_calls = std::collections::BTreeSet::<u64>::new();
    let mut cycle_zero_rechecks = Vec::<(i64, Value)>::new();
    let mut cycle_zero_recheck_calls = std::collections::BTreeSet::<u64>::new();
    let mut verifications = BTreeMap::<u32, Vec<(i64, Value)>>::new();
    let mut assessments = BTreeMap::<u32, (i64, bool, Value)>::new();
    let mut provider_results = Vec::<(i64, Value)>::new();
    let mut used_provider_results = std::collections::BTreeSet::<i64>::new();
    let mut bytes = 0usize;
    let mut count = 0usize;
    for row in rows {
        count += 1;
        let (id, kind, source, raw) = row?;
        let source =
            source.ok_or_else(|| anyhow::anyhow!("Path progress source exceeds bounds"))?;
        let raw = raw.ok_or_else(|| anyhow::anyhow!("Path progress payload exceeds bounds"))?;
        bytes = bytes.saturating_add(source.len()).saturating_add(raw.len());
        if count > MAX_EVENTS || bytes > MAX_HISTORY_BYTES {
            bail!("Shortest-path progress history exceeds bounds")
        }
        let payload = crate::strict_json::value(raw.as_bytes(), MAX_EVENT_BYTES)?;
        match kind.as_str() {
            "reasoning" => {
                if source != "jev-r" {
                    bail!("Invalid shortest-path solver source")
                }
                if payload["node"]["role"] != "solver" {
                    continue;
                }
                if payload.as_object().is_none_or(|object| {
                    object.keys().any(|key| {
                        !matches!(
                            key.as_str(),
                            "call"
                                | "node"
                                | "text"
                                | "input_tokens"
                                | "output_tokens"
                                | "inference_ms"
                                | "queue_wait_ms"
                        )
                    })
                }) {
                    bail!("Unexpected field in shortest-path solver event")
                }
                let node: crate::reasoning::Node = serde_json::from_value(payload["node"].clone())?;
                let cycle = node.cycle;
                let answer = payload["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing shortest-path solver answer"))?;
                let hash = sha256(answer.as_bytes());
                let call = payload["call"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing shortest-path solver call"))?;
                if answer.len() > 96_000 || node.content_hash != hash {
                    bail!("Altered shortest-path solver answer")
                }
                if cycle == 0 {
                    if !cycle_zero_solver_calls.insert(call) {
                        bail!("Duplicate cycle-zero shortest-path solver call identity")
                    }
                    cycle_zero_solvers.push((id, call, answer.to_owned(), hash));
                } else if solvers
                    .insert(cycle, (id, call, answer.to_owned(), hash))
                    .is_some()
                {
                    bail!("Duplicate shortest-path whole-goal solver cycle")
                }
            }
            "verification" if payload["origin"] == "published_memory" => {
                if source != "verifier-r" {
                    bail!("Invalid published-memory verification source")
                }
                validate_published_memory_check(query, id, context_anchor, &payload)?;
                if payload.get("provider").is_some() {
                    match_provider_result(
                        &provider_results,
                        &mut used_provider_results,
                        id,
                        query,
                        &payload,
                    )?;
                }
            }
            "verification_provider_result" => {
                if payload["criterion"] != CRITERION {
                    continue;
                }
                if source != "verifier-r" {
                    bail!("Invalid shortest-path provider result source")
                }
                validate_provider_result(&source, &payload)?;
                provider_results.push((id, payload));
            }
            "verification" if payload["criterion"] == CRITERION => {
                if source != "verifier-r" {
                    bail!("Invalid shortest-path verification source")
                }
                if payload["criterion"] != CRITERION {
                    bail!("Unexpected verification criterion in shortest-path history")
                }
                let cycle = payload["cycle"]
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| anyhow::anyhow!("Invalid shortest-path verification cycle"))?;
                if cycle == 0 {
                    if payload["origin"] != "saved_solver_recovery" {
                        bail!("Unsupported cycle-zero shortest-path verification")
                    }
                    cycle_zero_rechecks.push((id, payload));
                    continue;
                }
                verifications.entry(cycle).or_default().push((id, payload));
            }
            "reasoning_progress" | "reasoning_progress_provider_veto" => {
                if payload.get("criterion").and_then(Value::as_str) != Some(CRITERION) {
                    bail!("Malformed or mismatched shortest-path progress assessment")
                }
                if source != "verifier-r" {
                    bail!("Invalid shortest-path progress audit source")
                }
                if payload["policy"] != POLICY {
                    bail!("Shortest-path progress assessment has an altered policy or criterion")
                }
                let cycle = payload["cycle"]
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|cycle| *cycle > 0)
                    .ok_or_else(|| anyhow::anyhow!("Invalid shortest-path progress cycle"))?;
                let veto = kind == "reasoning_progress_provider_veto";
                if assessments.insert(cycle, (id, veto, payload)).is_some() {
                    bail!("Duplicate shortest-path progress assessment")
                }
            }
            _ => {}
        }
    }
    drop(statement);

    if solvers.len() > 16 || assessments.len() > 16 {
        bail!("Shortest-path progress exceeds the cycle cap")
    }
    if cycle_zero_solvers.len() > 32 || cycle_zero_rechecks.len() > 32 {
        bail!("Cycle-zero shortest-path recovery evidence exceeds bounds")
    }
    for (event_id, proof) in cycle_zero_rechecks {
        let fields = [
            "criterion",
            "verified",
            "origin",
            "objective_hash",
            "answer_hash",
            "reason",
            "cycle",
            "call",
        ];
        if !exact_fields(&proof, &fields)
            || proof["criterion"] != CRITERION
            || proof["verified"] != false
            || proof["origin"] != "saved_solver_recovery"
            || proof["objective_hash"] != sha256(query.as_bytes())
            || proof["cycle"] != 0
        {
            bail!("Malformed cycle-zero shortest-path saved-result verification")
        }
        let answer_hash = proof["answer_hash"]
            .as_str()
            .filter(|hash| is_lower_hex_hash(Some(hash)))
            .ok_or_else(|| anyhow::anyhow!("Invalid cycle-zero shortest-path answer hash"))?;
        let call = proof["call"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing cycle-zero shortest-path solver call"))?;
        if !cycle_zero_recheck_calls.insert(call) {
            bail!("Duplicate cycle-zero shortest-path saved-result verification")
        }
        let reason = proof["reason"]
            .as_str()
            .filter(|reason| !reason.is_empty() && reason.len() <= 16384)
            .ok_or_else(|| anyhow::anyhow!("Invalid cycle-zero shortest-path rejection"))?;
        let matched =
            cycle_zero_solvers
                .iter()
                .any(|(solver_id, solver_call, answer, solver_hash)| {
                    *solver_id < event_id
                        && *solver_call == call
                        && solver_hash == answer_hash
                        && crate::verification::Criterion::from_query(query)
                            .ok()
                            .flatten()
                            .and_then(|criterion| criterion.check(answer).err())
                            .is_some_and(|error| error.to_string() == reason)
                });
        if !matched {
            bail!("Cycle-zero shortest-path recovery has no preceding matching rejected solver output")
        }
    }
    let cycles = solvers.keys().copied().collect::<Vec<_>>();
    if cycles
        .iter()
        .enumerate()
        .any(|(index, cycle)| *cycle as usize != index + 1)
    {
        bail!("Shortest-path solver history is gapped")
    }
    let mut history = PathHistory::default();
    for (position, cycle) in cycles.iter().copied().enumerate() {
        let (solver_id, solver_call, answer, answer_hash) = &solvers[&cycle];
        let checks = verifications
            .get(&cycle)
            .ok_or_else(|| anyhow::anyhow!("Missing shortest-path independent verification"))?;
        let compiled = criterion
            .as_ref()
            .expect("active shortest-path marker has a parsed criterion")
            .check(answer);
        let veto = compiled.is_ok()
            && checks.len() == 1
            && checks[0].1["provider_accepted"] == false
            && checks[0].1["compiled_accepted"] == true;
        if veto {
            let (verification_id, verification) = &checks[0];
            let expected_fields = [
                "verified",
                "criterion",
                "expert",
                "objective_hash",
                "answer_hash",
                "reason",
                "cycle",
                "provider",
                "provider_accepted",
                "compiled_accepted",
                "provider_reason_code",
            ];
            if !exact_fields(verification, &expected_fields)
                || verification["verified"] != false
                || verification["criterion"] != CRITERION
                || verification["objective_hash"] != sha256(query.as_bytes())
                || verification["answer_hash"] != answer_hash.as_str()
                || verification["cycle"] != cycle
                || !verification["provider"].is_object()
                || verification["reason"]
                    .as_str()
                    .is_none_or(|value| value.is_empty() || value.len() > 16384)
                || verification["provider_reason_code"]
                    .as_str()
                    .is_none_or(|value| value.is_empty() || value.len() > 128)
                || *verification_id <= *solver_id
            {
                bail!("Malformed shortest-path provider-veto verification")
            }
            match_provider_result(
                &provider_results,
                &mut used_provider_results,
                *verification_id,
                query,
                verification,
            )?;
        } else {
            let compiled_reason = compiled
                .err()
                .ok_or_else(|| {
                    anyhow::anyhow!("Accepted shortest path cannot enter failed-answer history")
                })?
                .to_string();
            if checks.is_empty() || checks.len() > 2 {
                bail!("Unsupported shortest-path failed verification set")
            }
            let mut canonical = 0usize;
            let mut recovery = 0usize;
            let mut max_verification_id = *solver_id;
            for (verification_id, verification) in checks {
                let allowed = [
                    "verified",
                    "criterion",
                    "expert",
                    "objective_hash",
                    "answer_hash",
                    "reason",
                    "cycle",
                    "origin",
                    "provider",
                    "provider_accepted",
                    "compiled_accepted",
                    "provider_reason_code",
                    "call",
                ];
                if verification
                    .as_object()
                    .is_none_or(|object| object.keys().any(|key| !allowed.contains(&key.as_str())))
                    || verification["verified"] != false
                    || verification["criterion"] != CRITERION
                    || verification["objective_hash"] != sha256(query.as_bytes())
                    || verification["answer_hash"] != answer_hash.as_str()
                    || verification["cycle"] != cycle
                    || verification["reason"].as_str() != Some(compiled_reason.as_str())
                    || verification
                        .get("expert")
                        .is_some_and(|value| !value.is_null() && !value.is_object())
                    || verification.get("provider_accepted").is_some()
                        != verification.get("compiled_accepted").is_some()
                    || verification
                        .get("compiled_accepted")
                        .is_some_and(|value| value != false)
                    || verification
                        .get("provider_accepted")
                        .is_some_and(|value| value != &Value::Bool(true))
                    || verification
                        .get("provider")
                        .is_some_and(|value| !value.is_object())
                    || verification
                        .get("origin")
                        .is_some_and(|value| value != "saved_solver_recovery")
                {
                    bail!("Malformed shortest-path failed verification")
                }
                match verification.get("origin").and_then(Value::as_str) {
                    None => canonical += 1,
                    Some("saved_solver_recovery") => recovery += 1,
                    _ => unreachable!(),
                }
                if verification
                    .get("call")
                    .is_some_and(|value| value.as_u64() != Some(*solver_call))
                {
                    bail!("Shortest-path verification call differs from its solver")
                }
                if verification.get("provider").is_some() {
                    match_provider_result(
                        &provider_results,
                        &mut used_provider_results,
                        *verification_id,
                        query,
                        verification,
                    )?;
                }
                if *verification_id <= *solver_id
                    || cycles
                        .get(position + 1)
                        .is_some_and(|next| *verification_id >= solvers[next].0)
                {
                    bail!("Shortest-path verification is out of order")
                }
                max_verification_id = max_verification_id.max(*verification_id);
            }
            if !((checks.len() == 1 && canonical == 1)
                || (checks.len() == 2 && canonical == 1 && recovery == 1))
            {
                bail!("Shortest-path failed verification set is duplicated or unsupported")
            }
            if max_verification_id <= *solver_id {
                bail!("Shortest-path verification is not after its solver")
            }
        }

        let (assessment_id, is_veto, recorded) = assessments
            .remove(&cycle)
            .ok_or_else(|| anyhow::anyhow!("Missing shortest-path progress assessment"))?;
        let checks = verifications
            .get(&cycle)
            .expect("verification set was just validated");
        // Runtime verification must precede the assessment. A later
        // saved-result recovery recheck is valid evidence for the same answer.
        let first_check_id = checks.iter().map(|(id, _)| *id).min().unwrap();
        if assessment_id <= first_check_id
            || cycles
                .get(position + 1)
                .is_some_and(|next| assessment_id >= solvers[next].0)
        {
            bail!("Shortest-path progress audit is out of order")
        }
        let expected = if veto {
            if !is_veto {
                bail!("Compiled-valid path veto lacks its unscored veto audit")
            }
            history.observe_provider_veto(
                query,
                cycle,
                answer,
                cfg.progress_stopping.as_ref().unwrap(),
            )?
        } else {
            if is_veto {
                bail!("Failed shortest-path candidate has a provider-veto audit")
            }
            history.observe(
                query,
                cycle,
                answer,
                cfg.progress_stopping.as_ref().unwrap(),
            )?
        };
        if recorded != expected {
            bail!("Shortest-path progress audit differs from independent recomputation")
        }
        verifications.remove(&cycle);
        if expected["stop"] == true && boundary.is_some() {
            bail!("A plateaued shortest-path task cannot continue reasoning")
        }
    }
    if !assessments.is_empty() || !verifications.is_empty() {
        bail!("Shortest-path progress or verification has no matching solver output")
    }
    if provider_results
        .iter()
        .any(|(id, _)| !used_provider_results.contains(id))
    {
        bail!("Shortest-path provider result has no matching verification")
    }

    if let Some(boundary) = boundary {
        let saved = boundary
            .get("path_progress_state")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing shortest-path progress checkpoint state"))?;
        let saved: PathHistory = serde_json::from_value(saved)?;
        if saved != history {
            bail!("Shortest-path progress checkpoint differs from recomputed failed-answer history")
        }
    }
    Ok(Some(history))
}

fn exact_fields(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|field| object.contains_key(*field))
    })
}

fn validate_published_memory_check(
    query: &str,
    event_id: i64,
    context_anchor: Option<i64>,
    payload: &Value,
) -> Result<()> {
    let fields = [
        "criterion",
        "verified",
        "origin",
        "memory_id",
        "expert",
        "objective_hash",
        "answer_hash",
        "reason",
    ];
    if !exact_fields(payload, &fields)
        || payload["criterion"] != CRITERION
        || payload["verified"] != false
        || payload["origin"] != "published_memory"
        || payload["objective_hash"] != sha256(query.as_bytes())
        || context_anchor.is_none_or(|anchor| event_id >= anchor)
    {
        bail!("Malformed shortest-path published-memory verification")
    }
    let memory_id = payload["memory_id"]
        .as_str()
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .ok_or_else(|| anyhow::anyhow!("Invalid published-memory identity"))?;
    uuid::Uuid::parse_str(memory_id)?;
    if !payload["expert"].is_null() && !payload["expert"].is_object() {
        bail!("Malformed published-memory checker identity")
    }
    if payload["answer_hash"].as_str().is_none_or(|hash| {
        hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }) || payload["reason"]
        .as_str()
        .is_none_or(|reason| reason.is_empty() || reason.len() > 16384)
        || event_id < 0
    {
        bail!("Malformed shortest-path published-memory evidence")
    }
    Ok(())
}

fn validate_provider_result(source: &str, payload: &Value) -> Result<()> {
    let fields = [
        "format_version",
        "provider",
        "criterion",
        "objective_sha256",
        "answer_sha256",
        "provider_accepted",
        "compiled_accepted",
        "verified",
        "reason_code",
        "elapsed_ms",
        "permissions_granted",
    ];
    if source != "verifier-r"
        || !exact_fields(payload, &fields)
        || payload["format_version"] != 1
        || payload["criterion"] != CRITERION
        || !payload["provider"].is_object()
        || payload["provider_accepted"].as_bool().is_none()
        || payload["compiled_accepted"].as_bool().is_none()
        || payload["verified"]
            != (payload["provider_accepted"] == true && payload["compiled_accepted"] == true)
        || payload["reason_code"]
            .as_str()
            .is_none_or(|code| code.is_empty() || code.len() > 128)
        || payload["elapsed_ms"]
            .as_f64()
            .is_none_or(|value| !value.is_finite() || !(0.0..=300_000.0).contains(&value))
        || payload["permissions_granted"] != false
        || !is_lower_hex_hash(payload["objective_sha256"].as_str())
        || !is_lower_hex_hash(payload["answer_sha256"].as_str())
    {
        bail!("Malformed shortest-path provider result")
    }
    Ok(())
}

fn match_provider_result(
    results: &[(i64, Value)],
    used: &mut std::collections::BTreeSet<i64>,
    verification_id: i64,
    query: &str,
    verification: &Value,
) -> Result<()> {
    let descriptor = verification
        .get("provider")
        .ok_or_else(|| anyhow::anyhow!("Missing provider descriptor in final verification"))?;
    let answer_hash = verification["answer_hash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing provider-bound answer hash"))?;
    let provider_accepted = &verification["provider_accepted"];
    let compiled_accepted = &verification["compiled_accepted"];
    let reason_code = if verification["compiled_accepted"] == false {
        "compiled_check_rejected"
    } else {
        verification["provider_reason_code"]
            .as_str()
            .unwrap_or("accepted")
    };
    let candidate = results.iter().rev().find(|(id, payload)| {
        *id < verification_id
            && !used.contains(id)
            && payload["criterion"] == CRITERION
            && payload["objective_sha256"] == sha256(query.as_bytes())
            && payload["answer_sha256"] == answer_hash
    });
    let (id, result) = candidate
        .ok_or_else(|| anyhow::anyhow!("Final provider verification lacks its matching result"))?;
    if &result["provider"] != descriptor
        || &result["provider_accepted"] != provider_accepted
        || &result["compiled_accepted"] != compiled_accepted
        || result["verified"] != verification["verified"]
        || result["reason_code"] != reason_code
    {
        bail!("Shortest-path provider result differs from its final verification")
    }
    used.insert(*id);
    Ok(())
}

fn is_lower_hex_hash(value: Option<&str>) -> bool {
    value.is_some_and(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, reasoning::progress::Settings, store::Store};

    const PATH_QUERY: &str =
        r#"path:{"nodes":["s","g"],"edges":[["s","g"]],"start":"s","goal":"g"}"#;
    const CSP_QUERY: &str = r#"csp:{"variables":[{"id":"x","domain":[1]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1}]}"#;

    fn fixture(query: &str, marked: bool) -> (Store, String, Config) {
        let store = Store::in_memory().unwrap();
        let task = store.create_task(query, "research").unwrap();
        let cfg = Config {
            progress_stopping: Some(Settings { plateau_cycles: 2 }),
            ..Config::default()
        };
        let mut execution = json!({"configuration":cfg});
        if marked {
            execution["progress_policy"] = json!(POLICY);
        }
        store
            .event(&task, "execution_settings", "runtime", execution)
            .unwrap();
        let mut context = json!({"format_version":1});
        if marked {
            context["progress_policy"] = json!(POLICY);
        }
        store
            .event(&task, "reasoning_context", "jev-r", context)
            .unwrap();
        (store, task, cfg)
    }

    #[test]
    fn duplicate_cycle_zero_saved_result_proofs_cannot_reuse_one_solver_call() {
        let (store, task, cfg) = fixture(PATH_QUERY, true);
        let answer = r#"["g"]"#;
        let answer_hash = sha256(answer.as_bytes());
        let reason = crate::verification::Criterion::from_query(PATH_QUERY)
            .unwrap()
            .unwrap()
            .check(answer)
            .unwrap_err()
            .to_string();
        store
            .event(
                &task,
                "reasoning",
                "jev-r",
                json!({
                    "call":1,
                    "node":{"id":0,"parent":null,"role":"solver","cycle":0,"model":"fixture","content_hash":answer_hash},
                    "text":answer
                }),
            )
            .unwrap();
        let proof = json!({
            "criterion":CRITERION,
            "verified":false,
            "origin":"saved_solver_recovery",
            "objective_hash":sha256(PATH_QUERY.as_bytes()),
            "answer_hash":answer_hash,
            "reason":reason,
            "cycle":0,
            "call":1
        });
        store
            .event(&task, "verification", "verifier-r", proof.clone())
            .unwrap();
        store
            .event(&task, "verification", "verifier-r", proof)
            .unwrap();

        let error = validate_before(
            &store.lock().unwrap(),
            &task,
            PATH_QUERY,
            &cfg,
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("Duplicate cycle-zero shortest-path saved-result verification"));
    }

    #[test]
    fn missing_or_mismatched_progress_criterion_is_rejected_for_path_tasks() {
        for marked in [false, true] {
            for kind in ["reasoning_progress", "reasoning_progress_provider_veto"] {
                for payload in [
                    json!({"cycle":1}),
                    json!({"criterion":"finite_domain/v1","cycle":1}),
                ] {
                    let (store, task, cfg) = fixture(PATH_QUERY, marked);
                    store.event(&task, kind, "verifier-r", payload).unwrap();
                    assert!(
                        validate_before(
                            &store.lock().unwrap(),
                            &task,
                            PATH_QUERY,
                            &cfg,
                            None,
                            None,
                            None,
                        )
                        .is_err(),
                        "accepted marked={marked}, kind={kind}"
                    );
                }
            }
        }
    }

    #[test]
    fn finite_domain_progress_records_are_left_to_the_existing_validator() {
        let (store, task, cfg) = fixture(CSP_QUERY, false);
        store
            .event(
                &task,
                "reasoning_progress_provider_veto",
                "verifier-r",
                json!({"criterion":"finite_domain/v1","cycle":1}),
            )
            .unwrap();
        assert!(validate_before(
            &store.lock().unwrap(),
            &task,
            CSP_QUERY,
            &cfg,
            None,
            None,
            None
        )
        .unwrap()
        .is_none());
    }
}
