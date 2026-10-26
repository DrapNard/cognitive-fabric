//! Durable, operator-pinned native lifecycle scope for a client session.
//!
//! This module admits only already-managed receipts that remain present in a
//! fresh native inventory. It never loads a model or dispatches inference.
use crate::{
    config::Config,
    execution_clock::Snapshot,
    residency::{
        self,
        lifecycle::{NativeLifecyclePolicy, Ownership},
        managed,
    },
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

const SCOPE_KIND: &str = "agent_native_scope";
const CHECKPOINT_KIND: &str = "agent_native_scope_checkpoint";
const SOURCE: &str = "security-kernel";
const MAX_SCOPE_BYTES: usize = 131_072;
const MAX_CHECKPOINTS: usize = 8;

struct SqlProgressReset<'a>(&'a rusqlite::Connection);

impl Drop for SqlProgressReset<'_> {
    fn drop(&mut self) {
        self.0.progress_handler(0, None::<fn() -> bool>);
    }
}

fn bounded_sql_scan(db: &rusqlite::Connection) -> SqlProgressReset<'_> {
    let started = Instant::now();
    db.progress_handler(
        1000,
        Some(move || started.elapsed() > Duration::from_secs(1)),
    );
    SqlProgressReset(db)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    receipt: managed::Receipt,
    /// This module currently admits existing managed instances only.
    origin: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ScopeRecord {
    format_version: u32,
    root_task_id: String,
    policy: NativeLifecyclePolicy,
    policy_sha256: String,
    configuration_sha256: String,
    boot_id: String,
    session_started_ns: u64,
    session_deadline_ns: u64,
    lifecycle_deadline_ns: u64,
    bindings: BTreeMap<String, Binding>,
    bindings_sha256: String,
    checkpoint_sequence: u32,
    checkpoint_sha256: String,
    permissions_granted: bool,
    weights_attested: bool,
    response_instance_check_required: bool,
    response_instance_attested: bool,
    inference_dispatched: bool,
    scope_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    format_version: u32,
    root_task_id: String,
    sequence: u32,
    previous_sha256: String,
    policy_sha256: String,
    configuration_sha256: String,
    bindings: BTreeMap<String, Binding>,
    bindings_sha256: String,
    checkpoint_sha256: String,
    permissions_granted: bool,
    weights_attested: bool,
    response_instance_check_required: bool,
    response_instance_attested: bool,
    inference_dispatched: bool,
}

/// A hash-bound session pin. It does not enable model activation or inference.
pub struct NativeClientScope {
    store: Arc<Store>,
    owner: Arc<Ownership>,
    record: ScopeRecord,
    bindings: BTreeMap<String, Binding>,
    checkpoint_sequence: u32,
    checkpoint_sha256: String,
}

fn digest<T: Serialize>(value: &T) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(value)?))
}

fn scope_digest(record: &ScopeRecord) -> Result<String> {
    let mut value = serde_json::to_value(record)?;
    value
        .as_object_mut()
        .expect("scope serializes as object")
        .remove("scope_sha256");
    Ok(sha256(&serde_json::to_vec(&value)?))
}

fn checkpoint_digest(checkpoint: &Checkpoint) -> Result<String> {
    let mut value = serde_json::to_value(checkpoint)?;
    value
        .as_object_mut()
        .expect("checkpoint serializes as object")
        .remove("checkpoint_sha256");
    Ok(sha256(&serde_json::to_vec(&value)?))
}

fn snapshot_fields(clock: &Snapshot, cfg: &Config) -> Result<(String, u64, u64)> {
    clock.validate(cfg.max_wall_seconds)?;
    let remaining = clock.remaining()?;
    if remaining["status"] != "same_boot" || remaining["expired"] != false {
        bail!("Client native scope requires a live same-boot session clock")
    }
    let value = serde_json::to_value(clock)?;
    let boot = value["boot_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Session clock has no boot identity"))?;
    let deadline = value["deadline_ns"]
        .as_u64()
        .filter(|deadline| *deadline > 0)
        .ok_or_else(|| anyhow::anyhow!("Session clock has no deadline"))?;
    let started = value["started_ns"]
        .as_u64()
        .filter(|started| *started > 0 && *started < deadline)
        .ok_or_else(|| anyhow::anyhow!("Session clock has no valid start"))?;
    Ok((boot.to_owned(), started, deadline))
}

