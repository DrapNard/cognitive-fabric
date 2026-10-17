//! Original configured resource charges must form complete, matching call chains.
use crate::{
    backend::{InferenceTarget, ModelTarget},
    config::Config,
    resource_scheduler::ResourcePool,
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(super) fn validate(db: &rusqlite::Connection, task: &str, cfg: &Config) -> Result<Value> {
    let mut statement=db.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND (kind IN ('model_dispatch','reasoning_context') OR kind GLOB 'resource_*') ORDER BY id LIMIT 769")?;
    let mut rows = statement.query([task])?;
    let configuration = cfg.resource_admission.as_ref();
    let binding = configuration
        .map(ResourcePool::configuration_binding)
        .transpose()?;
    let mut stage = None;
    let mut role = String::new();
    let mut request = String::new();
    let mut pool = String::new();
    let mut pools = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut count = 0;
    let mut calls = 0;
    let mut in_reasoning = false;
    let nonempty = configuration.is_some_and(|c| !c.residents.is_empty());
    let mut seen_context = false;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 768 {
            bail!("Resource continuation history exceeds bounds")
        }
        let kind: String = row.get(0)?;
        let source: String = row.get(1)?;
        // Other checkpoint validation already bounds large context separately.
        if kind == "reasoning_context" {
            seen_context = true;
            in_reasoning = true;
            continue;
        }
        if kind == "model_dispatch" && configuration.is_none() {
            continue;
        }
        let bytes: String = row.get(2)?;
        let value = crate::strict_json::value(bytes.as_bytes(), 16384)?;
        if kind == "model_dispatch" {
            if stage.is_some_and(|n| n != 6) {
                bail!("Original resource call has an incomplete outcome")
            }
            if source != "jev-r" {
                bail!("Invalid original model dispatch source")
            }
            calls += 1;
            if calls > 64 {
                bail!("Excessive original resource calls")
            }
            role = value["role"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing resource role"))?
                .into();
            crate::fabric::protocol::actor(&role)?;
            stage = Some(0);
            request.clear();
            pool.clear();
            continue;
        }
        let cfg_resources = configuration.ok_or_else(|| {
            anyhow::anyhow!("Original shared resource pool requires explicit host restoration")
        })?;
        if source != "jev-plus" || value["format_version"] != 1 {
            bail!("Invalid original resource audit source or version")
        }
        let id = value["reservation_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing resource identity"))?;
        let pool_id = value["pool_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing resource pool identity"))?;
        uuid::Uuid::parse_str(id)?;
        uuid::Uuid::parse_str(pool_id)?;
        if request.is_empty() {
            if stage != Some(0) || !ids.insert(id.to_string()) {
                bail!("Unexpected or duplicate resource call identity")
            }
            request = id.into();
            pool = pool_id.into();
            pools.insert(pool.clone());
        }
        if id != request || pool_id != pool {
            bail!("Resource call identity changed within its audit chain")
        }
        let expected_binding = binding.as_ref().unwrap();
        match kind.as_str() {
            "resource_target_intent" if stage == Some(0) && nonempty => {
                if value["role"] != role
                    || value["resident_catalog_sha256"]
                        != expected_binding["resident_catalog_sha256"]
                    || value["permissions_granted"] != false
                {
                    bail!("Original resource target intent changed")
                }
                stage = Some(1);
            }
            "resource_target" if stage == Some(1) && nonempty => {
                let recorded = &value["coverage"]["target"];
                let target = match recorded["kind"].as_str() {
                    Some("deterministic") => InferenceTarget::Deterministic,
                    Some("local_http") => {
                        InferenceTarget::LocalHttp(serde_json::from_value::<ModelTarget>(
                            recorded["target"].clone(),
                        )?)
                    }
                    _ => bail!("Unsupported original resource target"),
                };
                if in_reasoning
                    && Some(target.clone()) != crate::backend::configured_target(cfg, Some(&role))?
                {
                    bail!("Original reasoning resource target differs from pinned configuration")
                }
                let coverage = ResourcePool::configuration_coverage(cfg_resources, Some(target))?;
                if value["coverage"] != coverage
                    || value["role"] != role
                    || value["resident_catalog_sha256"]
                        != expected_binding["resident_catalog_sha256"]
                    || value["allowed"] != true
                    || value["permissions_granted"] != false
                    || value["weights_attested"] != false
                    || value["reason_code"] != "configured_target_covered"
                {
                    bail!("Original resource coverage is missing or changed")
                }
                stage = Some(2);
            }
            "resource_intent" if stage == Some(if nonempty { 2 } else { 0 }) => {
                cfg_resources
                    .settings
                    .validate_queue_audit(&role, &value, false)?;
                if value["settings"] != serde_json::to_value(&cfg_resources.settings)?
                    || value["allocation"] != serde_json::to_value(cfg_resources.allocation)?
                    || value["role"] != role
                    || value["permissions_granted"] != false
                    || value["physical_limits_enforced"] != false
                {
                    bail!("Original resource allocation differs from its pinned configuration")
                }
                let legacy_empty = !nonempty
                    && [
                        "resident_allocated",
                        "resident_catalog_sha256",
                        "residency_attested",
                    ]
                    .iter()
                    .all(|key| value.get(*key).is_none());
                if !legacy_empty
                    && [
                        "resident_allocated",
                        "resident_catalog_sha256",
                        "residency_attested",
                    ]
                    .iter()
                    .any(|key| value[*key] != expected_binding[*key])
                {
                    bail!("Original resident allocation binding changed")
                }
                stage = Some(3);
            }
            "resource_admitted" if stage == Some(3) => {
                cfg_resources
                    .settings
                    .validate_queue_audit(&role, &value, true)?;
                if value["role"] != role
                    || value["allocation"] != serde_json::to_value(cfg_resources.allocation)?
                    || value["provider_dispatched"] != false
                    || value["wait_ms"]
                        .as_f64()
                        .is_none_or(|n| !n.is_finite() || !(0.0..=300_000.0).contains(&n))
                {
                    bail!("Invalid original resource admission")
                }
                stage = Some(4);
            }
            "resource_dispatch" if stage == Some(4) && value["provider_stop_proven"] == false => {
                stage = Some(5)
            }
            "resource_completed"
                if stage == Some(5)
                    && value["provider_request_completed"] == true
                    && value["physical_limits_enforced"] == false =>
            {
                stage = Some(6)
            }
            _ => bail!("Unsupported, incomplete or reordered original resource call"),
        }
    }
    if configuration.is_none() {
        return Ok(Value::Null);
    }
    if !seen_context || stage.is_some_and(|n| n != 6) {
        bail!("Original configured resource history is incomplete")
    }
    ResourcePool::validate_resume_ledger(db)?;
    Ok(
        json!({"configured":true,"original_pool_ids":pools,"completed_calls":calls,"binding":binding.unwrap(),"permissions_granted":false,"physical_limits_enforced":false}),
    )
}
