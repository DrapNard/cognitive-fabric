//! Bounded native model activation journal and runtime leases for a client scope.
use super::*;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const ACTIVATION_KIND: &str = "agent_native_activation";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActivationEvent {
    format_version: u32,
    state: String,
    root_task_id: String,
    endpoint: String,
    policy_sha256: String,
    scope_sha256: String,
    model: String,
    context_length: u32,
    offload_kv_cache_to_gpu: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    load_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operator_stop_acknowledged: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unreviewed_receipt_ids: Vec<String>,
}

fn activation_payload(
    record: &ScopeRecord,
    model: &str,
    state: &str,
    receipt: Option<&managed::Receipt>,
) -> Result<ActivationEvent> {
    let ensure = record
        .policy
        .models
        .iter()
        .find(|item| item.model == model)
        .ok_or_else(|| anyhow::anyhow!("Activation model is outside the pinned native policy"))?;
    if !matches!(
        state,
        "activation_started" | "activation_confirmed" | "activation_outcome_unknown"
    ) {
        bail!("Invalid native model activation state")
    }
    if let Some(receipt) = receipt {
        if receipt.model_key != model
            || receipt.context_length != ensure.context_length
            || receipt.offload_kv_cache_to_gpu != ensure.offload_kv_cache_to_gpu
        {
            bail!("Native activation receipt differs from the exact pinned model settings")
        }
    }
    Ok(ActivationEvent {
        format_version: 1,
        state: state.to_owned(),
        root_task_id: record.root_task_id.clone(),
        endpoint: inventory_endpoint(&record.policy)?,
        policy_sha256: record.policy_sha256.clone(),
        scope_sha256: record.scope_sha256.clone(),
        model: model.to_owned(),
        context_length: ensure.context_length,
        offload_kv_cache_to_gpu: ensure.offload_kv_cache_to_gpu,
        receipt_id: receipt.map(|item| item.receipt_id.clone()),
        load_task_id: receipt.map(|item| item.load_task_id.clone()),
        operator_stop_acknowledged: None,
        original_outcome: None,
        unreviewed_receipt_ids: Vec::new(),
    })
}

fn persist_activation_event(
    store: &Store,
    record: &ScopeRecord,
    model: &str,
    state: &str,
    receipt: Option<&managed::Receipt>,
) -> Result<()> {
    let event = activation_payload(record, model, state, receipt)?;
    if serde_json::to_vec(&event)?.len() > 4096 {
        bail!("Native model activation event exceeds bounds")
    }
    store.event(
        &record.root_task_id,
        ACTIVATION_KIND,
        SOURCE,
        serde_json::to_value(event)?,
    )
}

fn same_activation_identity(left: &ActivationEvent, right: &ActivationEvent) -> bool {
    left.root_task_id == right.root_task_id
        && left.endpoint == right.endpoint
        && left.policy_sha256 == right.policy_sha256
        && left.scope_sha256 == right.scope_sha256
        && left.model == right.model
        && left.context_length == right.context_length
        && left.offload_kv_cache_to_gpu == right.offload_kv_cache_to_gpu
}

