//! Original configured resource charges must form complete, matching call chains.
use crate::{
    backend::{InferenceTarget, ModelTarget, NativeModelTarget},
    config::Config,
    resource_scheduler::{ResourcePool, WorkingProfile},
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

struct CallHistory {
    role: String,
    pool: String,
    stage: u8,
    target: Option<InferenceTarget>,
    profile: Option<WorkingProfile>,
}

fn target_from_value(value: &Value) -> Result<InferenceTarget> {
    let target = match value["kind"].as_str() {
        Some("deterministic") if value.get("target").is_none() => InferenceTarget::Deterministic,
        Some("local_http") => {
            let target: ModelTarget = serde_json::from_value(value["target"].clone())?;
            let canonical = target.canonical()?;
            if canonical != target {
                bail!("Original resource target is not canonical")
            }
            InferenceTarget::LocalHttp(target)
        }
        Some("native") => {
            let target: NativeModelTarget = serde_json::from_value(value["target"].clone())?;
            let canonical = target.canonical()?;
            if canonical != target
                || value.as_object().is_none_or(|object| object.len() != 2)
                || value["target"]
                    .as_object()
                    .is_none_or(|object| object.len() != 3)
            {
                bail!("Original native resource target is not canonical")
            }
            InferenceTarget::Native(target)
        }
        _ => bail!("Unsupported original resource target"),
    };
    if serde_json::to_value(&target)? != *value {
        bail!("Original resource target shape changed")
    }
    Ok(target)
}

fn expected_allocation(
    configuration: &crate::resource_scheduler::Configuration,
    profile: Option<&WorkingProfile>,
) -> crate::resource_scheduler::Allocation {
    profile
        .map(|profile| profile.allocation)
        .unwrap_or(configuration.allocation)
}

/// Validate independent bounded reservation chains. Multiple model calls may
/// legitimately overlap in Jev-R waves, so event chronology is checked per
/// reservation and each chain start consumes a prior same-role model dispatch.
pub(super) fn validate(db: &rusqlite::Connection, task: &str, cfg: &Config) -> Result<Value> {
    let mut statement=db.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND (kind IN ('model_dispatch','reasoning_context') OR kind GLOB 'resource_*') ORDER BY id LIMIT 769")?;
    let mut rows = statement.query([task])?;
    let configuration = cfg.resource_admission.as_ref();
    let binding = configuration
        .map(ResourcePool::configuration_binding)
        .transpose()?;
    let mut histories = BTreeMap::<String, CallHistory>::new();
    let mut outstanding_dispatches = BTreeMap::<String, usize>::new();
    let mut pools = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut count = 0;
    let mut calls = 0;
    let mut in_reasoning = false;
    let nonempty_residents = configuration.is_some_and(|c| !c.residents.is_empty());
    let profiled = configuration.is_some_and(|c| !c.working_profiles.is_empty());
    let mut seen_context = false;

    if profiled
        && !binding
            .as_ref()
            .is_some_and(|value| value["working_profiles_sha256"].as_str().is_some())
    {
        bail!("Original working-profile catalog has no stable digest")
    }

    let profile_events = u8::from(profiled) * 2;
    let target_events = u8::from(nonempty_residents) * 2;
    let resource_intent_stage = profile_events + target_events;
    let resource_complete_stage = resource_intent_stage + 4;

    while let Some(row) = rows.next()? {
        count += 1;
        if count > 768 {
            bail!("Resource continuation history exceeds bounds")
        }
        let kind: String = row.get(0)?;
        let source: String = row.get(1)?;
        if kind == "reasoning_context" {
            if !outstanding_dispatches.values().all(|pending| *pending == 0)
                || histories
                    .values()
                    .any(|history| history.stage != resource_complete_stage)
            {
                bail!("Unfinished resource calls cross a reasoning-context boundary")
            }
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
            if source != "jev-r" {
                bail!("Invalid original model dispatch source")
            }
            calls += 1;
            if calls > 64 {
                bail!("Excessive original resource calls")
            }
            let role = value["role"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing resource role"))?;
            crate::fabric::protocol::actor(role)?;
            *outstanding_dispatches.entry(role.to_string()).or_default() += 1;
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

        if !histories.contains_key(id) {
            let start_kind = if profiled {
                "resource_profile_intent"
            } else if nonempty_residents {
                "resource_target_intent"
            } else {
                "resource_intent"
            };
            if kind != start_kind || !ids.insert(id.to_string()) {
                bail!("Unexpected or duplicate resource call identity")
            }
            let role = value["role"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing resource role"))?;
            crate::fabric::protocol::actor(role)?;
            let pending = outstanding_dispatches.get_mut(role).ok_or_else(|| {
                anyhow::anyhow!("Resource reservation has no prior model dispatch")
            })?;
            if *pending == 0 {
                bail!("Resource reservation has no prior model dispatch")
            }
            *pending -= 1;
            pools.insert(pool_id.to_string());
            histories.insert(
                id.to_string(),
                CallHistory {
                    role: role.to_string(),
                    pool: pool_id.to_string(),
                    stage: 0,
                    target: None,
                    profile: None,
                },
            );
        }

        let history = histories.get_mut(id).expect("History was initialized");
        if history.pool != pool_id
            || value
                .get("role")
                .is_some_and(|recorded| recorded.as_str() != Some(history.role.as_str()))
        {
            bail!("Resource call identity changed within its audit chain")
        }
        let role = history.role.as_str();
        let expected_binding = binding.as_ref().unwrap();

        match kind.as_str() {
            "resource_profile_intent" if profiled && history.stage == 0 => {
                let expected = json!({
                    "format_version":1,
                    "pool_id":history.pool,
                    "reservation_id":id,
                    "role":role,
                    "working_profiles_sha256":expected_binding["working_profiles_sha256"],
                    "permissions_granted":false,
                });
                if value != expected {
                    bail!("Original working-profile intent changed")
                }
                history.stage = 1;
            }
            "resource_profile" if profiled && history.stage == 1 => {
                let target_value = &value["target"];
                let target = target_from_value(target_value)?;
                if in_reasoning
                    && !matches!(&target, InferenceTarget::Native(_))
                    && Some(target.clone()) != crate::backend::configured_target(cfg, Some(role))?
                {
                    bail!("Original reasoning resource target differs from pinned configuration")
                }
                let profile = ResourcePool::profile_for(cfg_resources, role, Some(&target))?
                    .ok_or_else(|| anyhow::anyhow!("Original working profile is unavailable"))?;
                let expected = json!({
                    "format_version":1,
                    "pool_id":history.pool,
                    "reservation_id":id,
                    "role":role,
                    "working_profiles_sha256":expected_binding["working_profiles_sha256"],
                    "allowed":true,
                    "reason_code":"configured_profile_match",
                    "target":target,
                    "profile_id":profile.id,
                    "allocation":profile.allocation,
                    "permissions_granted":false,
                    "physical_limits_enforced":false,
                });
                if value != expected {
                    bail!("Original working-profile decision changed")
                }
                history.target = Some(target);
                history.profile = Some(profile);
                history.stage = 2;
            }
            "resource_target_intent" if nonempty_residents && history.stage == profile_events => {
                let expected = json!({
                    "format_version":1,
                    "pool_id":history.pool,
                    "reservation_id":id,
                    "role":role,
                    "resident_catalog_sha256":expected_binding["resident_catalog_sha256"],
                    "permissions_granted":false,
                });
                if value != expected {
                    bail!("Original resource target intent changed")
                }
                history.stage += 1;
            }
            "resource_target" if nonempty_residents && history.stage == profile_events + 1 => {
                let target = target_from_value(&value["coverage"]["target"])?;
                if in_reasoning
                    && !matches!(&target, InferenceTarget::Native(_))
                    && Some(target.clone()) != crate::backend::configured_target(cfg, Some(role))?
                {
                    bail!("Original reasoning resource target differs from pinned configuration")
                }
                if history
                    .target
                    .as_ref()
                    .is_some_and(|profile_target| profile_target != &target)
                {
                    bail!("Original resident target differs from its working profile")
                }
                let coverage =
                    ResourcePool::configuration_coverage(cfg_resources, Some(target.clone()))?;
                let expected = json!({
                    "format_version":1,
                    "pool_id":history.pool,
                    "reservation_id":id,
                    "role":role,
                    "resident_catalog_sha256":expected_binding["resident_catalog_sha256"],
                    "allowed":true,
                    "coverage":coverage,
                    "reason_code":"configured_target_covered",
                    "permissions_granted":false,
                    "weights_attested":false,
                });
                if value != expected {
                    bail!("Original resource coverage is missing or changed")
                }
                history.target = Some(target);
                history.stage += 1;
            }
            "resource_intent" if history.stage == resource_intent_stage => {
                cfg_resources
                    .settings
                    .validate_queue_audit(role, &value, false)?;
                let allocation = expected_allocation(cfg_resources, history.profile.as_ref());
                if value["settings"] != serde_json::to_value(&cfg_resources.settings)?
                    || value["allocation"] != serde_json::to_value(allocation)?
                    || value["role"] != role
                    || value["permissions_granted"] != false
                    || value["physical_limits_enforced"] != false
                {
                    bail!("Original resource allocation differs from its pinned configuration")
                }
                let legacy_empty = !nonempty_residents
                    && !profiled
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
                history.stage += 1;
            }
            "resource_admitted" if history.stage == resource_intent_stage + 1 => {
                cfg_resources
                    .settings
                    .validate_queue_audit(role, &value, true)?;
                let allocation = expected_allocation(cfg_resources, history.profile.as_ref());
                if value["role"] != role
                    || value["allocation"] != serde_json::to_value(allocation)?
                    || value["provider_dispatched"] != false
                    || value["wait_ms"]
                        .as_f64()
                        .is_none_or(|n| !n.is_finite() || !(0.0..=300_000.0).contains(&n))
                {
                    bail!("Invalid original resource admission")
                }
                history.stage += 1;
            }
            "resource_dispatch"
                if history.stage == resource_intent_stage + 2
                    && value["provider_stop_proven"] == false =>
            {
                history.stage += 1
            }
            "resource_completed"
                if history.stage == resource_intent_stage + 3
                    && value["provider_request_completed"] == true
                    && value["physical_limits_enforced"] == false =>
            {
                history.stage += 1
            }
            _ => bail!("Unsupported, incomplete or reordered original resource call"),
        }
    }

    if configuration.is_none() {
        return Ok(Value::Null);
    }
    if !seen_context
        || !outstanding_dispatches.values().all(|pending| *pending == 0)
        || histories.len() != calls
        || histories
            .values()
            .any(|history| history.stage != resource_complete_stage)
    {
        bail!("Original configured resource history is incomplete")
    }
    ResourcePool::validate_resume_ledger(db)?;
    Ok(json!({
        "configured":true,
        "original_pool_ids":pools,
        "completed_calls":calls,
        "binding":binding.unwrap(),
        "permissions_granted":false,
        "physical_limits_enforced":false
    }))
}
