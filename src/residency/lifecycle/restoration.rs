//! Read-only native restoration preflight. No provider mutation or task replay.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

struct Original {
    parent: String,
    binding_event: i64,
    binding_sha256: String,
    receipts: BTreeMap<String, managed::Receipt>,
    clock: crate::execution_clock::Snapshot,
    cfg: Config,
}

fn one_event(db: &rusqlite::Connection, task: &str, kind: &str, source: &str) -> Result<Value> {
    let mut statement = db.prepare("SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT 2")?;
    let rows = statement
        .query_map(rusqlite::params![task, kind, source], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 1 {
        bail!("Native restoration requires one original {kind} event")
    }
    crate::strict_json::value(rows[0].as_bytes(), 262144)
}

fn read_original(
    db: &rusqlite::Connection,
    task: &str,
    policy: &NativeLifecyclePolicy,
) -> Result<Original> {
    let state: String = db.query_row("SELECT status FROM tasks WHERE id=?1", [task], |row| {
        row.get(0)
    })?;
    if !matches!(state.as_str(), "running" | "interrupted") {
        bail!("Native restoration requires an unfinished original task")
    }
    let policy_hash = crate::store::sha256(&serde_json::to_vec(policy)?);
    let scope = one_event(db, task, "native_task_scope", "security-kernel")?;
    if scope["format_version"] != 1
        || scope["instance_binding_required"] != true
        || scope["permissions_granted"] != false
        || scope["policy_sha256"] != policy_hash
    {
        bail!("Native restoration policy differs from original task scope")
    }
    let settings = one_event(db, task, "execution_settings", "runtime")?;
    let cfg: Config = serde_json::from_value(settings["configuration"].clone())?;
    cfg.validate()?;
    if cfg.inference_process {
        bail!("Native restoration cannot replace an inference process backend")
    }
    let clock: crate::execution_clock::Snapshot =
        serde_json::from_value(settings["execution_clock"].clone())?;
    clock.validate(cfg.max_wall_seconds)?;
    remaining(&clock)?;

    // Bound the entire owner-local binding history before matching child IDs.
    // A partial scan must never establish uniqueness or select a newer binding.
    let mut statement = db.prepare("SELECT task_id,id,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE kind='native_task_bindings' AND source='human-cli' ORDER BY id LIMIT 4097")?;
    let mut rows = statement.query([])?;
    let mut found = None;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 4096 {
            bail!("Native restoration binding history exceeds bounds")
        }
        let parent: String = row.get(0)?;
        let id: i64 = row.get(1)?;
        let raw: String = row.get(2)?;
        let binding = crate::strict_json::value(raw.as_bytes(), 16384)?;
        if binding["child_task_id"] != task {
            continue;
        }
        if found.is_some() {
            bail!("Native restoration has ambiguous lifecycle bindings")
        }
        uuid::Uuid::parse_str(&parent)?;
        let receipts: BTreeMap<String, managed::Receipt> =
            serde_json::from_value(binding["receipts"].clone())?;
        if binding["format_version"] != 1
            || binding["permissions_granted"] != false
            || binding["weights_attested"] != false
            || binding["response_instance_check_required"] != true
            || binding["policy_sha256"] != policy_hash
            || binding["bindings_sha256"] != crate::store::sha256(&serde_json::to_vec(&receipts)?)
            || receipts.len() != policy.models.len()
            || receipts.is_empty()
            || receipts.len() > 8
        {
            bail!("Native restoration binding identity differs from original policy")
        }
        let mut instances = BTreeSet::new();
        for model in &policy.models {
            let receipt = receipts
                .get(&model.model)
                .ok_or_else(|| anyhow::anyhow!("Native restoration model is missing"))?;
            if receipt.model_key != model.model
                || receipt.context_length != model.context_length
                || !instances.insert(receipt.instance_id.clone())
            {
                bail!("Native restoration model/context mapping changed")
            }
        }
        for role in crate::fabric::protocol::ROLES {
            let model = cfg
                .role_profiles
                .get(role)
                .map(|p| p.model.as_str())
                .unwrap_or(&cfg.model);
            if !receipts.contains_key(model) {
                bail!("Original native role has no approved receipt")
            }
        }
        let mut phases = db.prepare("SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind='native_task_lifecycle' AND source='human-cli' AND json_extract(payload,'$.state') IN ('generation_started','generation_confirmed','generation_operator_stopped') ORDER BY id LIMIT 3")?;
        let phases = phases
            .query_map([&parent], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if phases.len() != 2 {
            bail!("Original native generation lacks a unique completed admission")
        }
        for (index, raw) in phases.iter().enumerate() {
            let phase = crate::strict_json::value(raw.as_bytes(), 4096)?;
            if phase["format_version"] != 1
                || phase["endpoint"] != endpoint(&policy.api_base)?.to_string()
                || phase["child_task_id"] != task
                || phase["bindings_sha256"] != binding["bindings_sha256"]
                || phase["permissions_granted"] != false
                || phase["provider_stop_proven"] != false
                || (index == 0 && phase["state"] != "generation_started")
                || (index == 1
                    && phase["state"] != "generation_confirmed"
                    && phase["state"] != "generation_operator_stopped")
                || (phase["state"] == "generation_operator_stopped"
                    && (phase["provider_stop_acknowledged"] != true
                        || phase["original_outcome"] != "unknown"))
            {
                bail!("Original native generation differs from its bindings")
            }
        }
        found = Some(Original {
            parent,
            binding_event: id,
            binding_sha256: crate::store::sha256(raw.as_bytes()),
            receipts,
            clock: clock.clone(),
            cfg: cfg.clone(),
        });
    }
    found.ok_or_else(|| anyhow::anyhow!("Native restoration has no original lifecycle binding"))
}

fn remaining(clock: &crate::execution_clock::Snapshot) -> Result<u64> {
    clock.remaining()?["remaining_ns"]
        .as_u64()
        .filter(|ns| *ns > 0)
        .ok_or_else(|| anyhow::anyhow!("Original native task deadline is expired or unavailable"))
}

/// Inspect original native receipt identity under exclusive local ownership.
/// This GET-only preflight does not authorize continuation, restore callbacks,
/// acknowledge stopped providers, reset budgets, or load/release models.
pub async fn inspect(
    store: Arc<Store>,
    task: &str,
    policy: &NativeLifecyclePolicy,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    uuid::Uuid::parse_str(task)?;
    policy.validate()?;
    let native_endpoint = endpoint(&policy.api_base)?.to_string();
    let ownership = Ownership::acquire(store.clone())?;
    ownership.validate(&store)?;
    let started = Instant::now();
    let original = {
        let mut db = store.lock()?;
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(5)),
        );
        let value = (|| {
            let tx = db.transaction()?;
            read_original(&tx, task, policy)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        value?
    };
    require_no_unknown_generation(&store, &native_endpoint)?;
    let allowance = Duration::from_nanos(remaining(&original.clock)?)
        .min(Duration::from_secs(policy.max_lifecycle_seconds))
        .min(Duration::from_secs(30));
    let inventory = tokio::time::timeout(allowance, status(&policy.api_base, policy.authenticate))
        .await
        .map_err(|_| anyhow::anyhow!("Native restoration inventory deadline exhausted"))??;
    let inventory = parse(&serde_json::to_vec(&inventory)?)?;
    for receipt in original.receipts.values() {
        let observed = managed::validate_task_binding(
            &store,
            &receipt.receipt_id,
            &policy.api_base,
            &inventory,
        )?;
        if serde_json::to_value(observed)? != serde_json::to_value(receipt)? {
            bail!("Native restoration receipt changed")
        }
    }
    let rechecked = {
        let mut db = store.lock()?;
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(5)),
        );
        let value = (|| {
            let tx = db.transaction()?;
            read_original(&tx, task, policy)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        value?
    };
    if rechecked.binding_event != original.binding_event
        || rechecked.binding_sha256 != original.binding_sha256
        || serde_json::to_value(&rechecked.clock)? != serde_json::to_value(&original.clock)?
        || serde_json::to_value(&rechecked.cfg)? != serde_json::to_value(&original.cfg)?
    {
        bail!("Original native restoration state changed during inventory inspection")
    }
    require_no_unknown_generation(&store, &native_endpoint)?;
    let remaining_ns = remaining(&original.clock)?;
    Ok(
        json!({"format_version":1,"task_id":task,"lifecycle_task_id":original.parent,
        "binding_event_id":original.binding_event,"binding_event_sha256":original.binding_sha256,
        "inventory_sha256":planning::fingerprint(&inventory)?,"receipt_count":original.receipts.len(),
        "receipt_identity_matches":true,"original_remaining_ns":remaining_ns,
        "resume_authorized":false,"checkpoint_validated":false,"permissions_granted":false,
        "model_mutation_performed":false,"provider_stop_proven":false,"weights_attested":false,
        "automatic_replay":false}),
    )
}