fn inventory_endpoint(policy: &NativeLifecyclePolicy) -> Result<String> {
    let mut endpoint = url::Url::parse(&policy.api_base)?;
    endpoint.set_path("/api/v1/models");
    Ok(endpoint.to_string())
}

fn validate_identity(root_task: &str, cfg: &Config, policy: &NativeLifecyclePolicy) -> Result<()> {
    uuid::Uuid::parse_str(root_task)?;
    cfg.validate()?;
    policy.validate()?;
    if serde_json::to_vec(policy)?.len() > 65_536 || serde_json::to_vec(cfg)?.len() > 65_536 {
        bail!("Native client scope policy or configuration exceeds metadata bounds")
    }
    if policy.format_version != 3
        || cfg.backend != "openai-local"
        || cfg.inference_process
        || cfg.input_token_admission.is_some()
        || cfg.model_registration.is_some()
    {
        bail!("Client native scope requires direct openai-local inference without token admission or registered-model routing")
    }
    let endpoint = url::Url::parse(&policy.api_base)?;
    let client_endpoint = url::Url::parse(&cfg.api_base)?;
    if endpoint.origin() != client_endpoint.origin() {
        bail!("Client and native lifecycle endpoints must share the operator-approved origin")
    }
    if cfg.inference_auth_scope.as_ref().is_some_and(|scope| {
        url::Url::parse(scope)
            .ok()
            .is_none_or(|scope| scope.origin() != endpoint.origin())
    }) {
        bail!("Client inference authentication scope differs from the approved native origin")
    }
    let allowed = policy
        .models
        .iter()
        .map(|model| model.model.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if !allowed.contains(cfg.model.as_str())
        || cfg
            .role_profiles
            .values()
            .any(|profile| !allowed.contains(profile.model.as_str()))
    {
        bail!("Native client policy does not cover every configured model route")
    }
    Ok(())
}

fn lifecycle_deadline(
    session_started_ns: u64,
    session_deadline_ns: u64,
    policy: &NativeLifecyclePolicy,
) -> Result<u64> {
    let policy_ns = policy
        .max_lifecycle_seconds
        .checked_mul(1_000_000_000)
        .ok_or_else(|| anyhow::anyhow!("Native lifecycle duration overflows"))?;
    Ok(session_deadline_ns.min(
        session_started_ns
            .checked_add(policy_ns)
            .ok_or_else(|| anyhow::anyhow!("Native lifecycle deadline overflows"))?,
    ))
}

fn ensure_owner(owner: &Ownership, store: &Store) -> Result<()> {
    owner.validate(store)
}

fn insert_event_tx(
    tx: &rusqlite::Transaction<'_>,
    root_task: &str,
    kind: &str,
    payload: &impl Serialize,
) -> Result<()> {
    let value = serde_json::to_value(payload)?;
    let raw = serde_json::to_vec(&value)?;
    if raw.len() > MAX_SCOPE_BYTES {
        bail!("Native client scope event exceeds metadata bounds")
    }
    tx.execute(
        "INSERT INTO events(task_id,kind,source,payload,created) SELECT id,?2,?3,?4,unixepoch() FROM tasks WHERE id=?1 AND status IN ('running','waiting_client')",
        params![root_task, kind, SOURCE, String::from_utf8(raw)?],
    )?;
    if tx.changes() != 1 {
        bail!("Native client scope root task is missing or not resumable")
    }
    Ok(())
}

fn active_task_state(db: &rusqlite::Connection, root_task: &str) -> Result<String> {
    let state: String = db.query_row(
        "SELECT CASE WHEN length(CAST(status AS BLOB))<=64 THEN status END FROM tasks WHERE id=?1",
        [root_task],
        |row| row.get(0),
    )?;
    if !matches!(state.as_str(), "running" | "waiting_client") {
        bail!("Native client scope root task is not active")
    }
    Ok(state)
}

impl NativeClientScope {
    /// Prepare the native scope for one existing durable Agent client session.
    /// Approval is explicit; the scope can inspect/admit existing receipts but
    /// does not activate models or issue inference.
    pub async fn prepare_session(
        store: Arc<Store>,
        root_task: &str,
        cfg: &Config,
        policy: NativeLifecyclePolicy,
        human_approve: bool,
    ) -> Result<Self> {
        if !human_approve {
            bail!("Native client lifecycle requires explicit operator approval")
        }
        crate::security::authorize("human-cli", "model.residency.inspect")?;
        let mut expected_runtime_lock = cfg.db().into_os_string();
        expected_runtime_lock.push(".runtime.lock");
        if store
            .runtime_lock
            .as_deref()
            .map(std::path::Path::as_os_str)
            != Some(expected_runtime_lock.as_os_str())
        {
            bail!("Native client scope requires the configured persistent workspace database")
        }
        let owner = Ownership::acquire(store.clone())?;
        let session = super::sessions::native_scope_session_snapshot(&store, root_task, cfg)?;
        crate::residency::lifecycle::require_no_unknown_generation(
            &store,
            &inventory_endpoint(&policy)?,
        )?;
        let exists = {
            let db = store.lock()?;
            let _scan_deadline = bounded_sql_scan(&db);
            let found: Option<i64> = db
                .query_row(
                    "SELECT 1 FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 LIMIT 1",
                    params![root_task, SCOPE_KIND, SOURCE],
                    |row| row.get(0),
                )
                .optional()?;
            drop(_scan_deadline);
            found.is_some()
        };
        let mut scope = if !exists {
            if session.generation != 0 || session.calls != 0 || session.state != "active" {
                bail!("A native scope cannot be attached after the client session has started")
            }
            Self::create(store, owner, root_task, cfg, &session.clock, policy)?
        } else {
            Self::restore(store, owner, root_task, cfg, &session.clock, &policy)?
        };
        scope.admit_existing_receipts().await?;
        Ok(scope)
    }

    fn create(
        store: Arc<Store>,
        owner: Arc<Ownership>,
        root_task: &str,
        cfg: &Config,
        session_clock: &Snapshot,
        policy: NativeLifecyclePolicy,
    ) -> Result<Self> {
        ensure_owner(&owner, &store)?;
        validate_identity(root_task, cfg, &policy)?;
        let (boot_id, session_started_ns, session_deadline_ns) =
            snapshot_fields(session_clock, cfg)?;
        let (observed_boot, now_ns) = crate::execution_clock::observation()
            .ok_or_else(|| anyhow::anyhow!("Native client scope requires a same-boot clock"))?;
        if observed_boot != boot_id || now_ns < session_started_ns || now_ns >= session_deadline_ns
        {
            bail!("Native client scope clock differs from the original session")
        }
        let bindings = BTreeMap::new();
        let mut record = ScopeRecord {
            format_version: 1,
            root_task_id: root_task.to_owned(),
            policy_sha256: digest(&policy)?,
            configuration_sha256: digest(cfg)?,
            lifecycle_deadline_ns: lifecycle_deadline(
                session_started_ns,
                session_deadline_ns,
                &policy,
            )?,
            policy,
            boot_id,
            session_started_ns,
            session_deadline_ns,
            bindings_sha256: digest(&bindings)?,
            bindings,
            checkpoint_sequence: 0,
            checkpoint_sha256: sha256(b"agent-native-scope-genesis-v1"),
            permissions_granted: false,
            weights_attested: false,
            response_instance_check_required: true,
            response_instance_attested: false,
            inference_dispatched: false,
            scope_sha256: String::new(),
        };
        record.scope_sha256 = scope_digest(&record)?;
        let mut db = store.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        active_task_state(&tx, root_task)?;
        let _scan_deadline = bounded_sql_scan(&tx);
        let existing: i64 = tx.query_row(
            "SELECT count(*) FROM events WHERE task_id=?1 AND kind=?2",
            params![root_task, SCOPE_KIND],
            |row| row.get(0),
        )?;
        if existing != 0 {
            bail!("Native client scope already exists for this session root")
        }
        insert_event_tx(&tx, root_task, SCOPE_KIND, &record)?;
        drop(_scan_deadline);
        tx.commit()?;
        drop(db);
        Ok(Self {
            store,
            owner,
            bindings: BTreeMap::new(),
            checkpoint_sequence: 0,
            checkpoint_sha256: record.checkpoint_sha256.clone(),
            record,
        })
    }

    /// Restore the single original scope and its full bounded checkpoint chain.
    fn restore(
        store: Arc<Store>,
        owner: Arc<Ownership>,
        root_task: &str,
        cfg: &Config,
        session_clock: &Snapshot,
        expected_policy: &NativeLifecyclePolicy,
    ) -> Result<Self> {
        ensure_owner(&owner, &store)?;
        validate_identity(root_task, cfg, expected_policy)?;
        let (boot_id, session_started_ns, session_deadline_ns) =
            snapshot_fields(session_clock, cfg)?;
        let mut db = store.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let _scan_deadline = bounded_sql_scan(&tx);
        active_task_state(&tx, root_task)?;
        let mut statement = tx.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT 2",
        )?;
        let rows = statement
            .query_map(params![root_task, SCOPE_KIND, SOURCE], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() != 1 {
            bail!("Native client scope is missing or ambiguous")
        }
        let scope_value = crate::strict_json::value(rows[0].as_bytes(), MAX_SCOPE_BYTES)?;
        let mut record: ScopeRecord = serde_json::from_value(scope_value)?;
        if record.format_version != 1
            || record.root_task_id != root_task
            || record.policy_sha256 != digest(expected_policy)?
            || serde_json::to_value(&record.policy)? != serde_json::to_value(expected_policy)?
            || record.configuration_sha256 != digest(cfg)?
            || record.boot_id != boot_id
            || record.session_deadline_ns != session_deadline_ns
            || record.session_started_ns != session_started_ns
            || record.lifecycle_deadline_ns
                != lifecycle_deadline(session_started_ns, session_deadline_ns, expected_policy)?
            || record.bindings_sha256 != digest(&record.bindings)?
            || record.scope_sha256 != scope_digest(&record)?
            || record.checkpoint_sequence != 0
            || record.checkpoint_sha256 != sha256(b"agent-native-scope-genesis-v1")
            || record.permissions_granted
            || record.weights_attested
            || !record.response_instance_check_required
            || record.response_instance_attested
            || record.inference_dispatched
            || !record.bindings.is_empty()
        {
            bail!("Native client scope differs from its pinned session, policy, or config")
        }
        let (current_boot, now_ns) = crate::execution_clock::observation()
            .ok_or_else(|| anyhow::anyhow!("Native client scope clock is unavailable"))?;
        if current_boot != record.boot_id || now_ns >= record.lifecycle_deadline_ns {
            bail!("Original native client lifecycle deadline expired or belongs to another boot")
        }
        let mut checkpoints = tx.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT 9",
        )?;
        let rows = checkpoints
            .query_map(params![root_task, CHECKPOINT_KIND, SOURCE], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() > MAX_CHECKPOINTS {
            bail!("Native client scope checkpoint history is incomplete or exceeds bounds")
        }
        let mut previous = sha256(b"agent-native-scope-genesis-v1");
        let mut bindings = BTreeMap::new();
        for (index, raw) in rows.iter().enumerate() {
            let value = crate::strict_json::value(raw.as_bytes(), MAX_SCOPE_BYTES)?;
            let checkpoint: Checkpoint = serde_json::from_value(value)?;
            if checkpoint.format_version != 1
                || checkpoint.root_task_id != root_task
                || checkpoint.sequence != index as u32 + 1
                || checkpoint.previous_sha256 != previous
                || checkpoint.policy_sha256 != record.policy_sha256
                || checkpoint.configuration_sha256 != record.configuration_sha256
                || checkpoint.bindings_sha256 != digest(&checkpoint.bindings)?
                || checkpoint.checkpoint_sha256 != checkpoint_digest(&checkpoint)?
                || checkpoint.permissions_granted
                || checkpoint.weights_attested
                || !checkpoint.response_instance_check_required
                || checkpoint.response_instance_attested
                || checkpoint.inference_dispatched
            {
                bail!("Native client scope checkpoint chain is invalid")
            }
            for (model, binding) in &checkpoint.bindings {
                validate_binding(&record.policy, model, binding)?;
                if let Some(previous) = bindings.get(model) {
                    if serde_json::to_value(previous)? != serde_json::to_value(binding)? {
                        bail!("Native client checkpoint replaced an original receipt pin")
                    }
                }
                bindings
                    .entry(model.clone())
                    .or_insert_with(|| binding.clone());
            }
            if digest(&bindings)? != checkpoint.bindings_sha256 {
                bail!("Native client scope checkpoint does not represent its cumulative bindings")
            }
            previous = checkpoint.checkpoint_sha256;
        }
        drop(checkpoints);
        drop(statement);
        drop(_scan_deadline);
        tx.commit()?;
        drop(db);
        // Restored bindings must still exist in the managed ledger. This is a
        // local bounded inspection; fresh provider inventory is checked before
        // every later admission, not inferred from this historical snapshot.
        for (model, binding) in &bindings {
            validate_binding(&record.policy, model, binding)?;
            let saved = managed::inspect(&store, &binding.receipt.receipt_id)?;
            if saved["state"] != "active" || saved["receipt"] != json!(binding.receipt) {
                bail!("Native client scope references a missing, changed, or inactive receipt")
            }
        }
        record.scope_sha256 = scope_digest(&record)?;
        Ok(Self {
            store,
            owner,
            record,
            bindings,
            checkpoint_sequence: rows.len() as u32,
            checkpoint_sha256: previous,
        })
    }

    /// Admit already-loaded managed receipts after a fresh, bounded inventory read.
    /// No model loading, generation, tool execution, or response attestation occurs.
    pub async fn admit_existing_receipts(&mut self) -> Result<usize> {
        ensure_owner(&self.owner, &self.store)?;
        self.check_deadline()?;
        let status = tokio::time::timeout(
            self.remaining()?,
            residency::status(
                &self.record.policy.api_base,
                self.record.policy.authenticate,
            ),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Native inventory admission deadline exhausted"))??;
        let models: Vec<residency::Model> = serde_json::from_value(status["models"].clone())?;
        if models.len() > 256 {
            bail!("Native model inventory exceeds scope admission bounds")
        }
        let local = managed::list(&self.store, &self.record.policy.api_base)?;
        if local["managed_lifecycle_blocked"] != false {
            bail!("Managed native endpoint requires operator reconciliation")
        }
        let rows = local["receipts"]
            .as_array()
            .filter(|rows| rows.len() <= 1024)
            .ok_or_else(|| {
                anyhow::anyhow!("Managed receipt inventory is malformed or over limit")
            })?;
        let mut next = self.bindings.clone();
        let mut added = 0usize;
        for ensure in &self.record.policy.models {
            self.check_deadline()?;
            let candidates = rows
                .iter()
                .filter(|row| {
                    row["state"] == "active"
                        && row["receipt"]["model_key"] == ensure.model
                        && row["receipt"]["context_length"] == ensure.context_length
                        && row["receipt"]["offload_kv_cache_to_gpu"]
                            == json!(ensure.offload_kv_cache_to_gpu)
                })
                .collect::<Vec<_>>();
            if candidates.len() > 1 {
                bail!("Ambiguous active managed receipts for an approved client model")
            }
            let Some(row) = candidates.first() else {
                if next.contains_key(&ensure.model) {
                    bail!("Previously admitted client receipt is missing from managed state")
                }
                continue;
            };
            let receipt: managed::Receipt = serde_json::from_value(row["receipt"].clone())?;
            let observed = managed::validate_task_binding(
                &self.store,
                &receipt.receipt_id,
                &self.record.policy.api_base,
                &models,
            )?;
            if observed.model_key != ensure.model
                || observed.context_length != ensure.context_length
                || observed.offload_kv_cache_to_gpu != ensure.offload_kv_cache_to_gpu
                || serde_json::to_value(&observed)? != serde_json::to_value(&receipt)?
            {
                bail!("Managed receipt differs from the exact approved client model scope")
            }
            if let Some(previous) = next.get(&ensure.model) {
                if serde_json::to_value(&previous.receipt)? != serde_json::to_value(&observed)? {
                    bail!("An existing client receipt binding changed and cannot be replaced")
                }
            } else {
                next.insert(
                    ensure.model.clone(),
                    Binding {
                        receipt: observed,
                        origin: "reused".into(),
                    },
                );
                added += 1;
            }
        }
        if added > 0 {
            self.publish_checkpoint(next)?;
        }
        self.check_deadline()?;
        Ok(added)
    }

    pub fn root_task_id(&self) -> &str {
        &self.record.root_task_id
    }
    pub fn policy_sha256(&self) -> &str {
        &self.record.policy_sha256
    }
    pub fn configuration_sha256(&self) -> &str {
        &self.record.configuration_sha256
    }
    /// Auditable state for the next runtime bridge; excludes prompts and
    /// credentials and makes the weaker OpenAI response evidence explicit.
    pub fn evidence(&self) -> Result<Value> {
        let receipt_bindings = self
            .bindings
            .iter()
            .map(|(model, binding)| -> Result<Value> {
                Ok(json!({
                    "model_key": model,
                    "receipt_id": binding.receipt.receipt_id,
                    "instance_id": binding.receipt.instance_id,
                    "origin": binding.origin,
                    "receipt_sha256": sha256(&serde_json::to_vec(&binding.receipt)?),
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({
            "format_version": 1,
            "root_task_id": self.record.root_task_id,
            "policy_sha256": self.record.policy_sha256,
            "configuration_sha256": self.record.configuration_sha256,
            "scope_sha256": self.record.scope_sha256,
            "checkpoint_sequence": self.checkpoint_sequence,
            "checkpoint_sha256": self.checkpoint_sha256,
            "lifecycle_deadline_ns": self.record.lifecycle_deadline_ns,
            "receipt_bindings": receipt_bindings,
            "permissions_granted": false,
            "weights_attested": false,
            "response_instance_attested": false,
            "inference_dispatched": false,
            "scope_only": true,
            "runtime_bridge_connected": false,
        }))
    }

    fn publish_checkpoint(&mut self, bindings: BTreeMap<String, Binding>) -> Result<()> {
        ensure_owner(&self.owner, &self.store)?;
        self.check_deadline()?;
        if bindings.len() > self.record.policy.models.len() || bindings.len() > MAX_CHECKPOINTS {
            bail!("Native client scope receipt bindings exceed policy bounds")
        }
        if self.checkpoint_sequence as usize >= MAX_CHECKPOINTS {
            bail!("Native client scope checkpoint capacity exhausted")
        }
        for (model, pinned) in &self.bindings {
            let Some(candidate) = bindings.get(model) else {
                bail!("Native client scope checkpoints cannot remove existing receipt pins")
            };
            if serde_json::to_value(candidate)? != serde_json::to_value(pinned)? {
                bail!("Native client scope checkpoints cannot replace existing receipt pins")
            }
        }
        for (model, binding) in &bindings {
            validate_binding(&self.record.policy, model, binding)?;
        }
        let mut checkpoint = Checkpoint {
            format_version: 1,
            root_task_id: self.record.root_task_id.clone(),
            sequence: self.checkpoint_sequence + 1,
            previous_sha256: self.checkpoint_sha256.clone(),
            policy_sha256: self.record.policy_sha256.clone(),
            configuration_sha256: self.record.configuration_sha256.clone(),
            bindings_sha256: digest(&bindings)?,
            bindings,
            checkpoint_sha256: String::new(),
            permissions_granted: false,
            weights_attested: false,
            response_instance_check_required: true,
            response_instance_attested: false,
            inference_dispatched: false,
        };
        checkpoint.checkpoint_sha256 = checkpoint_digest(&checkpoint)?;
        let mut db = self.store.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        active_task_state(&tx, &self.record.root_task_id)?;
        let _scan_deadline = bounded_sql_scan(&tx);
        let current: Option<String> = tx.query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id DESC LIMIT 1",
            params![self.record.root_task_id, CHECKPOINT_KIND, SOURCE],
            |row| row.get(0),
        ).optional()?;
        let expected = if self.checkpoint_sequence == 0 {
            if current.is_some() {
                bail!("Unexpected native client checkpoint exists before genesis")
            }
            sha256(b"agent-native-scope-genesis-v1")
        } else {
            let current = current
                .ok_or_else(|| anyhow::anyhow!("Native client checkpoint chain disappeared"))?;
            let value = crate::strict_json::value(current.as_bytes(), MAX_SCOPE_BYTES)?;
            let current: Checkpoint = serde_json::from_value(value)?;
            current.checkpoint_sha256
        };
        if expected != self.checkpoint_sha256 {
            bail!("Native client scope checkpoint changed concurrently")
        }
        insert_event_tx(&tx, &self.record.root_task_id, CHECKPOINT_KIND, &checkpoint)?;
        drop(_scan_deadline);
        tx.commit()?;
        self.bindings = checkpoint.bindings;
        self.checkpoint_sequence = checkpoint.sequence;
        self.checkpoint_sha256 = checkpoint.checkpoint_sha256;
        Ok(())
    }

    fn check_deadline(&self) -> Result<()> {
        let (boot, now) = crate::execution_clock::observation()
            .ok_or_else(|| anyhow::anyhow!("Native client scope clock is unavailable"))?;
        if boot != self.record.boot_id || now >= self.record.lifecycle_deadline_ns {
            bail!("Original native client lifecycle deadline expired or belongs to another boot")
        }
        Ok(())
    }

    fn remaining(&self) -> Result<Duration> {
        let (boot, now) = crate::execution_clock::observation()
            .ok_or_else(|| anyhow::anyhow!("Native client scope clock is unavailable"))?;
        if boot != self.record.boot_id {
            bail!("Native client scope belongs to another boot")
        }
        let remaining = self
            .record
            .lifecycle_deadline_ns
            .checked_sub(now)
            .filter(|remaining| *remaining > 0)
            .ok_or_else(|| anyhow::anyhow!("Original native client lifecycle deadline expired"))?;
        Ok(Duration::from_nanos(remaining))
    }
}

fn validate_binding(policy: &NativeLifecyclePolicy, model: &str, binding: &Binding) -> Result<()> {
    let ensure = policy
        .models
        .iter()
        .find(|item| item.model == model)
        .ok_or_else(|| {
            anyhow::anyhow!("Native client receipt is outside the approved model set")
        })?;
    let receipt = &binding.receipt;
    if binding.origin != "reused"
        || receipt.model_key != model
        || receipt.context_length != ensure.context_length
        || receipt.offload_kv_cache_to_gpu != ensure.offload_kv_cache_to_gpu
        || receipt.endpoint != inventory_endpoint(policy)?
    {
        bail!("Native client receipt or origin differs from approved scope")
    }
    Ok(())
}