/// Fail closed if any scope at this native endpoint has an unresolved load.
/// This prevents a receipt created between provider success and its durable
/// scope checkpoint from being adopted by a different client task.
pub(super) fn ensure_no_pending_activation(store: &Store, endpoint: &str) -> Result<()> {
    let db = store.lock()?;
    let _scan_deadline = bounded_sql_scan(&db);
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END,task_id FROM events WHERE kind=?1 AND source=?2 ORDER BY id LIMIT 16385",
    )?;
    let rows = statement
        .query_map(params![ACTIVATION_KIND, SOURCE], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 16_384 {
        bail!("Native activation history exceeds its bounded endpoint scan")
    }
    let mut states = BTreeMap::<(String, String), ActivationEvent>::new();
    let mut confirmed = Vec::new();
    let mut stopped_receipts = BTreeSet::new();
    for (raw, task) in rows {
        let value = crate::strict_json::value(raw.as_bytes(), 4096)?;
        let event: ActivationEvent = serde_json::from_value(value)?;
        if event.format_version != 1
            || event.root_task_id != task
            || uuid::Uuid::parse_str(&event.root_task_id).is_err()
        {
            bail!("Native activation history contains malformed or conflicting scope evidence")
        }
        if event.endpoint != endpoint {
            continue;
        }
        let key = (event.root_task_id.clone(), event.model.clone());
        let previous = states.get(&key);
        match (
            previous.map(|previous| previous.state.as_str()),
            event.state.as_str(),
        ) {
            (None, "activation_started") => {
                if event.receipt_id.is_some()
                    || event.load_task_id.is_some()
                    || event.operator_stop_acknowledged.is_some()
                    || event.original_outcome.is_some()
                    || !event.unreviewed_receipt_ids.is_empty()
                {
                    bail!("Native activation intent contains an unapproved receipt identity")
                }
                states.insert(key, event);
            }
            (Some("activation_started"), "activation_confirmed") => {
                let start = previous.expect("matched an existing start");
                if !same_activation_identity(start, &event)
                    || event
                        .receipt_id
                        .as_deref()
                        .is_none_or(|id| uuid::Uuid::parse_str(id).is_err())
                    || event
                        .load_task_id
                        .as_deref()
                        .is_none_or(|id| uuid::Uuid::parse_str(id).is_err())
                    || event.operator_stop_acknowledged.is_some()
                    || event.original_outcome.is_some()
                    || !event.unreviewed_receipt_ids.is_empty()
                    || event.root_task_id != start.root_task_id
                    || event.endpoint != start.endpoint
                    || event.policy_sha256 != start.policy_sha256
                    || event.scope_sha256 != start.scope_sha256
                    || event.context_length != start.context_length
                    || event.offload_kv_cache_to_gpu != start.offload_kv_cache_to_gpu
                {
                    bail!("Confirmed native activation lacks its exact managed receipt identity")
                }
                confirmed.push(event.clone());
                states.insert(key, event);
            }
            (Some("activation_started"), "activation_outcome_unknown") => {
                let start = previous.expect("matched an existing start");
                if !same_activation_identity(start, &event)
                    || event.receipt_id.is_some()
                    || event.load_task_id.is_some()
                    || event.operator_stop_acknowledged.is_some()
                    || event.original_outcome.is_some()
                {
                    bail!("Unknown native activation contains an unverified receipt identity")
                }
                states.insert(key, event);
            }
            (Some("activation_outcome_unknown"), "activation_operator_stopped")
            | (Some("activation_started"), "activation_operator_stopped") => {
                let prior = previous.expect("matched an existing activation intent");
                let mut event_receipts = BTreeSet::new();
                if !same_activation_identity(prior, &event)
                    || event.receipt_id.is_some()
                    || event.load_task_id.is_some()
                    || event.operator_stop_acknowledged != Some(true)
                    || event.original_outcome.as_deref() != Some("unknown")
                    || event.unreviewed_receipt_ids.len() > 32
                    || event.unreviewed_receipt_ids.iter().any(|id| {
                        uuid::Uuid::parse_str(id).is_err() || !event_receipts.insert(id.clone())
                    })
                {
                    bail!(
                        "Native activation stop acknowledgement does not match its unknown intent"
                    )
                }
                stopped_receipts.extend(event_receipts);
                states.insert(key, event);
            }
            _ => bail!("Native activation history has an invalid state transition"),
        }
    }
    if states.values().any(|event| {
        !matches!(
            event.state.as_str(),
            "activation_confirmed" | "activation_operator_stopped"
        )
    }) {
        bail!("A native model activation has an unknown or unresolved outcome; operator resolution is required")
    }
    for event in &confirmed {
        validate_confirmed_checkpoint(&db, event)?;
    }
    drop(statement);
    drop(_scan_deadline);
    drop(db);
    for receipt_id in stopped_receipts {
        let receipt = managed::inspect(store, &receipt_id)?;
        if !matches!(receipt["state"].as_str(), Some("released" | "retired")) {
            bail!("A stopped activation has a pinned managed receipt that still requires operator reconciliation")
        }
    }
    Ok(())
}

