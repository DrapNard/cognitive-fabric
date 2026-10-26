//! Exact native restoration and durable continuation dispatch without replay.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

struct Original {
    parent: String,
    binding_event: i64,
    binding_sha256: String,
    receipts: BTreeMap<String, managed::Receipt>,
    clock: crate::execution_clock::Snapshot,
    cfg: Config,
    binding: Value,
    /// Digest of every bounded, task-local lazy model binding. Eager v1/v2
    /// tasks have no such rows and retain their original receipt-map digest.
    activation_bindings_sha256: Option<String>,
    activated_origins: BTreeMap<String, String>,
    lifecycle_remaining_ns: u64,
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
    let lazy = policy.format_version == 3;
    let expected_scope = if lazy { 2 } else { 1 };
    if scope["format_version"] != expected_scope
        || scope["instance_binding_required"] != true
        || scope["permissions_granted"] != false
        || scope["policy_sha256"] != policy_hash
        || (lazy && scope["activation"] != "on_demand")
        || (!lazy && scope.get("activation").is_some())
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
        let expected_binding_format = if lazy { 2 } else { 1 };
        if binding["format_version"] != expected_binding_format
            || binding["permissions_granted"] != false
            || binding["weights_attested"] != false
            || binding["response_instance_check_required"] != true
            || binding["policy_sha256"] != policy_hash
            || binding.get("activation").is_some()
            || binding["bindings_sha256"] != crate::store::sha256(&serde_json::to_vec(&receipts)?)
            || (lazy && !receipts.is_empty())
            || (!lazy && receipts.len() != policy.models.len())
            || (!lazy && receipts.is_empty())
            || receipts.len() > 8
        {
            bail!("Native restoration binding identity differs from original policy")
        }
        if lazy {
            let expected = [
                "format_version",
                "child_task_id",
                "request_sha256",
                "policy_sha256",
                "receipts",
                "bindings_sha256",
                "lifecycle_boot_id",
                "lifecycle_deadline_ns",
                "permissions_granted",
                "weights_attested",
                "response_instance_check_required",
            ]
            .into_iter()
            .collect::<BTreeSet<_>>();
            if binding.as_object().is_none_or(|object| {
                object.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected
            }) {
                bail!("Lazy native root binding has an unsupported shape")
            }
        }
        let lifecycle_remaining_ns = if lazy {
            lazy_lifecycle_remaining(&binding, &clock, policy.max_lifecycle_seconds)?
        } else {
            remaining(&clock)?
        };
        let (activation_receipts, activated_origins, activation_event_ids) = if lazy {
            read_lazy_activation_receipts(db, &parent, task, policy, &policy_hash, &cfg)?
        } else {
            (BTreeMap::new(), BTreeMap::new(), BTreeMap::new())
        };
        let activation_bindings_sha256 = if lazy {
            Some(activation_receipts_digest(db, &parent)?)
        } else {
            None
        };
        let receipts = if lazy { activation_receipts } else { receipts };
        let mut instances = BTreeSet::new();
        for (model_key, receipt) in &receipts {
            let model = policy
                .models
                .iter()
                .find(|model| model.model == *model_key)
                .ok_or_else(|| anyhow::anyhow!("Native restoration model is outside policy"))?;
            if receipt.model_key != model.model
                || receipt.context_length != model.context_length
                || receipt.offload_kv_cache_to_gpu != model.offload_kv_cache_to_gpu
                || !instances.insert(receipt.instance_id.clone())
            {
                bail!("Native restoration model/context mapping changed")
            }
        }
        for role in crate::fabric::protocol::ROLES {
            let (model, _) = cfg.model_settings(role)?;
            if !policy.models.iter().any(|approved| approved.model == model)
                || (!lazy && !receipts.contains_key(model))
            {
                bail!("Original native role has no approved receipt")
            }
        }
        if lazy {
            validate_lazy_lifecycle_history(
                db,
                &parent,
                task,
                &policy_hash,
                endpoint(&policy.api_base)?.as_str(),
                &receipts,
                &activated_origins,
                &activation_event_ids,
            )?;
        } else {
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
        }
        found = Some(Original {
            parent,
            binding_event: id,
            binding_sha256: crate::store::sha256(raw.as_bytes()),
            receipts,
            clock: clock.clone(),
            cfg: cfg.clone(),
            binding: binding.clone(),
            activation_bindings_sha256,
            activated_origins,
            lifecycle_remaining_ns,
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

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[allow(clippy::too_many_arguments)]
fn validate_lazy_lifecycle_history(
    db: &rusqlite::Connection,
    lifecycle_task: &str,
    child_task: &str,
    policy_sha256: &str,
    endpoint: &str,
    receipts: &BTreeMap<String, managed::Receipt>,
    origins: &BTreeMap<String, String>,
    activation_event_ids: &BTreeMap<String, i64>,
) -> Result<()> {
    let mut statement = db.prepare("SELECT id,kind,source,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND ((kind='native_task_lifecycle' AND source='human-cli') OR (kind='native_task_model_binding' AND source='security-kernel')) ORDER BY id LIMIT 4097")?;
    let rows = statement
        .query_map([lifecycle_task], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() || rows.len() > 4096 {
        bail!("Lazy native lifecycle history is absent or exceeds bounds")
    }
    let expected = [
        "format_version",
        "activation",
        "state",
        "endpoint",
        "child_task_id",
        "policy_sha256",
        "model_bindings_sha256",
        "model_key",
        "receipt_id",
        "call_id",
        "permissions_granted",
        "provider_stop_proven",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let mut allowed = expected.clone();
    allowed.insert("provider_stop_acknowledged");
    allowed.insert("original_outcome");
    let mut root_started = false;
    let mut root_active = false;
    let mut calls = BTreeMap::<String, (String, String, String, bool)>::new();
    let mut active_receipts = BTreeMap::<String, managed::Receipt>::new();
    let mut active_origins = BTreeMap::<String, String>::new();
    let mut activation_index = 0usize;
    let expected_activation_ids = activation_event_ids
        .values()
        .copied()
        .collect::<BTreeSet<_>>();
    if expected_activation_ids.len() != activation_event_ids.len() {
        bail!("Lazy native model activation event IDs are ambiguous")
    }
    for (event_id, kind, source, raw) in rows {
        if kind == "native_task_model_binding" {
            if source != "security-kernel" || !root_active {
                bail!("Lazy model binding is outside its original active lifecycle")
            }
            let value = crate::strict_json::value(raw.as_bytes(), 16384)?;
            let model = value["model_key"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Lazy model binding lacks a model key"))?;
            let receipt = receipts.get(model).ok_or_else(|| {
                anyhow::anyhow!("Lazy model binding is outside the verified receipt set")
            })?;
            let event_receipt: managed::Receipt = serde_json::from_value(value["receipt"].clone())?;
            let origin = value["origin"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Lazy model binding has no activation origin"))?;
            if activation_event_ids.get(model) != Some(&event_id)
                || serde_json::to_value(&event_receipt)? != serde_json::to_value(receipt)?
                || origins.get(model).is_none_or(|saved| saved != origin)
                || active_receipts
                    .insert(model.to_owned(), receipt.clone())
                    .is_some()
                || active_origins
                    .insert(model.to_owned(), origin.to_owned())
                    .is_some()
            {
                bail!("Lazy model activation event differs from its exact receipt binding")
            }
            activation_index += 1;
            continue;
        }
        if kind != "native_task_lifecycle" || source != "human-cli" {
            bail!("Unexpected event in lazy native lifecycle history")
        }
        let value = crate::strict_json::value(raw.as_bytes(), 4096)?;
        if value.as_object().is_none_or(|object| {
            let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
            !expected.is_subset(&keys) || keys.iter().any(|key| !allowed.contains(key))
        }) {
            bail!("Lazy native lifecycle event has an unsupported shape")
        }
        if value["format_version"] != 2
            || value["activation"] != "on_demand"
            || value["endpoint"] != endpoint
            || value["child_task_id"] != child_task
            || value["policy_sha256"] != policy_sha256
            || value["permissions_granted"] != false
            || value["provider_stop_proven"] != false
        {
            bail!("Lazy native lifecycle event differs from its original scope")
        }
        let bindings = value["model_bindings_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Lazy lifecycle event lacks model binding identity"))?;
        if !valid_digest(bindings) {
            bail!("Lazy native lifecycle event has an invalid binding digest")
        }
        let state = value["state"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Lazy lifecycle event has no state"))?;
        if state != "generation_operator_stopped"
            && (value.get("provider_stop_acknowledged").is_some()
                || value.get("original_outcome").is_some())
        {
            bail!("Unexpected native stop-resolution metadata")
        }
        match state {
            "execution_started" => {
                if (root_active && calls.values().any(|(_, _, _, completed)| !completed))
                    || !value["model_key"].is_null()
                    || !value["receipt_id"].is_null()
                    || !value["call_id"].is_null()
                {
                    bail!("Lazy lifecycle task has an ambiguous start event")
                }
                if value["model_bindings_sha256"]
                    != crate::residency::lifecycle::lazy::activation_bindings_sha256(
                        &active_receipts,
                        &active_origins,
                    )?
                {
                    bail!("Lazy lifecycle start has a mismatched activated-model set")
                }
                root_started = true;
                root_active = true;
            }
            "execution_finished" | "outcome_unknown" => {
                if !root_active
                    || !value["model_key"].is_null()
                    || !value["receipt_id"].is_null()
                    || !value["call_id"].is_null()
                {
                    bail!("Lazy lifecycle task has an ambiguous terminal event")
                }
                if value["model_bindings_sha256"]
                    != crate::residency::lifecycle::lazy::activation_bindings_sha256(
                        &active_receipts,
                        &active_origins,
                    )?
                {
                    bail!("Lazy lifecycle terminal has a mismatched activated-model set")
                }
                root_active = false;
                if state == "outcome_unknown" {
                    bail!("Lazy native lifecycle has an unresolved task outcome")
                }
            }
            "generation_started"
            | "generation_confirmed"
            | "generation_not_dispatched"
            | "generation_outcome_unknown"
            | "generation_operator_stopped" => {
                if !root_started || !root_active {
                    bail!("Lazy inference call is outside its original lifecycle")
                }
                let model = value["model_key"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Lazy inference event lacks a model"))?;
                let receipt_id = value["receipt_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Lazy inference event lacks a receipt"))?;
                if receipts
                    .get(model)
                    .is_none_or(|receipt| receipt.receipt_id != receipt_id)
                {
                    bail!("Lazy inference event refers to an unbound model receipt")
                }
                let call_id = value["call_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Lazy inference event lacks a call ID"))?;
                uuid::Uuid::parse_str(call_id)?;
                match state {
                    "generation_started" => {
                        if value["model_bindings_sha256"]
                            != crate::residency::lifecycle::lazy::activation_bindings_sha256(
                                &active_receipts,
                                &active_origins,
                            )?
                        {
                            bail!("Lazy inference start has a mismatched activated-model set")
                        }
                        if calls
                            .insert(
                                call_id.to_owned(),
                                (
                                    model.to_owned(),
                                    receipt_id.to_owned(),
                                    bindings.to_owned(),
                                    false,
                                ),
                            )
                            .is_some()
                        {
                            bail!("Lazy inference call ID is duplicated")
                        }
                    }
                    terminal => {
                        let prior = calls.get_mut(call_id).ok_or_else(|| {
                            anyhow::anyhow!("Lazy inference completion has no matching start")
                        })?;
                        if prior.0 != model
                            || prior.1 != receipt_id
                            || prior.2 != bindings
                            || prior.3
                        {
                            bail!("Lazy inference completion differs from its original call")
                        }
                        prior.3 = true;
                        if terminal == "generation_outcome_unknown" {
                            bail!("Lazy native inference outcome is unresolved")
                        }
                        if terminal == "generation_operator_stopped"
                            && (value["provider_stop_acknowledged"] != true
                                || value["original_outcome"] != "unknown")
                        {
                            bail!(
                                "Lazy native generation resolution lacks operator acknowledgement"
                            )
                        }
                        if terminal != "generation_operator_stopped"
                            && (value.get("provider_stop_acknowledged").is_some()
                                || value.get("original_outcome").is_some())
                        {
                            bail!("Unexpected generation resolution metadata")
                        }
                    }
                }
            }
            _ => bail!("Unsupported lazy native lifecycle state"),
        }
    }
    if !root_started
        || calls.values().any(|(_, _, _, completed)| !completed)
        || activation_index != activation_event_ids.len()
    {
        bail!("Lazy native lifecycle has an incomplete task or inference admission")
    }
    Ok(())
}

fn lazy_lifecycle_remaining(
    binding: &Value,
    clock: &crate::execution_clock::Snapshot,
    max_lifecycle_seconds: u64,
) -> Result<u64> {
    let boot = binding["lifecycle_boot_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Lazy native binding has no original boot identity"))?;
    if uuid::Uuid::parse_str(boot)?.to_string() != boot {
        bail!("Lazy native lifecycle boot identity is malformed")
    }
    let deadline = binding["lifecycle_deadline_ns"]
        .as_u64()
        .filter(|deadline| *deadline > 0)
        .ok_or_else(|| anyhow::anyhow!("Lazy native binding has no lifecycle cutoff"))?;
    let (current_boot, now) = crate::execution_clock::observation()
        .ok_or_else(|| anyhow::anyhow!("Original lazy native lifecycle clock is unavailable"))?;
    if current_boot != boot {
        bail!("Lazy native lifecycle cannot be restored after a reboot")
    }
    if now >= deadline {
        bail!("Original lazy native lifecycle cutoff has expired")
    }
    let task_remaining = remaining(clock)?;
    let cutoff_remaining = deadline - now;
    let policy_remaining = max_lifecycle_seconds
        .checked_mul(1_000_000_000)
        .ok_or_else(|| anyhow::anyhow!("Native lifecycle duration overflow"))?;
    Ok(task_remaining.min(cutoff_remaining).min(policy_remaining))
}

type LazyActivationReceipts = (
    BTreeMap<String, managed::Receipt>,
    BTreeMap<String, String>,
    BTreeMap<String, i64>,
);

fn read_lazy_activation_receipts(
    db: &rusqlite::Connection,
    lifecycle_task: &str,
    child_task: &str,
    policy: &NativeLifecyclePolicy,
    policy_hash: &str,
    cfg: &Config,
) -> Result<LazyActivationReceipts> {
    let mut statement = db.prepare("SELECT id,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND kind='native_task_model_binding' AND source='security-kernel' ORDER BY id LIMIT 9")?;
    let rows = statement
        .query_map([lifecycle_task], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 8 {
        bail!("Lazy native activated-model bindings exceed policy bounds")
    }
    let mut activated = BTreeMap::new();
    let mut origins = BTreeMap::new();
    let mut event_ids = BTreeMap::new();
    for (event_id, raw) in rows {
        let value = crate::strict_json::value(raw.as_bytes(), 16384)?;
        let allowed = [
            "format_version",
            "policy_sha256",
            "child_task_id",
            "activation_binding_sha256",
            "model_key",
            "receipt",
            "receipt_sha256",
            "inventory_sha256",
            "origin",
            "role",
            "prompt_sha256",
            "backend_kind",
            "permissions_granted",
            "weights_attested",
            "response_instance_check_required",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        if value
            .as_object()
            .is_none_or(|object| object.keys().any(|key| !allowed.contains(key.as_str())))
        {
            bail!("Lazy native model binding contains unsupported fields")
        }
        let model_key = value["model_key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Lazy native binding has no model key"))?;
        let approved = policy
            .models
            .iter()
            .find(|model| model.model == model_key)
            .ok_or_else(|| anyhow::anyhow!("Lazy native binding names an unapproved model"))?;
        let role = value["role"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Lazy native binding has no original model role"))?;
        let backend_kind = value["backend_kind"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Lazy native binding has no backend identity"))?;
        let (configured_model, _) = cfg.model_settings(role)?;
        cfg.require_role_model(role, model_key)?;
        if !crate::fabric::protocol::ROLES.contains(&role)
            || !matches!(backend_kind, "configured_role" | "expert_pinned")
            || (backend_kind == "configured_role" && configured_model != model_key)
            || value["format_version"] != 1
            || value["policy_sha256"] != policy_hash
            || value["child_task_id"] != child_task
            || value["permissions_granted"] != false
            || value["weights_attested"] != false
            || value["response_instance_check_required"] != true
            || !matches!(value["origin"].as_str(), Some("newly_loaded" | "reused"))
        {
            bail!("Lazy native model binding differs from original operator scope")
        }
        let activation_digest = value["activation_binding_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Lazy model binding lacks its origin-bound digest"))?;
        if !valid_digest(activation_digest) {
            bail!("Lazy model binding origin digest is malformed")
        }
        let mut unsigned = value.clone();
        unsigned
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Lazy model binding is not an object"))?
            .remove("activation_binding_sha256");
        if activation_digest != crate::store::sha256(&serde_json::to_vec(&unsigned)?) {
            bail!("Lazy model binding origin digest does not match its receipt")
        }
        for digest_key in ["receipt_sha256", "inventory_sha256", "prompt_sha256"] {
            let digest = value[digest_key]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Lazy native binding is missing {digest_key}"))?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                bail!("Lazy native binding has an invalid {digest_key}")
            }
        }
        let receipt: managed::Receipt = serde_json::from_value(value["receipt"].clone())?;
        if receipt.model_key != model_key
            || receipt.context_length != approved.context_length
            || receipt.offload_kv_cache_to_gpu != approved.offload_kv_cache_to_gpu
            || receipt.endpoint != endpoint(&policy.api_base)?.as_str()
            || value["receipt_sha256"] != crate::store::sha256(&serde_json::to_vec(&receipt)?)
        {
            bail!("Lazy native activated-model receipt differs from approved policy")
        }
        if activated.insert(model_key.to_owned(), receipt).is_some() {
            bail!("Lazy native model has ambiguous activation receipts")
        }
        origins.insert(
            model_key.to_owned(),
            value["origin"].as_str().unwrap().to_owned(),
        );
        event_ids.insert(model_key.to_owned(), event_id);
    }
    Ok((activated, origins, event_ids))
}

fn activation_receipts_digest(db: &rusqlite::Connection, lifecycle_task: &str) -> Result<String> {
    let mut statement = db.prepare("SELECT id,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND kind='native_task_model_binding' AND source='security-kernel' ORDER BY id LIMIT 9")?;
    let rows = statement
        .query_map([lifecycle_task], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 8 {
        bail!("Lazy native activated-model bindings exceed policy bounds")
    }
    Ok(crate::store::sha256(&serde_json::to_vec(&rows)?))
}

/// Inspect original native receipt identity under exclusive local ownership.
/// This GET-only preflight does not authorize continuation, restore callbacks,
/// acknowledge stopped providers, reset budgets, or load/release models.
pub(crate) async fn prepare(
    store: Arc<Store>,
    task: &str,
    policy: &NativeLifecyclePolicy,
    build_backend: bool,
) -> Result<NativeRuntime> {
    security::authorize("human-cli", "task.resume")?;
    uuid::Uuid::parse_str(task)?;
    policy.validate()?;
    let lazy = policy.format_version == 3;
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
    let local = managed::list(&store, &policy.api_base)?;
    if local["managed_lifecycle_blocked"] != false {
        bail!("Native restoration requires operator reconciliation of prior lifecycle activity")
    }
    let allowance = Duration::from_nanos(original.lifecycle_remaining_ns)
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
        || rechecked.activation_bindings_sha256 != original.activation_bindings_sha256
        || serde_json::to_value(&rechecked.clock)? != serde_json::to_value(&original.clock)?
        || serde_json::to_value(&rechecked.cfg)? != serde_json::to_value(&original.cfg)?
    {
        bail!("Original native restoration state changed during inventory inspection")
    }
    require_no_unknown_generation(&store, &native_endpoint)?;
    let remaining_ns = original
        .lifecycle_remaining_ns
        .min(remaining(&original.clock)?);
    let report = json!({"format_version":1,"task_id":task,"lifecycle_task_id":original.parent,
        "binding_event_id":original.binding_event,"binding_event_sha256":original.binding_sha256,
        "inventory_sha256":planning::fingerprint(&inventory)?,"receipt_count":original.receipts.len(),
        "activation_receipt_origins":&original.activated_origins,
        "receipt_identity_matches":true,"original_remaining_ns":remaining_ns,
        "resume_authorized":false,"checkpoint_validated":false,"permissions_granted":false,
        "model_mutation_performed":false,"provider_stop_proven":false,"weights_attested":false,
        "automatic_replay":false});
    let mut restored_coordinator = None;
    let mut cancel_keepalive = None;
    let backend: Option<Arc<dyn crate::backend::InferenceBackend>> = if build_backend {
        let mut cfg = original.cfg.clone();
        cfg.api_base = policy.api_base.clone();
        if lazy {
            let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
            let cutoff = crate::residency::lifecycle::lazy::LifecycleCutoff {
                boot_id: original.binding["lifecycle_boot_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing original lifecycle boot ID"))?
                    .to_owned(),
                deadline_ns: original.binding["lifecycle_deadline_ns"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing original lifecycle cutoff"))?,
            };
            let initial = original
                .receipts
                .iter()
                .map(|(model, receipt)| {
                    let origin = match original.activated_origins.get(model).map(String::as_str) {
                        Some("newly_loaded") => "newly_loaded",
                        Some("reused") => "reused",
                        _ => bail!("Restored lazy receipt has no original ownership source"),
                    };
                    Ok((
                        model.clone(),
                        crate::residency::lifecycle::lazy::BoundReceipt {
                            receipt: receipt.clone(),
                            origin,
                        },
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            if initial.len() != original.activated_origins.len() {
                bail!("Restored lazy receipt ownership map is inconsistent")
            }
            let coordinator = crate::residency::lifecycle::lazy::Coordinator::new(
                store.clone(),
                Arc::new(policy.clone()),
                ownership.clone(),
                original.parent.clone(),
                task,
                policy.authenticate,
                cutoff,
                crate::residency::lifecycle::lazy::LifecycleCutoff {
                    boot_id: original.binding["lifecycle_boot_id"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing original lifecycle boot ID"))?
                        .to_owned(),
                    deadline_ns: original.binding["lifecycle_deadline_ns"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("Missing original lifecycle cutoff"))?,
                }
                .instant()?,
                cancel_rx,
                initial,
            )?;
            let lazy_backend = crate::residency::lifecycle::lazy::LazyBackend::new(
                cfg,
                coordinator.clone(),
                None,
            )?;
            restored_coordinator = Some(coordinator);
            cancel_keepalive = Some(cancel_tx);
            Some(Arc::new(lazy_backend) as Arc<dyn crate::backend::InferenceBackend>)
        } else {
            let mut targets = BTreeMap::new();
            let mut receipts = BTreeMap::new();
            for role in crate::fabric::protocol::ROLES {
                let (model, _) = cfg.model_settings(role)?;
                let receipt = original
                    .receipts
                    .get(model)
                    .ok_or_else(|| anyhow::anyhow!("Missing original role receipt"))?;
                targets.insert(
                    role.to_owned(),
                    crate::backend::NativeModelTarget {
                        api_base: policy.api_base.clone(),
                        model: model.to_owned(),
                        model_instance_id: receipt.instance_id.clone(),
                    }
                    .canonical()?,
                );
                receipts.insert(role.to_owned(), receipt.clone());
            }
            let inner = Arc::new(crate::backend::native::NativeBackend::with_role_targets(
                cfg,
                targets,
                policy.authenticate,
            )?);
            let bound = BoundBackend {
                inner,
                ownership: ownership.clone(),
                base: policy.api_base.clone(),
                authenticate: policy.authenticate,
                child_task: task.to_owned(),
                receipts,
            };
            Some(Arc::new(ResumedBackend {
                bound,
                binding: original.binding.clone(),
                endpoint: native_endpoint,
            }) as Arc<dyn crate::backend::InferenceBackend>)
        }
    } else {
        None
    };
    Ok(NativeRuntime {
        ownership,
        task: task.to_owned(),
        policy: policy.clone(),
        original,
        backend,
        coordinator: restored_coordinator,
        _cancel_keepalive: cancel_keepalive,
        report,
    })
}

/// Inspect native receipts without admitting continuation or dispatching inference.
pub async fn inspect(
    store: Arc<Store>,
    task: &str,
    policy: &NativeLifecyclePolicy,
) -> Result<Value> {
    Ok(prepare(store, task, policy, false).await?.report)
}

pub(crate) struct NativeRuntime {
    pub(crate) ownership: Arc<Ownership>,
    task: String,
    policy: NativeLifecyclePolicy,
    original: Original,
    backend: Option<Arc<dyn crate::backend::InferenceBackend>>,
    coordinator: Option<Arc<crate::residency::lifecycle::lazy::Coordinator>>,
    _cancel_keepalive: Option<tokio::sync::watch::Sender<bool>>,
    report: Value,
}
impl NativeRuntime {
    pub(crate) fn validate(&self, db: &rusqlite::Connection, task: &str) -> Result<()> {
        if self.task != task {
            bail!("Native restoration belongs to another task")
        }
        let current = read_original(db, task, &self.policy)?;
        if current.binding_event != self.original.binding_event
            || current.binding_sha256 != self.original.binding_sha256
            || current.activation_bindings_sha256 != self.original.activation_bindings_sha256
            || serde_json::to_value(&current.cfg)? != serde_json::to_value(&self.original.cfg)?
            || serde_json::to_value(&current.clock)? != serde_json::to_value(&self.original.clock)?
        {
            bail!("Native restoration changed before continuation admission")
        }
        remaining(&self.original.clock)?;
        Ok(())
    }
    pub(crate) fn backend(&self) -> Result<Arc<dyn crate::backend::InferenceBackend>> {
        self.backend
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Native inspection has no inference backend"))
    }
    pub(crate) fn model_backend_factory(&self) -> Option<crate::engine::ModelBackendFactory> {
        let coordinator = self.coordinator.clone()?;
        Some(Arc::new(move |config| {
            crate::residency::lifecycle::lazy::backend_for_expert(
                config.clone(),
                coordinator.clone(),
                config.model.clone(),
            )
            .map(|backend| backend as Arc<dyn crate::backend::InferenceBackend>)
        }))
    }
    pub(crate) fn begin_continuation(&self) -> Result<()> {
        if let Some(coordinator) = &self.coordinator {
            coordinator.lifecycle_event(
                &self.original.parent,
                "execution_started",
                None,
                None,
                None,
            )?;
        }
        Ok(())
    }
    pub(crate) async fn finish_continuation(&self, status: &str) -> Result<Value> {
        let Some(coordinator) = &self.coordinator else {
            return Ok(Value::Null);
        };
        let uncertain = coordinator.has_uncertain_activation()
            || self
                .backend
                .as_ref()
                .is_none_or(|backend| backend.has_uncertain_dispatch());
        let cleanup_safe =
            !uncertain && !matches!(status, "cancelled" | "exhausted" | "failed" | "interrupted");
        let mut released = Vec::new();
        if self.policy.release_after && cleanup_safe {
            for receipt_id in coordinator.newly_loaded_ids()? {
                let deadline = coordinator.cutoff().instant()?;
                tokio::time::timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    managed::release_owned(
                        &self.ownership.store,
                        &self.policy.api_base,
                        self.policy.authenticate,
                        &receipt_id,
                        (true, Some(&self.ownership)),
                    ),
                )
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "Original native lifecycle deadline exhausted during receipt cleanup"
                    )
                })??;
                released.push(receipt_id);
            }
        }
        coordinator.lifecycle_event(
            &self.original.parent,
            if uncertain {
                "outcome_unknown"
            } else {
                "execution_finished"
            },
            None,
            None,
            None,
        )?;
        let owned_count = coordinator.newly_loaded_ids()?.len();
        Ok(json!({
            "activation":"on_demand",
            "released_receipt_ids":released,
            "release_deferred":self.policy.release_after && (!cleanup_safe || released.len()!=owned_count),
            "provider_stop_proven":false,
            "weights_attested":false,
            "permissions_granted":false,
        }))
    }
    pub(crate) fn evidence(&self) -> Value {
        self.report.clone()
    }
}

#[derive(Clone)]
struct ResumedBackend {
    bound: BoundBackend,
    binding: Value,
    endpoint: String,
}
struct DispatchGuard {
    store: Arc<Store>,
    parent: String,
    armed: bool,
}
impl Drop for DispatchGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.store.finish_task(
                &self.parent,
                "interrupted",
                "Native continuation generation outcome unknown; do not replay",
            );
        }
    }
}
impl crate::backend::InferenceBackend for ResumedBackend {
    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        crate::backend::InferenceBackend::input_token_admission(&self.bound)
    }
    fn inference_target(&self, role: &str) -> Result<Option<crate::backend::InferenceTarget>> {
        self.bound.inference_target(role)
    }
    fn has_uncertain_dispatch(&self) -> bool {
        self.bound.has_uncertain_dispatch()
    }
    fn pin(&self) -> Result<Option<Arc<dyn crate::backend::InferenceBackend>>> {
        Ok(Some(Arc::new(self.clone())))
    }
    fn generate<'a>(
        &'a self,
        _role: &'a str,
        _prompt: &'a str,
        _max_tokens: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::backend::Generation>> + Send + 'a>,
    > {
        Box::pin(async { bail!("Native continuation requires original task identity") })
    }
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        max_tokens: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::backend::Generation>> + Send + 'a>,
    > {
        Box::pin(async move {
            if task != self.bound.child_task {
                bail!("Native continuation belongs to another task")
            }
            let store = self.bound.ownership.store.clone();
            let parent = store.create_task("Native continuation generation", "fast")?;
            let mut guard = DispatchGuard {
                store: store.clone(),
                parent: parent.clone(),
                armed: true,
            };
            let mut binding = self.binding.clone();
            binding["continuation_dispatch"] = json!({"role":role,
                "prompt_sha256":crate::store::sha256(prompt.as_bytes()),
                "max_output_tokens":max_tokens,"permissions_granted":false});
            store.event(
                &parent,
                "native_continuation_bindings",
                "human-cli",
                binding,
            )?;
            let digest = self.binding["bindings_sha256"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing original binding digest"))?;
            event(
                &store,
                &parent,
                "generation_started",
                &self.endpoint,
                task,
                digest,
            )?;
            let result = self
                .bound
                .generate_for(task, role, prompt, max_tokens)
                .await;
            if result.is_ok() && !self.bound.has_uncertain_dispatch() {
                event(
                    &store,
                    &parent,
                    "generation_confirmed",
                    &self.endpoint,
                    task,
                    digest,
                )?;
                store.finish_task(
                    &parent,
                    "completed",
                    "Native continuation generation confirmed",
                )?;
                guard.armed = false;
            }
            result
        })
    }
}
