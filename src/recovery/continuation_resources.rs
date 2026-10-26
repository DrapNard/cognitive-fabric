//! Original configured resource charges must form complete, matching call chains.
use crate::{
    backend::{InferenceTarget, ManagedNativeTarget, ModelTarget, NativeModelTarget},
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
        Some("managed_native") => {
            let target: ManagedNativeTarget = serde_json::from_value(value["target"].clone())?;
            let canonical = target.canonical()?;
            if canonical != target
                || value.as_object().is_none_or(|object| object.len() != 2)
                || value["target"]
                    .as_object()
                    .is_none_or(|object| object.len() != 3)
            {
                bail!("Original managed native resource target is not canonical")
            }
            InferenceTarget::ManagedNative(target)
        }
        _ => bail!("Unsupported original resource target"),
    };
    if serde_json::to_value(&target)? != *value {
        bail!("Original resource target shape changed")
    }
    Ok(target)
}

fn validate_managed_native_scope(
    db: &rusqlite::Connection,
    task: &str,
    target: &ManagedNativeTarget,
) -> Result<()> {
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END, CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND kind='native_task_scope' ORDER BY id LIMIT 2",
    )?;
    let mut rows = statement.query([task])?;
    let mut found = None;
    while let Some(row) = rows.next()? {
        if found.is_some() {
            bail!("Repeated native task scope")
        }
        let source: String = row.get(0)?;
        let raw: String = row.get(1)?;
        let scope = crate::strict_json::value(raw.as_bytes(), 16_384)?;
        found = Some((source, scope));
    }
    let (source, scope) = found.ok_or_else(|| anyhow::anyhow!("Missing lazy native task scope"))?;
    if source != "security-kernel"
        || scope["format_version"] != 2
        || scope["activation"] != "on_demand"
        || scope["policy_sha256"] != target.policy_sha256
        || scope["instance_binding_required"] != true
        || scope["permissions_granted"] != false
    {
        bail!("Managed native route differs from its original lazy lifecycle scope")
    }
    Ok(())
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
    validate_phase(db, task, cfg, true)
}

/// A terminal checkpoint can follow independently checked inline tools without
/// ever entering a whole-goal reasoning context. All call chains must still end.
pub(in crate::recovery) fn validate_terminal(
    db: &rusqlite::Connection,
    task: &str,
    cfg: &Config,
) -> Result<Value> {
    validate_phase(db, task, cfg, false)
}