fn validate_confirmed_checkpoint(db: &rusqlite::Connection, event: &ActivationEvent) -> Result<()> {
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT 2",
    )?;
    let rows = statement
        .query_map(
            params![event.root_task_id, super::SCOPE_KIND, super::SOURCE],
            |row| row.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 1 {
        bail!("Confirmed native activation has no unique root scope record")
    }
    let value = crate::strict_json::value(rows[0].as_bytes(), super::MAX_SCOPE_BYTES)?;
    let record: ScopeRecord = serde_json::from_value(value)?;
    if record.format_version != 3
        || !record.on_demand
        || record.root_task_id != event.root_task_id
        || record.policy_sha256 != event.policy_sha256
        || record.scope_sha256 != event.scope_sha256
        || record.scope_sha256 != super::scope_digest(&record)?
        || super::inventory_endpoint(&record.policy)? != event.endpoint
    {
        bail!("Confirmed native activation differs from its exact on-demand scope")
    }
    drop(statement);
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id DESC LIMIT 1",
    )?;
    let raw: String = statement.query_row(
        params![event.root_task_id, super::CHECKPOINT_KIND, super::SOURCE],
        |row| row.get(0),
    )?;
    let value = crate::strict_json::value(raw.as_bytes(), super::MAX_SCOPE_BYTES)?;
    let checkpoint: Checkpoint = serde_json::from_value(value)?;
    if checkpoint.format_version != 3
        || checkpoint.root_task_id != event.root_task_id
        || !(1..=MAX_CHECKPOINTS as u32).contains(&checkpoint.sequence)
        || checkpoint.scope_sha256 != record.scope_sha256
        || checkpoint.policy_sha256 != record.policy_sha256
        || checkpoint.configuration_sha256 != record.configuration_sha256
        || checkpoint.bindings_sha256 != super::digest(&checkpoint.bindings)?
        || checkpoint.checkpoint_sha256 != super::checkpoint_digest(&checkpoint)?
        || checkpoint.permissions_granted
        || checkpoint.weights_attested
        || checkpoint.response_instance_check_required != record.response_instance_check_required
        || checkpoint.response_instance_attested
        || checkpoint.inference_dispatched
        || !checkpoint
            .bindings
            .get(&event.model)
            .is_some_and(|binding| {
                binding.origin == "newly_loaded"
                    && Some(binding.receipt.receipt_id.as_str()) == event.receipt_id.as_deref()
                    && Some(binding.receipt.load_task_id.as_str()) == event.load_task_id.as_deref()
                    && super::validate_binding(&record.policy, &event.model, binding).is_ok()
            })
    {
        bail!("Confirmed native activation has no exact durable scope receipt checkpoint")
    }
    Ok(())
}

pub(super) fn validate_activation_history(
    db: &rusqlite::Connection,
    root_task: &str,
    record: &ScopeRecord,
    bindings: &BTreeMap<String, Binding>,
) -> Result<()> {
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT 17",
    )?;
    let rows = statement
        .query_map(params![root_task, ACTIVATION_KIND, SOURCE], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 16 {
        bail!("Native activation history exceeds scope bounds")
    }
    let mut states = BTreeMap::<String, String>::new();
    let endpoint = inventory_endpoint(&record.policy)?;
    for raw in rows {
        let value = crate::strict_json::value(raw.as_bytes(), 4096)?;
        let event: ActivationEvent = serde_json::from_value(value)?;
        let expected = activation_payload(record, &event.model, &event.state, None)?;
        if event.format_version != 1
            || event.root_task_id != root_task
            || event.endpoint != endpoint
            || event.policy_sha256 != record.policy_sha256
            || event.scope_sha256 != record.scope_sha256
            || event.context_length != expected.context_length
            || event.offload_kv_cache_to_gpu != expected.offload_kv_cache_to_gpu
            || (event.state == "activation_started"
                && (event.receipt_id.is_some()
                    || event.load_task_id.is_some()
                    || event.operator_stop_acknowledged.is_some()
                    || event.original_outcome.is_some()))
        {
            bail!("Native activation event differs from its pinned scope")
        }
        match (
            states.get(&event.model).map(String::as_str),
            event.state.as_str(),
        ) {
            (None, "activation_started") => {
                states.insert(event.model.clone(), event.state);
            }
            (Some("activation_started"), "activation_confirmed") => {
                if event.receipt_id.is_none()
                    || event.load_task_id.is_none()
                    || event.operator_stop_acknowledged.is_some()
                    || event.original_outcome.is_some()
                {
                    bail!("Confirmed native activation lacks its exact managed receipt identity")
                }
                let binding = bindings.get(&event.model).filter(|item| {
                    item.origin == "newly_loaded"
                        && event.receipt_id.as_deref() == Some(item.receipt.receipt_id.as_str())
                        && event.load_task_id.as_deref() == Some(item.receipt.load_task_id.as_str())
                });
                if binding.is_none() {
                    bail!(
                        "Confirmed native activation has no exact durable scope receipt checkpoint"
                    )
                }
                states.insert(event.model.clone(), event.state);
            }
            (Some("activation_started"), "activation_outcome_unknown") => {
                bail!("Native activation outcome is unknown; operator resolution is required")
            }
            _ => bail!("Native activation history has an invalid state transition"),
        }
    }
    if states.values().any(|state| state != "activation_confirmed")
        || bindings.iter().any(|(model, binding)| {
            binding.origin == "newly_loaded"
                && states.get(model).map(String::as_str) != Some("activation_confirmed")
        })
    {
        bail!("Native activation and scope receipt histories disagree")
    }
    Ok(())
}

impl NativeClientScope {
    /// Permanently retire a stopped on-demand activation scope after an
    /// operator confirms the provider has stopped. This does not resolve a
    /// managed load or inference journal and never re-enables the old session.
    pub fn acknowledge_stopped_activation(
        store: Arc<Store>,
        cfg: &Config,
        root_task: &str,
        model: &str,
        human_approve: bool,
        provider_stop_acknowledged: bool,
    ) -> Result<Value> {
        if !human_approve || !provider_stop_acknowledged {
            bail!("Stopping an unknown native activation requires explicit operator approval and provider-stop acknowledgement")
        }
        crate::security::authorize("human-cli", "model.residency.change")?;
        uuid::Uuid::parse_str(root_task)?;
        let mut expected_runtime_lock = cfg.db().into_os_string();
        expected_runtime_lock.push(".runtime.lock");
        if store
            .runtime_lock
            .as_deref()
            .map(std::path::Path::as_os_str)
            != Some(expected_runtime_lock.as_os_str())
        {
            bail!("Native activation resolution requires the configured persistent workspace database")
        }
        let owner = Ownership::acquire(store.clone())?;
        ensure_owner(&owner, &store)?;
        let preliminary_record = {
            let db = store.lock()?;
            let _scan_deadline = bounded_sql_scan(&db);
            let mut statement = db.prepare(
                "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT 2",
            )?;
            let rows = statement
                .query_map(
                    params![root_task, super::SCOPE_KIND, super::SOURCE],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if rows.len() != 1 {
                bail!("Activation resolution requires one exact original scope record")
            }
            let value = crate::strict_json::value(rows[0].as_bytes(), super::MAX_SCOPE_BYTES)?;
            let record: ScopeRecord = serde_json::from_value(value)?;
            if record.format_version != 3
                || !record.on_demand
                || record.root_task_id != root_task
                || record.configuration_sha256 != super::digest(cfg)?
                || record.policy_sha256 != super::digest(&record.policy)?
                || record.scope_sha256 != super::scope_digest(&record)?
                || super::validate_configuration(cfg, &record.policy).is_err()
                || !record.policy.models.iter().any(|item| item.model == model)
            {
                bail!("Activation resolution differs from the original on-demand scope and configuration")
            }
            record
        };
        crate::security::authorize("human-cli", "model.residency.inspect")?;
        let local_receipts = managed::list(&store, &preliminary_record.policy.api_base)?;
        let rows = local_receipts["receipts"]
            .as_array()
            .filter(|rows| rows.len() <= 1024)
            .ok_or_else(|| {
                anyhow::anyhow!("Managed receipt inventory is malformed or over limit")
            })?;
        let mut unreviewed_receipt_ids = Vec::new();
        for row in rows {
            let state = row["state"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Managed receipt state is malformed"))?;
            if matches!(state, "released" | "retired") {
                continue;
            }
            let receipt: managed::Receipt = serde_json::from_value(row["receipt"].clone())?;
            if receipt.model_key == model {
                uuid::Uuid::parse_str(&receipt.receipt_id)?;
                unreviewed_receipt_ids.push(receipt.receipt_id);
                if unreviewed_receipt_ids.len() > 32 {
                    bail!("More than 32 managed receipts require reconciliation for this model")
                }
            }
        }
        unreviewed_receipt_ids.sort();
        unreviewed_receipt_ids.dedup();
        let db = store.lock()?;
        let _scan_deadline = bounded_sql_scan(&db);
        let task_status: String = db.query_row(
            "SELECT CASE WHEN length(CAST(status AS BLOB))<=64 THEN status END FROM tasks WHERE id=?1",
            [root_task],
            |row| row.get(0),
        )?;
        if !matches!(
            task_status.as_str(),
            "interrupted" | "failed" | "cancelled" | "exhausted"
        ) {
            bail!(
                "Only a terminal interrupted or failed client task can retire an activation intent"
            )
        }
        let mut statement = db.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT 2",
        )?;
        let scopes = statement
            .query_map(
                params![root_task, super::SCOPE_KIND, super::SOURCE],
                |row| row.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if scopes.len() != 1 {
            bail!("Activation resolution requires one exact original scope record")
        }
        let value = crate::strict_json::value(scopes[0].as_bytes(), super::MAX_SCOPE_BYTES)?;
        let record: ScopeRecord = serde_json::from_value(value)?;
        if record.format_version != 3
            || !record.on_demand
            || record.root_task_id != root_task
            || record.configuration_sha256 != super::digest(cfg)?
            || record.policy_sha256 != super::digest(&record.policy)?
            || record.scope_sha256 != super::scope_digest(&record)?
            || super::validate_configuration(cfg, &record.policy).is_err()
            || record.scope_sha256 != preliminary_record.scope_sha256
        {
            bail!(
                "Activation resolution differs from the original on-demand scope and configuration"
            )
        }
        let expected_endpoint = super::inventory_endpoint(&record.policy)?;
        let intent = activation_payload(&record, model, "activation_started", None)?;
        if intent.endpoint != expected_endpoint {
            bail!("Activation resolution endpoint differs from the original operator policy")
        }
        drop(statement);
        let mut statement = db.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT 25",
        )?;
        let rows = statement
            .query_map(params![root_task, ACTIVATION_KIND, SOURCE], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() > 24 {
            bail!("Activation journal exceeds its bounded history")
        }
        let mut matching = Vec::new();
        for raw in rows {
            let value = crate::strict_json::value(raw.as_bytes(), 4096)?;
            let event: ActivationEvent = serde_json::from_value(value)?;
            if event.model == model {
                matching.push(event);
            }
        }
        if !matches!(matching.len(), 1 | 2)
            || matching[0].state != "activation_started"
            || !same_activation_identity(&intent, &matching[0])
            || (matching.len() == 2
                && (matching[1].state != "activation_outcome_unknown"
                    || !same_activation_identity(&matching[0], &matching[1])))
            || matching.iter().any(|event| {
                event.receipt_id.is_some()
                    || event.load_task_id.is_some()
                    || event.operator_stop_acknowledged.is_some()
                    || event.original_outcome.is_some()
                    || !event.unreviewed_receipt_ids.is_empty()
            })
        {
            bail!("Activation resolution requires the unique original unresolved intent for this model")
        }
        let mut resolution = matching.last().expect("matched activation event").clone();
        resolution.state = "activation_operator_stopped".into();
        resolution.operator_stop_acknowledged = Some(true);
        resolution.original_outcome = Some("unknown".into());
        resolution.unreviewed_receipt_ids = unreviewed_receipt_ids;
        drop(statement);
        drop(_scan_deadline);
        drop(db);
        store.event(
            root_task,
            ACTIVATION_KIND,
            SOURCE,
            serde_json::to_value(&resolution)?,
        )?;
        Ok(serde_json::json!({
            "status": "activation_operator_stopped",
            "root_task_id": root_task,
            "model_key": model,
            "activation_scope_sha256": resolution.scope_sha256,
            "operator_stop_acknowledged": true,
            "original_outcome": "unknown",
            "provider_stop_proven": false,
            "old_session_resumable": false,
            "automatic_retry": false,
            "managed_receipts_requiring_reconciliation": resolution.unreviewed_receipt_ids.len(),
            "managed_load_or_generation_resolution": "separate_operator_review_required"
        }))
    }

    /// Ensure only the selected model is available. In on-demand mode, a new
    /// load is journaled before provider mutation and is never retried after an
    /// uncertain outcome.
    pub(crate) async fn ensure_model(&self, model: &str) -> Result<()> {
        ensure_owner(&self.owner, &self.store)?;
        self.check_deadline()?;
        let ensure = self
            .record
            .policy
            .models
            .iter()
            .find(|item| item.model == model)
            .ok_or_else(|| {
                anyhow::anyhow!("Requested model is outside the pinned native policy")
            })?;
        if self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Native scope state lock is poisoned"))?
            .bindings
            .contains_key(model)
        {
            return Ok(());
        }
        let _activation = self.activation_gate.write().await;
        ensure_no_pending_activation(&self.store, &inventory_endpoint(&self.record.policy)?)?;
        let existing = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Native scope state lock is poisoned"))?
            .bindings
            .get(model)
            .cloned();
        if existing.is_some() {
            self.validate_receipt_for(model, &self.record.policy.api_base)
                .await?;
            return Ok(());
        }
        if !self.record.on_demand {
            bail!("Requested model has no pinned existing receipt and on-demand activation is disabled")
        }
        let mut activation = ActivationGuard::start(self, model)?;
        let loaded = tokio::time::timeout(
            self.remaining()?,
            managed::load_owned_with_gpu(
                &self.store,
                &self.record.policy.api_base,
                self.record.policy.authenticate,
                model,
                ensure.context_length,
                ensure.offload_kv_cache_to_gpu,
                (true, Some(&self.owner)),
            ),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!("Native model activation exceeded the original scope deadline")
        })??;
        let receipt: managed::Receipt = serde_json::from_value(loaded["receipt"].clone())?;
        let status = tokio::time::timeout(
            self.remaining()?,
            residency::status(
                &self.record.policy.api_base,
                self.record.policy.authenticate,
            ),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!("Native activation validation exceeded the original scope deadline")
        })??;
        let models: Vec<residency::Model> = serde_json::from_value(status["models"].clone())?;
        let observed = managed::validate_task_binding(
            &self.store,
            &receipt.receipt_id,
            &self.record.policy.api_base,
            &models,
        )?;
        if serde_json::to_value(&receipt)? != serde_json::to_value(&observed)? {
            bail!("New native receipt differs from the freshly observed managed instance")
        }
        let mut next = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Native scope state lock is poisoned"))?
            .bindings
            .clone();
        if next.contains_key(model) {
            bail!("Native model receipt appeared concurrently during activation")
        }
        next.insert(
            model.to_owned(),
            Binding {
                receipt: observed.clone(),
                origin: "newly_loaded".into(),
            },
        );
        self.publish_checkpoint(next)?;
        activation.confirm(&observed)?;
        Ok(())
    }