fn validate_phase(
    db: &rusqlite::Connection,
    task: &str,
    cfg: &Config,
    require_context: bool,
) -> Result<Value> {
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
    let gpu_observation = configuration.and_then(|resources| resources.gpu_observation.as_ref());
    let gpu_observation_events = u8::from(gpu_observation.is_some());
    let resource_complete_stage = resource_intent_stage + 4 + gpu_observation_events;

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
                if let InferenceTarget::ManagedNative(managed) = &target {
                    validate_managed_native_scope(db, task, managed)?;
                }
                if in_reasoning
                    && !matches!(
                        &target,
                        InferenceTarget::Native(_) | InferenceTarget::ManagedNative(_)
                    )
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
                if let InferenceTarget::ManagedNative(managed) = &target {
                    validate_managed_native_scope(db, task, managed)?;
                }
                if in_reasoning
                    && !matches!(
                        &target,
                        InferenceTarget::Native(_) | InferenceTarget::ManagedNative(_)
                    )
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
                match &cfg_resources.gpu_observation {
                    Some(expected)
                        if value["gpu_observation"] == serde_json::to_value(expected)? => {}
                    None if value.get("gpu_observation").is_none() => {}
                    _ => bail!("Original GPU observation admission changed"),
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
            "resource_gpu_observation"
                if gpu_observation.is_some() && history.stage == resource_intent_stage + 1 =>
            {
                let configured = gpu_observation.unwrap();
                let allocation = expected_allocation(cfg_resources, history.profile.as_ref());
                let expected_minimum = allocation
                    .device_memory_bytes
                    .checked_add(configured.headroom_bytes)
                    .ok_or_else(|| anyhow::anyhow!("GPU observation threshold overflow"))?;
                let total = value["memory_total_bytes"]
                    .as_u64()
                    .filter(|bytes| *bytes > 0 && *bytes <= (1 << 60))
                    .ok_or_else(|| anyhow::anyhow!("Invalid saved GPU memory total"))?;
                let used = value["memory_used_bytes"]
                    .as_u64()
                    .filter(|bytes| *bytes <= total)
                    .ok_or_else(|| anyhow::anyhow!("Invalid saved GPU memory use"))?;
                let available = total - used;
                let query_duration_ms = value["sample_query_elapsed_ms"]
                    .as_f64()
                    .filter(|ms| ms.is_finite() && (0.0..=5000.0).contains(ms))
                    .ok_or_else(|| anyhow::anyhow!("Invalid saved GPU query duration"))?;
                let sample_age_ms = value["sample_age_ms_at_observation"]
                    .as_f64()
                    .filter(|ms| ms.is_finite() && (0.0..=2000.0).contains(ms))
                    .ok_or_else(|| anyhow::anyhow!("Invalid saved GPU sample age"))?;
                let exact_keys = [
                    "format_version",
                    "pool_id",
                    "reservation_id",
                    "role",
                    "sensor",
                    "device_uuid",
                    "allocation",
                    "headroom_bytes",
                    "memory_total_bytes",
                    "memory_used_bytes",
                    "memory_available_bytes",
                    "minimum_available_bytes",
                    "sample_query_elapsed_ms",
                    "sample_age_ms_at_observation",
                    "sampled",
                    "provider_dispatched",
                    "physical_limits_enforced",
                ];
                if value.as_object().is_none_or(|object| {
                    object.len() != exact_keys.len()
                        || exact_keys.iter().any(|key| !object.contains_key(*key))
                }) || value["sensor"] != configured.sensor.name()
                    || value["device_uuid"] != configured.device_uuid
                    || value["allocation"] != serde_json::to_value(allocation)?
                    || value["headroom_bytes"] != configured.headroom_bytes
                    || value["memory_available_bytes"] != available
                    || value["minimum_available_bytes"] != expected_minimum
                    || available < expected_minimum
                    || value["sampled"] != true
                    || value["provider_dispatched"] != false
                    || value["physical_limits_enforced"] != false
                    || query_duration_ms > 5000.0
                    || sample_age_ms > 2000.0
                {
                    bail!("Original GPU observation decision is invalid or changed")
                }
                history.stage += 1;
            }
            "resource_admitted"
                if history.stage == resource_intent_stage + 1 + gpu_observation_events =>
            {
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
                if history.stage == resource_intent_stage + 2 + gpu_observation_events
                    && value["provider_stop_proven"] == false =>
            {
                match gpu_observation {
                    Some(_) => {
                        let age_ms = value["sample_age_ms_at_dispatch_audit"]
                            .as_f64()
                            .filter(|ms| ms.is_finite() && (0.0..=2000.0).contains(ms))
                            .ok_or_else(|| {
                                anyhow::anyhow!("Original GPU sample was stale at dispatch audit")
                            })?;
                        if value.as_object().is_none_or(|object| object.len() != 5)
                            || age_ms > 2000.0
                        {
                            bail!("Original GPU dispatch audit is invalid")
                        }
                    }
                    None if value.get("sample_age_ms_at_dispatch_audit").is_none() => {}
                    None => bail!("Unexpected GPU sample in an unguarded dispatch"),
                }
                history.stage += 1;
            }
            "resource_completed"
                if history.stage == resource_intent_stage + 3 + gpu_observation_events
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
    if (require_context && !seen_context)
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

#[cfg(test)]
mod gpu_observation_tests {
    use super::*;
    use crate::{
        resource_scheduler::{Allocation, Configuration, GpuObservationAdmission, Settings},
        store::Store,
    };

    fn fixture(guarded: bool, include_observation: bool) -> (Store, String, Config) {
        let store = Store::in_memory().unwrap();
        let task = store
            .create_task("GPU continuation fixture", "standard")
            .unwrap();
        let allocation = Allocation {
            compute_slots: 1,
            host_memory_bytes: 64,
            device_memory_bytes: 100,
        };
        let settings = Settings {
            label: "gpu.resume".into(),
            capacity: Allocation {
                compute_slots: 1,
                host_memory_bytes: 1024,
                device_memory_bytes: 2048,
            },
            max_pending: 4,
            max_wait_ms: 1000,
            max_execution_ms: 5000,
            scheduling: None,
        };
        let gpu_observation = guarded.then_some(GpuObservationAdmission {
            device_uuid: "GPU-deadbeef".into(),
            sensor: crate::gpu::Sensor::NvidiaSmi,
            headroom_bytes: 10,
        });
        let resources = Configuration {
            settings: settings.clone(),
            allocation,
            residents: Vec::new(),
            working_profiles: Vec::new(),
            gpu_observation: gpu_observation.clone(),
        };
        let cfg = Config {
            resource_admission: Some(resources.clone()),
            ..Default::default()
        };
        let binding = ResourcePool::configuration_binding(&resources).unwrap();
        let pool_id = uuid::Uuid::new_v4().to_string();
        let reservation_id = uuid::Uuid::new_v4().to_string();
        store
            .event(&task, "model_dispatch", "jev-r", json!({"role":"solver"}))
            .unwrap();
        let mut intent = json!({
            "format_version":1,"pool_id":pool_id,"reservation_id":reservation_id,
            "settings":settings,"allocation":allocation,"role":"solver",
            "permissions_granted":false,"resident_allocated":binding["resident_allocated"],
            "resident_catalog_sha256":binding["resident_catalog_sha256"],
            "residency_attested":false,"physical_limits_enforced":false,
        });
        if let Some(observation) = &gpu_observation {
            intent["gpu_observation"] = serde_json::to_value(observation).unwrap();
        }
        store
            .event(&task, "resource_intent", "jev-plus", intent)
            .unwrap();
        if include_observation {
            store
                .event(
                    &task,
                    "resource_gpu_observation",
                    "jev-plus",
                    json!({
                        "format_version":1,"pool_id":pool_id,"reservation_id":reservation_id,
                        "role":"solver","sensor":"nvidia-smi","device_uuid":"GPU-deadbeef",
                        "allocation":allocation,"headroom_bytes":10,"memory_total_bytes":1000,
                        "memory_used_bytes":100,"memory_available_bytes":900,"minimum_available_bytes":110,
                        "sample_query_elapsed_ms":12.0,"sample_age_ms_at_observation":0.01,
                        "sampled":true,"provider_dispatched":false,"physical_limits_enforced":false,
                    }),
                )
                .unwrap();
        }
        store
            .event(
                &task,
                "resource_admitted",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":pool_id,"reservation_id":reservation_id,
                    "allocation":allocation,"role":"solver","wait_ms":1.0,"provider_dispatched":false,
                }),
            )
            .unwrap();
        let mut dispatch = json!({
            "format_version":1,"pool_id":pool_id,"reservation_id":reservation_id,
            "provider_stop_proven":false,
        });
        if guarded {
            dispatch["sample_age_ms_at_dispatch_audit"] = json!(0.2);
        }
        store
            .event(&task, "resource_dispatch", "jev-plus", dispatch)
            .unwrap();
        store
            .event(
                &task,
                "resource_completed",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":pool_id,"reservation_id":reservation_id,
                    "provider_request_completed":true,"physical_limits_enforced":false,
                }),
            )
            .unwrap();
        store
            .event(
                &task,
                "reasoning_context",
                "jev-r",
                json!({"format_version":1}),
            )
            .unwrap();
        (store, task, cfg)
    }

    fn validate_fixture(store: &Store, task: &str, cfg: &Config) -> Result<Value> {
        let db = store.lock()?;
        validate(&db, task, cfg)
    }

    #[test]
    fn guarded_and_legacy_resource_histories_resume_only_with_exact_observation_chain() {
        let (store, task, cfg) = fixture(true, true);
        assert!(validate_fixture(&store, &task, &cfg).is_ok());

        let (store, task, cfg) = fixture(false, false);
        assert!(validate_fixture(&store, &task, &cfg).is_ok());

        let (store, task, cfg) = fixture(true, false);
        assert!(validate_fixture(&store, &task, &cfg).is_err());

        let (store, task, cfg) = fixture(false, true);
        assert!(validate_fixture(&store, &task, &cfg).is_err());
    }

    #[test]
    fn guarded_resource_history_rejects_mutated_gpu_observation_evidence() {
        for mutation in 0..7 {
            let (store, task, cfg) = fixture(true, true);
            let db = store.lock().unwrap();
            let raw: String = db
                .query_row(
                    "SELECT payload FROM events WHERE task_id=?1 AND kind='resource_gpu_observation'",
                    [&task],
                    |row| row.get(0),
                )
                .unwrap();
            let mut observation: Value = serde_json::from_str(&raw).unwrap();
            match mutation {
                0 => observation["device_uuid"] = json!("GPU-other"),
                1 => observation["sensor"] = json!("nvml"),
                2 => observation["headroom_bytes"] = json!(11),
                3 => observation["allocation"]["device_memory_bytes"] = json!(101),
                4 => observation["minimum_available_bytes"] = json!(109),
                5 => observation["memory_used_bytes"] = json!(950),
                _ => observation["unexpected"] = json!(true),
            }
            db.execute(
                "UPDATE events SET payload=?1 WHERE task_id=?2 AND kind='resource_gpu_observation'",
                rusqlite::params![observation.to_string(), task],
            )
            .unwrap();
            drop(db);
            assert!(validate_fixture(&store, &task, &cfg).is_err());
        }
    }

    #[test]
    fn dispatch_sample_age_is_required_only_for_guarded_resource_history() {
        for mutation in 0..3 {
            let guarded = mutation != 2;
            let (store, task, cfg) = fixture(guarded, guarded);
            let db = store.lock().unwrap();
            let raw: String = db
                .query_row(
                    "SELECT payload FROM events WHERE task_id=?1 AND kind='resource_dispatch'",
                    [&task],
                    |row| row.get(0),
                )
                .unwrap();
            let mut dispatch: Value = serde_json::from_str(&raw).unwrap();
            match mutation {
                0 => {
                    dispatch
                        .as_object_mut()
                        .unwrap()
                        .remove("sample_age_ms_at_dispatch_audit");
                }
                1 => dispatch["sample_age_ms_at_dispatch_audit"] = json!(2000.01),
                _ => dispatch["sample_age_ms_at_dispatch_audit"] = json!(0.1),
            };
            db.execute(
                "UPDATE events SET payload=?1 WHERE task_id=?2 AND kind='resource_dispatch'",
                rusqlite::params![dispatch.to_string(), task],
            )
            .unwrap();
            drop(db);
            assert!(validate_fixture(&store, &task, &cfg).is_err());
        }
    }
}