    /// Hold a shared activation lease during provider generation. The write
    /// side is used only around model activation and is never held by inference.
    pub(crate) async fn generation_guard(&self) -> Result<tokio::sync::RwLockReadGuard<'_, ()>> {
        let guard = self.activation_gate.read().await;
        self.check_deadline()?;
        ensure_no_pending_activation(&self.store, &inventory_endpoint(&self.record.policy)?)?;
        Ok(guard)
    }

    /// Release only receipts loaded by this scope, and only when the pinned
    /// policy requests it. Call after the final client-session checkpoint.
    pub(crate) async fn finish_session(&self) -> Result<()> {
        let _activation = self.activation_gate.write().await;
        self.check_deadline()?;
        ensure_no_pending_activation(&self.store, &inventory_endpoint(&self.record.policy)?)?;
        crate::residency::lifecycle::require_no_unknown_generation(
            &self.store,
            &inventory_endpoint(&self.record.policy)?,
        )?;
        if !self.record.policy.release_after {
            return Ok(());
        }
        let owned = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Native scope state lock is poisoned"))?
            .bindings
            .values()
            .filter(|binding| binding.origin == "newly_loaded")
            .map(|binding| binding.receipt.receipt_id.clone())
            .collect::<Vec<_>>();
        for receipt_id in owned {
            self.check_deadline()?;
            tokio::time::timeout(
                self.remaining()?,
                managed::release_owned(
                    &self.store,
                    &self.record.policy.api_base,
                    self.record.policy.authenticate,
                    &receipt_id,
                    (true, Some(&self.owner)),
                ),
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("Native receipt cleanup exceeded the original scope deadline")
            })??;
        }
        Ok(())
    }
}

struct ActivationGuard<'a> {
    scope: &'a NativeClientScope,
    model: String,
    terminal: bool,
}

impl<'a> ActivationGuard<'a> {
    fn start(scope: &'a NativeClientScope, model: &str) -> Result<Self> {
        persist_activation_event(
            &scope.store,
            &scope.record,
            model,
            "activation_started",
            None,
        )?;
        Ok(Self {
            scope,
            model: model.to_owned(),
            terminal: false,
        })
    }

    fn confirm(&mut self, receipt: &managed::Receipt) -> Result<()> {
        persist_activation_event(
            &self.scope.store,
            &self.scope.record,
            &self.model,
            "activation_confirmed",
            Some(receipt),
        )?;
        self.terminal = true;
        Ok(())
    }
}

impl Drop for ActivationGuard<'_> {
    fn drop(&mut self) {
        if !self.terminal {
            let _ = persist_activation_event(
                &self.scope.store,
                &self.scope.record,
                &self.model,
                "activation_outcome_unknown",
                None,
            );
        }
    }
}
