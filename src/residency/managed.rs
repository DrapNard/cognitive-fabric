//! Explicitly approved native residency with durable, bounded local receipts.
//!
//! Receipts are causal records for this workspace's native request. They do not
//! attest provider identity, exclusive ownership, weights, or inference routing.
pub mod resolution;
pub mod retirement;

use super::{
    endpoint, identity, mutation_leases, parse, request, status, Instance, Model, Mutation,
};
use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use uuid::Uuid;

const MAX_RECEIPTS: i64 = 1024;
const MAX_EVENT_BYTES: usize = 4096;
const RESIDENCY_HISTORY_QUERY: &str = "SELECT CASE WHEN length(CAST(e.payload AS BLOB))<=4096 THEN e.payload END,CASE WHEN length(CAST(t.status AS BLOB))<=64 THEN t.status END,e.task_id FROM events e JOIN tasks t ON t.id=e.task_id WHERE e.kind='model_residency' AND e.source='human-cli' ORDER BY e.id LIMIT 36865";

struct ProgressHandlerReset<'a>(&'a rusqlite::Connection);

impl Drop for ProgressHandlerReset<'_> {
    fn drop(&mut self) {
        self.0.progress_handler(0, None::<fn() -> bool>);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub format_version: u32,
    pub receipt_id: String,
    pub adapter: String,
    pub endpoint: String,
    pub model_key: String,
    pub model_type: String,
    pub context_length: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offload_kv_cache_to_gpu: Option<bool>,
    pub instance_id: String,
    pub load_task_id: String,
    /// Inventory at receipt creation; the mutable ledger digest advances only
    /// after another exact, confirmed managed transition on this endpoint.
    pub initial_inventory_sha256: String,
}

#[derive(Debug)]
struct StoredReceipt {
    receipt: Receipt,
    current_inventory_sha256: String,
    state: String,
    release_task_id: Option<String>,
}

struct ReceiptRow {
    receipt_id: String,
    endpoint: String,
    instance_id: String,
    receipt_json: String,
    receipt_sha256: String,
    current_inventory_sha256: String,
    state: String,
    load_task_id: String,
    release_task_id: Option<String>,
}

fn receipt_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReceiptRow> {
    Ok(ReceiptRow {
        receipt_id: row.get(0)?,
        endpoint: row.get(1)?,
        instance_id: row.get(2)?,
        receipt_json: row.get(3)?,
        receipt_sha256: row.get(4)?,
        current_inventory_sha256: row.get(5)?,
        state: row.get(6)?,
        load_task_id: row.get(7)?,
        release_task_id: row.get(8)?,
    })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn validate_receipt(receipt: &Receipt) -> Result<()> {
    if !matches!(receipt.format_version, 1 | 2)
        || receipt.adapter != "lmstudio-native-v1"
        || Uuid::parse_str(&receipt.receipt_id).is_err()
        || canonical_inventory_endpoint(&receipt.endpoint)? != receipt.endpoint
        || !identity(&receipt.model_key)
        || !matches!(
            receipt.model_type.as_str(),
            "llm" | "embedding" | "decision"
        )
        || !(1..=32768).contains(&receipt.context_length)
        || !identity(&receipt.instance_id)
        || Uuid::parse_str(&receipt.load_task_id).is_err()
        || !valid_digest(&receipt.initial_inventory_sha256)
        || (receipt.format_version == 1 && receipt.offload_kv_cache_to_gpu.is_some())
        || (receipt.format_version == 2
            && (receipt.offload_kv_cache_to_gpu.is_none()
                || !matches!(receipt.model_type.as_str(), "llm" | "decision")))
    {
        bail!("Invalid managed residency receipt")
    }
    Ok(())
}

fn canonical_inventory_endpoint(value: &str) -> Result<String> {
    let mut url = url::Url::parse(value)?;
    if url.path() != "/api/v1/models" || url.as_str() != value {
        bail!("Managed receipt endpoint is not canonical")
    }
    url.set_path("/api/v1");
    let canonical = endpoint(url.as_str())?.to_string();
    if canonical != value {
        bail!("Managed receipt endpoint is not a canonical local native endpoint")
    }
    Ok(canonical)
}

fn receipt_hash(receipt: &Receipt) -> Result<(String, String)> {
    validate_receipt(receipt)?;
    let bytes = serde_json::to_vec(receipt)?;
    if bytes.len() > 8192 {
        bail!("Managed residency receipt exceeds bounds")
    }
    Ok((String::from_utf8(bytes.clone())?, sha256(&bytes)))
}

fn decode_row(row: ReceiptRow) -> Result<StoredReceipt> {
    if row.receipt_json.len() > 8192
        || sha256(row.receipt_json.as_bytes()) != row.receipt_sha256
        || !valid_digest(&row.current_inventory_sha256)
        || !matches!(
            row.state.as_str(),
            "active" | "release_requested" | "released" | "quarantined" | "retired"
        )
    {
        bail!("Managed residency receipt integrity check failed")
    }
    let receipt: Receipt = crate::strict_json::value(row.receipt_json.as_bytes(), 8192)
        .and_then(|value| Ok(serde_json::from_value(value)?))
        .map_err(|_| anyhow::anyhow!("Invalid managed residency receipt JSON"))?;
    validate_receipt(&receipt)?;
    if serde_json::to_vec(&receipt)?.as_slice() != row.receipt_json.as_bytes() {
        bail!("Managed residency receipt is not canonical JSON")
    }
    if receipt.receipt_id != row.receipt_id
        || receipt.endpoint != row.endpoint
        || receipt.instance_id != row.instance_id
        || receipt.load_task_id != row.load_task_id
        || row
            .release_task_id
            .as_ref()
            .is_some_and(|id| Uuid::parse_str(id).is_err())
    {
        bail!("Managed residency receipt identity mismatch")
    }
    Ok(StoredReceipt {
        receipt,
        current_inventory_sha256: row.current_inventory_sha256,
        state: row.state,
        release_task_id: row.release_task_id,
    })
}

fn load_receipt(store: &Store, receipt_id: &str) -> Result<StoredReceipt> {
    Uuid::parse_str(receipt_id)?;
    let db = store.lock()?;
    let row = db.query_row(
        "SELECT CASE WHEN length(CAST(receipt_id AS BLOB))<=64 THEN receipt_id END,CASE WHEN length(CAST(endpoint AS BLOB))<=512 THEN endpoint END,CASE WHEN length(CAST(instance_id AS BLOB))<=256 THEN instance_id END,CASE WHEN length(CAST(receipt_json AS BLOB))<=8192 THEN receipt_json END,CASE WHEN length(CAST(receipt_sha256 AS BLOB))=64 THEN receipt_sha256 END,CASE WHEN length(CAST(current_inventory_sha256 AS BLOB))=64 THEN current_inventory_sha256 END,CASE WHEN length(CAST(state AS BLOB))<=32 THEN state END,CASE WHEN length(CAST(load_task_id AS BLOB))<=64 THEN load_task_id END,CASE WHEN length(CAST(release_task_id AS BLOB))<=64 THEN release_task_id END FROM managed_residency_receipts WHERE receipt_id=?1",
        [receipt_id],
        receipt_row_from_sql,
    )?;
    decode_row(row)
}

// Archived receipts supply evidence only. Public release still uses the hot ledger.
fn load_receipt_evidence(store: &Store, id: &str) -> Result<StoredReceipt> {
    let hot = {
        let db = store.lock()?;
        db.query_row(
            "SELECT EXISTS(SELECT 1 FROM managed_residency_receipts WHERE receipt_id=?1)",
            [id],
            |r| r.get::<_, bool>(0),
        )?
    };
    if hot {
        load_receipt(store, id)
    } else {
        retirement::load_archive(store, id)?
            .ok_or_else(|| anyhow::anyhow!("Receipt evidence is missing"))
    }
}

fn model_instance<'a>(models: &'a [Model], receipt: &Receipt) -> Option<&'a Instance> {
    models
        .iter()
        .find(|model| model.key == receipt.model_key && model.kind == receipt.model_type)
        .and_then(|model| {
            model
                .loaded_instances
                .iter()
                .find(|instance| instance.id == receipt.instance_id)
        })
        .filter(|instance| {
            instance.config.context_length == u64::from(receipt.context_length)
                && receipt.offload_kv_cache_to_gpu.is_none_or(|expected| {
                    instance.config.offload_kv_cache_to_gpu == Some(expected)
                })
        })
}

/// Validates every live receipt against the exact observed inventory and digest.
fn active_cohort(
    store: &Store,
    endpoint: &str,
    models: &[Model],
    digest: &str,
) -> Result<Vec<StoredReceipt>> {
    if !valid_digest(digest) {
        bail!("Invalid managed inventory digest")
    }
    let db = store.lock()?;
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(receipt_id AS BLOB))<=64 THEN receipt_id END,CASE WHEN length(CAST(endpoint AS BLOB))<=512 THEN endpoint END,CASE WHEN length(CAST(instance_id AS BLOB))<=256 THEN instance_id END,CASE WHEN length(CAST(receipt_json AS BLOB))<=8192 THEN receipt_json END,CASE WHEN length(CAST(receipt_sha256 AS BLOB))=64 THEN receipt_sha256 END,CASE WHEN length(CAST(current_inventory_sha256 AS BLOB))=64 THEN current_inventory_sha256 END,CASE WHEN length(CAST(state AS BLOB))<=32 THEN state END,CASE WHEN length(CAST(load_task_id AS BLOB))<=64 THEN load_task_id END,CASE WHEN length(CAST(release_task_id AS BLOB))<=64 THEN release_task_id END FROM managed_residency_receipts WHERE endpoint=?1 AND state NOT IN ('released','retired') ORDER BY created,receipt_id LIMIT 1025",
    )?;
    let rows = statement
        .query_map([endpoint], receipt_row_from_sql)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > MAX_RECEIPTS as usize {
        bail!("Managed receipt cohort exceeds bounds")
    }
    let mut receipts = Vec::with_capacity(rows.len());
    for row in rows {
        let saved = decode_row(row)?;
        if saved.state != "active"
            || saved.current_inventory_sha256 != digest
            || model_instance(models, &saved.receipt).is_none()
        {
            bail!("Managed receipt cohort needs operator reconciliation")
        }
        receipts.push(saved);
    }
    Ok(receipts)
}

/// Fail closed on interrupted/unknown provider operations. Read-only inspection
/// never clears these records or adopts observed instances.
fn pending_endpoint_operation(store: &Store, endpoint: &str) -> Result<()> {
    super::lifecycle::require_no_unknown_generation(store, endpoint)?;
    pending_endpoint_operation_with_limits(store, endpoint, Duration::from_secs(5), 1000)
}

fn pending_endpoint_operation_with_limits(
    store: &Store,
    endpoint: &str,
    timeout: Duration,
    instruction_interval: i32,
) -> Result<()> {
    let db = store.lock()?;
    let started = Instant::now();
    db.progress_handler(
        instruction_interval,
        Some(move || started.elapsed() > timeout),
    );
    let reset = ProgressHandlerReset(&db);
    let mut statement = db.prepare(RESIDENCY_HISTORY_QUERY)?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    drop(reset);
    drop(db);
    if rows.len() > 36864 || rows.iter().map(|(raw, _, _)| raw.len()).sum::<usize>() > 64_000_000 {
        bail!("Residency operation history exceeds managed bounds")
    }
    let mut groups: std::collections::BTreeMap<String, (String, Vec<Value>)> =
        std::collections::BTreeMap::new();
    for (raw, task_state, task_id) in rows {
        if started.elapsed() > timeout {
            bail!("Residency history deadline exhausted")
        }
        if raw.len() > MAX_EVENT_BYTES {
            bail!("Residency operation history contains oversized metadata")
        }
        let value = crate::strict_json::value(raw.as_bytes(), MAX_EVENT_BYTES)
            .map_err(|_| anyhow::anyhow!("Residency operation history is malformed"))?;
        if !value["state"].as_str().is_some_and(|state| {
            matches!(state, "requested" | "server_confirmed" | "outcome_unknown")
        }) {
            bail!("Residency operation history has an unsupported state")
        }
        Uuid::parse_str(&task_id)?;
        if value
            .get("managed")
            .is_some_and(|value| !value.is_boolean())
        {
            bail!("Residency operation history has a malformed managed marker")
        }
        if value["managed"] == true
            && (value["adapter"] != "lmstudio-native-v1"
                || value["endpoint"].as_str().is_none()
                || value["state"].as_str().is_none()
                || value
                    .get("task_id")
                    .is_some_and(|v| v.as_str() != Some(task_id.as_str())))
        {
            bail!("Managed residency event is missing required identity fields")
        }
        let entry = groups
            .entry(task_id)
            .or_insert_with(|| (task_state.clone(), Vec::new()));
        if entry.0 != task_state {
            bail!("Residency task status changed during history inspection")
        }
        entry.1.push(value);
    }
    for (task_id, (task_state, events)) in groups {
        if started.elapsed() > timeout {
            bail!("Residency history deadline exhausted")
        }
        let intent_positions = events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| (event["state"] == "requested").then_some(index))
            .collect::<Vec<_>>();
        if intent_positions.len() != 1 {
            if events.iter().any(|event| event["managed"] == true) {
                bail!("Managed residency task has missing or duplicate intent")
            }
            continue;
        }
        let intent_position = intent_positions[0];
        let intent = &events[intent_position];
        let event_endpoint = intent["endpoint"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Residency operation intent has no endpoint"))?;
        let managed = intent["managed"] == true;
        if events
            .iter()
            .any(|event| (event["managed"] == true) != managed)
        {
            bail!("Residency operation history has conflicting managed markers")
        }
        if managed && intent_position != 0 {
            bail!("Managed residency request does not precede its completion")
        }
        if managed
            && events
                .iter()
                .any(|event| event["endpoint"].as_str() != Some(event_endpoint))
        {
            bail!("Managed residency task has conflicting endpoint history")
        }
        if !managed {
            // Legacy explicit mutations never become receipts. An old request
            // is resolved only by its ordinary server-confirmed completion and
            // completed task; task status alone is not enough.
            let completion = events.iter().find(|event| !std::ptr::eq(*event, intent));
            let resolved = events.len() == 2
                && intent_position == 0
                && completion.is_some_and(|event| event["state"] == "server_confirmed")
                && task_state == "completed";
            if event_endpoint == endpoint && !resolved {
                bail!("Unresolved residency operation blocks managed lifecycle on this endpoint")
            }
            continue;
        }
        if resolution::is_resolved(store, &task_id, event_endpoint)? {
            continue;
        }
        if events.len() != 2 {
            if event_endpoint == endpoint {
                bail!("Managed residency task has incomplete or ambiguous completion history")
            }
            continue;
        }
        let completion = &events[1];
        if completion["endpoint"].as_str() != Some(event_endpoint) {
            bail!("Managed residency task has conflicting endpoint history")
        }
        let state = completion["state"].as_str().unwrap_or("");
        if state == "outcome_unknown" {
            if event_endpoint == endpoint {
                bail!("Unresolved residency operation blocks managed lifecycle on this endpoint")
            }
            continue;
        }
        if state != "server_confirmed" || task_state != "completed" {
            if event_endpoint == endpoint {
                bail!("Managed residency completion does not match its task state")
            }
            continue;
        }
        let action = intent["action"]["operation"].as_str().unwrap_or("");
        match action {
            "load" => {
                let load_action: Mutation = serde_json::from_value(intent["action"].clone())
                    .map_err(|_| anyhow::anyhow!("Managed load intent action is malformed"))?;
                let receipt_value = completion["details"]["receipt"].clone();
                let receipt: Receipt = serde_json::from_value(receipt_value).map_err(|_| {
                    anyhow::anyhow!("Managed load completion lacks a valid receipt")
                })?;
                validate_receipt(&receipt)?;
                if !matches!(load_action, Mutation::Load { ref model, context_length, offload_kv_cache_to_gpu } if model == &receipt.model_key && context_length == receipt.context_length && offload_kv_cache_to_gpu == receipt.offload_kv_cache_to_gpu)
                {
                    bail!("Managed load completion receipt differs from the approved action")
                }
                if receipt.load_task_id != task_id || receipt.endpoint != event_endpoint {
                    bail!("Managed load completion receipt does not match its task")
                }
                let saved = load_receipt_evidence(store, &receipt.receipt_id)?;
                if saved.state == "retired" {
                    resolution::validate_retired(store, &receipt.receipt_id)?;
                }
                let (expected_json, expected_sha) = receipt_hash(&receipt)?;
                let (saved_json, saved_sha) = receipt_hash(&saved.receipt)?;
                if saved.receipt.receipt_id != receipt.receipt_id
                    || expected_json != saved_json
                    || expected_sha != saved_sha
                    || saved.receipt.endpoint != event_endpoint
                {
                    bail!("Managed load completion has no matching durable receipt")
                }
            }
            "unload" => {
                let unload_action: Mutation = serde_json::from_value(intent["action"].clone())
                    .map_err(|_| anyhow::anyhow!("Managed release intent action is malformed"))?;
                let receipt_id = intent["receipt_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Managed release intent has no receipt"))?;
                if completion["details"]["receipt_id"].as_str() != Some(receipt_id) {
                    bail!("Managed release completion names another receipt")
                }
                let saved = load_receipt_evidence(store, receipt_id)?;
                if !matches!(unload_action, Mutation::Unload { ref instance_id } if instance_id == &saved.receipt.instance_id)
                {
                    bail!("Managed release action differs from its saved receipt")
                }
                if saved.state != "released"
                    || saved.release_task_id.as_deref() != Some(task_id.as_str())
                    || saved.receipt.endpoint != event_endpoint
                {
                    bail!("Managed release completion has no matching released receipt")
                }
            }
            _ => bail!("Managed residency task has an unsupported operation"),
        }
        if event_endpoint == endpoint && task_state != "completed" {
            bail!("Managed residency task is not durably complete")
        }
    }
    Ok(())
}

fn inventory_digest(models: &[Model]) -> Result<String> {
    super::planning::fingerprint(models)
}

fn create_managed_task(store: &Store, query: &str, payload: &Value) -> Result<String> {
    let raw = serde_json::to_string(payload)?;
    if raw.len() > MAX_EVENT_BYTES {
        bail!("Managed residency intent exceeds bounds")
    }
    let task = Uuid::new_v4().to_string();
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    crate::storage::admit(&tx)?;
    tx.execute(
        "INSERT INTO tasks(id,query,mode,status,answer,created,updated) VALUES (?1,?2,'fast','running','',unixepoch(),unixepoch())",
        params![task, query],
    )?;
    tx.execute(
        "INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'model_residency','human-cli',?2,unixepoch())",
        params![task, raw],
    )?;
    tx.commit()?;
    Ok(task)
}

fn completion_event(task: &str, state: &str, endpoint: &str, payload: Value) -> Result<String> {
    let value = json!({"state":state,"adapter":"lmstudio-native-v1","managed":true,"task_id":task,"endpoint":endpoint,"details":payload,"automatic_retry":false,"automatic_cleanup":false,"ownership_established":false});
    let raw = serde_json::to_string(&value)?;
    if raw.len() > MAX_EVENT_BYTES {
        bail!("Managed residency completion exceeds bounds")
    }
    Ok(raw)
}

fn receipt_rows_for_endpoint(store: &Store, endpoint: &str) -> Result<Vec<StoredReceipt>> {
    let db = store.lock()?;
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(receipt_id AS BLOB))<=64 THEN receipt_id END,CASE WHEN length(CAST(endpoint AS BLOB))<=512 THEN endpoint END,CASE WHEN length(CAST(instance_id AS BLOB))<=256 THEN instance_id END,CASE WHEN length(CAST(receipt_json AS BLOB))<=8192 THEN receipt_json END,CASE WHEN length(CAST(receipt_sha256 AS BLOB))=64 THEN receipt_sha256 END,CASE WHEN length(CAST(current_inventory_sha256 AS BLOB))=64 THEN current_inventory_sha256 END,CASE WHEN length(CAST(state AS BLOB))<=32 THEN state END,CASE WHEN length(CAST(load_task_id AS BLOB))<=64 THEN load_task_id END,CASE WHEN length(CAST(release_task_id AS BLOB))<=64 THEN release_task_id END FROM managed_residency_receipts WHERE endpoint=?1 ORDER BY created,receipt_id LIMIT 1025",
    )?;
    let rows = statement
        .query_map([endpoint], receipt_row_from_sql)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > MAX_RECEIPTS as usize {
        bail!("Managed receipt capacity exceeded")
    }
    rows.into_iter().map(decode_row).collect()
}

fn ensure_receipt_capacity(store: &Store) -> Result<()> {
    let db = store.lock()?;
    let count: i64 = db.query_row(
        "SELECT count(*) FROM managed_residency_receipts",
        [],
        |row| row.get(0),
    )?;
    if count >= MAX_RECEIPTS {
        bail!("Managed receipt ledger capacity is exhausted; no receipt was created")
    }
    Ok(())
}

fn finish_unknown(
    store: &Store,
    task: &str,
    endpoint: &str,
    receipt_id: Option<&str>,
    reason: &str,
) -> Result<()> {
    let safe_reason = match reason {
        "preflight" => "Managed operation preflight failed",
        _ => "Managed provider outcome is unknown; inspect manually before any further operation",
    };
    let raw = completion_event(
        task,
        "outcome_unknown",
        endpoint,
        json!({"reason":safe_reason,"receipt_id":receipt_id}),
    )?;
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(receipt_id) = receipt_id {
        let changed=tx.execute("UPDATE managed_residency_receipts SET state='quarantined',updated=unixepoch() WHERE receipt_id=?1 AND state='release_requested' AND release_task_id=?2",params![receipt_id,task])?;
        if changed != 1 {
            bail!("Managed release quarantine transition was not recorded")
        }
    }
    let inserted = tx.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT id,'model_residency','human-cli',?2,unixepoch() FROM tasks WHERE id=?1 AND status='running'",params![task,raw])?;
    let updated = tx.execute("UPDATE tasks SET status='interrupted',answer='Managed residency outcome unknown; operator inspection required',updated=unixepoch() WHERE id=?1 AND status='running'",[task])?;
    if inserted != 1 || updated != 1 {
        bail!("Managed residency unknown-outcome audit could not be committed")
    }
    tx.commit()?;
    Ok(())
}

fn finish_load(
    store: &Store,
    task: &str,
    receipt: &Receipt,
    before_digest: &str,
    report: Value,
) -> Result<()> {
    let (receipt_json, receipt_sha256) = receipt_hash(receipt)?;
    let raw = completion_event(task, "server_confirmed", &receipt.endpoint, report)?;
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let active: i64 = tx.query_row(
        "SELECT count(*) FROM managed_residency_receipts WHERE endpoint=?1 AND state='active'",
        [&receipt.endpoint],
        |r| r.get(0),
    )?;
    let stale: i64 = tx.query_row("SELECT count(*) FROM managed_residency_receipts WHERE endpoint=?1 AND state='active' AND current_inventory_sha256!=?2",params![receipt.endpoint,before_digest],|r|r.get(0))?;
    let total: i64 = tx.query_row("SELECT count(*) FROM managed_residency_receipts", [], |r| {
        r.get(0)
    })?;
    if active > MAX_RECEIPTS || stale != 0 || total >= MAX_RECEIPTS {
        bail!("Managed receipt cohort changed or capacity exhausted")
    }
    tx.execute("UPDATE managed_residency_receipts SET current_inventory_sha256=?2,updated=unixepoch() WHERE endpoint=?1 AND state='active' AND current_inventory_sha256=?3",params![receipt.endpoint,receipt.initial_inventory_sha256,before_digest])?;
    tx.execute("INSERT INTO managed_residency_receipts(receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,current_inventory_sha256,state,load_task_id,release_task_id,created,updated) VALUES (?1,?2,?3,?4,?5,?6,'active',?7,NULL,unixepoch(),unixepoch())",params![receipt.receipt_id,receipt.endpoint,receipt.instance_id,receipt_json,receipt_sha256,receipt.initial_inventory_sha256,task])?;
    let inserted = tx.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT id,'model_residency','human-cli',?2,unixepoch() FROM tasks WHERE id=?1 AND status='running'",params![task,raw])?;
    let updated = tx.execute("UPDATE tasks SET status='completed',answer='Managed residency load confirmed by exact inventory transition',updated=unixepoch() WHERE id=?1 AND status='running'",[task])?;
    if inserted != 1 || updated != 1 {
        bail!("Managed load completion anchor is missing or no longer running")
    }
    tx.commit()?;
    Ok(())
}

fn finish_release(
    store: &Store,
    task: &str,
    saved: &StoredReceipt,
    post_digest: &str,
    report: Value,
) -> Result<()> {
    let raw = completion_event(task, "server_confirmed", &saved.receipt.endpoint, report)?;
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let changed = tx.execute("UPDATE managed_residency_receipts SET state='released',updated=unixepoch() WHERE receipt_id=?1 AND state='release_requested' AND release_task_id=?2 AND current_inventory_sha256=?3",params![saved.receipt.receipt_id,task,saved.current_inventory_sha256])?;
    if changed != 1 {
        bail!("Managed release receipt changed during provider operation")
    }
    tx.execute("UPDATE managed_residency_receipts SET current_inventory_sha256=?2,updated=unixepoch() WHERE endpoint=?1 AND state='active' AND current_inventory_sha256=?3",params![saved.receipt.endpoint,post_digest,saved.current_inventory_sha256])?;
    let inserted = tx.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT id,'model_residency','human-cli',?2,unixepoch() FROM tasks WHERE id=?1 AND status='running'",params![task,raw])?;
    let updated = tx.execute("UPDATE tasks SET status='completed',answer='Managed residency release confirmed by exact inventory transition',updated=unixepoch() WHERE id=?1 AND status='running'",[task])?;
    if inserted != 1 || updated != 1 {
        bail!("Managed release completion anchor is missing or no longer running")
    }
    tx.commit()?;
    Ok(())
}

/// Conservatively invalidates active receipts before any legacy explicit
/// operator mutation. Such mutations retain their existing permission model.
pub(super) fn quarantine_before_manual_mutation(
    store: &Store,
    task: &str,
    endpoint: &str,
) -> Result<()> {
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let count = tx.execute("UPDATE managed_residency_receipts SET state='quarantined',updated=unixepoch() WHERE endpoint=?1 AND state='active'",[endpoint])?;
    if count > 0 {
        let raw = serde_json::to_string(
            &json!({"format_version":1,"state":"managed_receipts_quarantined","endpoint":endpoint,"count":count,"reason":"manual provider mutation changes the shared inventory baseline","permissions_granted":false}),
        )?;
        if raw.len() > MAX_EVENT_BYTES {
            bail!("Managed receipt invalidation audit exceeds bounds")
        }
        let inserted = tx.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT id,'managed_residency_audit','human-cli',?2,unixepoch() FROM tasks WHERE id=?1 AND status='running'",params![task,raw])?;
        if inserted != 1 {
            bail!("Managed receipt invalidation audit anchor is missing")
        }
    }
    tx.commit()?;
    Ok(())
}

/// Explicit human-approved native load. The provider response alone never
/// creates ownership: a fresh exact full-inventory transition is required.
pub async fn load(
    store: &Store,
    base: &str,
    authenticate: bool,
    model_key: &str,
    context_length: u32,
    human_approve: bool,
) -> Result<Value> {
    load_with_gpu(
        store,
        base,
        authenticate,
        model_key,
        context_length,
        None,
        human_approve,
    )
    .await
}

/// Explicit operator-approved load with optional provider-declared KV-cache placement.
/// Omission retains the legacy request and receipt format.
pub async fn load_with_gpu(
    store: &Store,
    base: &str,
    authenticate: bool,
    model_key: &str,
    context_length: u32,
    offload_kv_cache_to_gpu: Option<bool>,
    human_approve: bool,
) -> Result<Value> {
    load_owned_with_gpu(
        store,
        base,
        authenticate,
        model_key,
        context_length,
        offload_kv_cache_to_gpu,
        (human_approve, None),
    )
    .await
}

pub(crate) async fn load_owned_with_gpu(
    store: &Store,
    base: &str,
    authenticate: bool,
    model_key: &str,
    context_length: u32,
    offload_kv_cache_to_gpu: Option<bool>,
    approval: (bool, Option<&super::lifecycle::Ownership>),
) -> Result<Value> {
    let (human_approve, ownership) = approval;
    security::authorize("human-cli", "model.residency.change")?;
    if !human_approve {
        bail!("Managed model loads require explicit operator approval")
    }
    let native_endpoint = endpoint(base)?;
    if !identity(model_key) || !(1..=32768).contains(&context_length) {
        bail!("Invalid managed load identity or context ceiling")
    }
    let _leases = match ownership {
        Some(owner) => {
            owner.validate(store)?;
            None
        }
        None => Some(mutation_leases(store)?),
    };
    pending_endpoint_operation(store, native_endpoint.as_str())?;
    let started = Instant::now();
    let before_value = status(base, authenticate).await?;
    let before = parse(&serde_json::to_vec(&before_value)?)?;
    let before_digest = inventory_digest(&before)?;
    active_cohort(store, native_endpoint.as_str(), &before, &before_digest)?;
    ensure_receipt_capacity(store)?;
    let selected = before
        .iter()
        .find(|item| item.key == model_key)
        .ok_or_else(|| {
            anyhow::anyhow!("Managed load requires an installed model key from native inventory")
        })?;
    if !selected.loaded_instances.is_empty() {
        bail!("Managed load requires a model with no loaded instances; reuse requires a separate reviewed action")
    }
    if offload_kv_cache_to_gpu.is_some() && !matches!(selected.kind.as_str(), "llm" | "decision") {
        bail!("Explicit KV-cache GPU placement is supported only for LLM and decision models")
    }
    let operation = "load";
    let action = Mutation::Load {
        model: model_key.to_owned(),
        context_length,
        offload_kv_cache_to_gpu,
    };
    let mut body = json!({"model":model_key,"context_length":context_length});
    if let Some(value) = offload_kv_cache_to_gpu {
        body["offload_kv_cache_to_gpu"] = json!(value);
        body["echo_load_config"] = json!(true);
    }
    let task = create_managed_task(
        store,
        "Operator-approved managed native model load",
        &json!({"state":"requested","adapter":"lmstudio-native-v1","managed":true,"endpoint":native_endpoint.as_str(),"action":action,"external_outcome":"unknown","permissions_granted":false}),
    )?;
    let result = async {
        let response = request(base,authenticate,Some(operation),Some(body),Duration::from_secs(60)).await?;
        let instance_id = response["instance_id"].as_str().filter(|id| identity(id)).ok_or_else(|| anyhow::anyhow!("Invalid managed load response"))?;
        if response["status"] != "loaded" || response["type"].as_str() != Some(selected.kind.as_str()) || !response["load_time_seconds"].as_f64().is_some_and(|n| n.is_finite() && n>=0.0) || before.iter().any(|item|item.loaded_instances.iter().any(|i|i.id==instance_id)) {
            bail!("Managed load response did not identify a new exact instance")
        }
        if let Some(expected) = offload_kv_cache_to_gpu {
            if !matches!(selected.kind.as_str(), "llm" | "decision")
                || response["load_config"]["offload_kv_cache_to_gpu"].as_bool() != Some(expected)
            {
                bail!("Managed load response did not echo the requested KV-cache GPU placement")
            }
        }
        let after_value = status(base,authenticate).await?;
        let after = parse(&serde_json::to_vec(&after_value)?)?;
        super::planning::validate_transition(&before,&after,&action,instance_id)?;
        let model = after.iter().find(|item|item.key==model_key).ok_or_else(|| anyhow::anyhow!("Managed loaded model disappeared from inventory"))?;
        let instance = model.loaded_instances.iter().find(|i|i.id==instance_id).ok_or_else(|| anyhow::anyhow!("Managed loaded instance is absent from fresh inventory"))?;
        if model.kind != selected.kind || instance.config.context_length != u64::from(context_length)
            || offload_kv_cache_to_gpu.is_some_and(|expected| instance.config.offload_kv_cache_to_gpu != Some(expected)) {
            bail!("Managed loaded instance identity differs from the approved request")
        }
        let after_digest = inventory_digest(&after)?;
        let receipt = Receipt { format_version:if offload_kv_cache_to_gpu.is_some() { 2 } else { 1 }, receipt_id:Uuid::new_v4().to_string(), adapter:"lmstudio-native-v1".into(), endpoint:native_endpoint.as_str().to_owned(), model_key:model_key.to_owned(), model_type:model.kind.clone(), context_length, offload_kv_cache_to_gpu, instance_id:instance_id.to_owned(), load_task_id:task.clone(), initial_inventory_sha256:after_digest.clone() };
        let report = json!({"receipt":receipt,"action":action,"server_confirmed":true,"before_inventory_sha256":before_digest,"after_inventory_sha256":after_digest,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"provider_load_seconds":response.get("load_time_seconds"),"weights_attested":false,"generation_tested":false,"ownership_established":false,"physical_residency_attested":false,"automatic_cleanup":false});
        finish_load(store,&task,&receipt,&before_digest,report.clone())?;
        Ok::<_,anyhow::Error>(report)
    }.await;
    match result {
        Ok(report) => Ok(report),
        Err(_) => {
            finish_unknown(store, &task, native_endpoint.as_str(), None, "provider")?;
            bail!("Managed load outcome is unknown for task {task}; inspect inventory before any further operation")
        }
    }
}

/// Explicit human-approved release by receipt UUID. Inventory observation and
/// provider ID matches never create or recover a missing receipt.
pub async fn release(
    store: &Store,
    base: &str,
    authenticate: bool,
    receipt_id: &str,
    human_approve: bool,
) -> Result<Value> {
    release_owned(store, base, authenticate, receipt_id, (human_approve, None)).await
}

pub(crate) async fn release_owned(
    store: &Store,
    base: &str,
    authenticate: bool,
    receipt_id: &str,
    approval: (bool, Option<&super::lifecycle::Ownership>),
) -> Result<Value> {
    let (human_approve, ownership) = approval;
    security::authorize("human-cli", "model.residency.change")?;
    if !human_approve {
        bail!("Managed model releases require explicit operator approval")
    }
    Uuid::parse_str(receipt_id)?;
    let native_endpoint = endpoint(base)?;
    let saved = load_receipt(store, receipt_id)?;
    if saved.state != "active" || saved.receipt.endpoint != native_endpoint.as_str() {
        bail!("Managed receipt is not active at this native endpoint")
    }
    let _leases = match ownership {
        Some(owner) => {
            owner.validate(store)?;
            None
        }
        None => Some(mutation_leases(store)?),
    };
    pending_endpoint_operation(store, native_endpoint.as_str())?;
    let started = Instant::now();
    let before_value = status(base, authenticate).await?;
    let before = parse(&serde_json::to_vec(&before_value)?)?;
    let before_digest = inventory_digest(&before)?;
    if before_digest != saved.current_inventory_sha256
        || model_instance(&before, &saved.receipt).is_none()
    {
        bail!("Current native inventory differs from the managed receipt; release refused")
    }
    let cohort = active_cohort(store, native_endpoint.as_str(), &before, &before_digest)?;
    if !cohort.iter().any(|r| r.receipt.receipt_id == receipt_id) {
        bail!("Managed receipt is absent from the active endpoint cohort")
    }
    let task = Uuid::new_v4().to_string();
    // Receipt transition, task creation and exact release intent are one durable
    // transaction before the provider can receive an unload request.
    let intent = json!({"state":"requested","adapter":"lmstudio-native-v1","managed":true,"endpoint":native_endpoint.as_str(),"receipt_id":receipt_id,"action":{"operation":"unload","instance_id":saved.receipt.instance_id},"external_outcome":"unknown","permissions_granted":false});
    let raw = serde_json::to_string(&intent)?;
    if raw.len() > MAX_EVENT_BYTES {
        bail!("Managed release intent exceeds bounds")
    }
    {
        let mut db = store.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        crate::storage::admit(&tx)?;
        tx.execute("INSERT INTO tasks(id,query,mode,status,answer,created,updated) VALUES (?1,'Operator-approved managed native model release','fast','running','',unixepoch(),unixepoch())",[&task])?;
        let changed=tx.execute("UPDATE managed_residency_receipts SET state='release_requested',release_task_id=?2,updated=unixepoch() WHERE receipt_id=?1 AND endpoint=?3 AND state='active' AND current_inventory_sha256=?4",params![receipt_id,task,native_endpoint.as_str(),before_digest])?;
        let inserted=tx.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT id,'model_residency','human-cli',?2,unixepoch() FROM tasks WHERE id=?1 AND status='running'",params![task,raw])?;
        if changed != 1 || inserted != 1 {
            bail!("Managed release intent could not be recorded")
        }
        tx.commit()?;
    }
    let result=async {
        let response=request(base,authenticate,Some("unload"),Some(json!({"instance_id":saved.receipt.instance_id})),Duration::from_secs(60)).await?;
        if response["instance_id"].as_str()!=Some(saved.receipt.instance_id.as_str()) { bail!("Managed release response named another instance") }
        let after_value=status(base,authenticate).await?;
        let after=parse(&serde_json::to_vec(&after_value)?)?;
        let action=Mutation::Unload{instance_id:saved.receipt.instance_id.clone()};
        super::planning::validate_transition(&before,&after,&action,&saved.receipt.instance_id)?;
        if model_instance(&after,&saved.receipt).is_some() { bail!("Managed instance remains loaded after release") }
        let after_digest=inventory_digest(&after)?;
        for member in &cohort {
            if member.receipt.receipt_id != receipt_id
                && model_instance(&after, &member.receipt).is_none()
            {
                bail!("Another managed instance or its KV-cache placement changed during release")
            }
        }
        let report=json!({"receipt_id":receipt_id,"instance_id":saved.receipt.instance_id,"server_confirmed":true,"before_inventory_sha256":before_digest,"after_inventory_sha256":after_digest,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"weights_attested":false,"generation_tested":false,"ownership_established":false,"physical_residency_attested":false,"automatic_retry":false,"automatic_cleanup":false});
        finish_release(store,&task,&saved,&after_digest,report.clone())?;
        Ok::<_,anyhow::Error>(report)
    }.await;
    match result {
        Ok(report) => Ok(report),
        Err(_) => {
            finish_unknown(
                store,
                &task,
                native_endpoint.as_str(),
                Some(receipt_id),
                "provider",
            )?;
            bail!("Managed release outcome is unknown for task {task}; receipt is quarantined and cannot be retried")
        }
    }
}

/// Read the immutable receipt and current state for an operator. This performs
/// no provider request and confers no retry or mutation authority.
pub fn inspect(store: &Store, receipt_id: &str) -> Result<Value> {
    security::authorize("human-cli", "model.residency.inspect")?;
    let saved = load_receipt_evidence(store, receipt_id)?;
    if saved.state == "retired" {
        resolution::validate_retired(store, receipt_id)?;
    }
    Ok(
        json!({"receipt":saved.receipt,"state":saved.state,"current_inventory_sha256":saved.current_inventory_sha256,"release_task_id":saved.release_task_id,"ownership_established":false,"automatic_retry":false,"model_mutation_performed":false}),
    )
}

/// List local receipts for the canonical native endpoint without contacting the
/// provider. This cannot adopt instances or clear unresolved operations.
pub fn list(store: &Store, base: &str) -> Result<Value> {
    security::authorize("human-cli", "model.residency.inspect")?;
    let native_endpoint = endpoint(base)?;
    let receipts = receipt_rows_for_endpoint(store, native_endpoint.as_str())?;
    let mut pending = false;
    let mut blocked = receipts
        .iter()
        .any(|item| item.state != "active" && item.state != "released" && item.state != "retired");
    match pending_endpoint_operation(store, native_endpoint.as_str()) {
        Ok(()) => {}
        Err(_) => {
            pending = true;
            blocked = true;
        }
    }
    let items = receipts
        .into_iter()
        .map(|item| {
            json!({
                "receipt": item.receipt,
                "state": item.state,
                "current_inventory_sha256": item.current_inventory_sha256,
                "release_task_id": item.release_task_id,
                "ownership_established": false,
                "automatic_retry": false
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "format_version": 1,
        "adapter": "lmstudio-native-v1",
        "endpoint": native_endpoint.as_str(),
        "receipts": items,
        "unresolved_provider_operation": pending,
        "managed_lifecycle_blocked": blocked,
        "server_contacted": false,
        "model_mutation_performed": false,
        "ownership_established": false
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_history_scan_is_bounded_and_clears_its_handler() -> Result<()> {
        let store = Store::in_memory()?;
        let result = pending_endpoint_operation_with_limits(
            &store,
            "http://127.0.0.1:1234/api/v1",
            Duration::ZERO,
            1,
        );
        assert!(
            result.is_err(),
            "an immediately interrupted scan must fail closed"
        );

        // This query executes enough VM instructions to hit any still-installed
        // progress handler. It succeeds only after the scan's RAII reset ran.
        let db = store.lock()?;
        let rows: i64 = db.query_row(
            "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<5000) SELECT max(x) FROM n",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(rows, 5000);
        Ok(())
    }

    #[test]
    fn malformed_or_conflicting_managed_markers_never_downgrade_receipt_validation() -> Result<()> {
        for marker in [json!("true"), json!(1), Value::Null, json!(false)] {
            let store = Store::in_memory()?;
            let task = store.create_task("Managed history marker regression", "fast")?;
            let endpoint = "http://127.0.0.1:1234/api/v1/models";
            store.event(&task, "model_residency", "human-cli", json!({
                "state":"requested","adapter":"lmstudio-native-v1","endpoint":endpoint,
                "managed":marker,"action":{"operation":"load","model":"fixture","context_length":1},
                "external_outcome":"unknown"
            }))?;
            store.event(
                &task,
                "model_residency",
                "human-cli",
                json!({
                    "state":"server_confirmed","adapter":"lmstudio-native-v1","endpoint":endpoint,
                    "managed":true
                }),
            )?;
            store.finish_task(&task, "completed", "fixture")?;
            assert!(pending_endpoint_operation(&store, endpoint).is_err());
        }
        Ok(())
    }

    #[test]
    fn pending_history_scan_uses_bounded_event_index() -> Result<()> {
        let store = Store::in_memory()?;
        let db = store.lock()?;
        let plan = format!("EXPLAIN QUERY PLAN {RESIDENCY_HISTORY_QUERY}");
        let mut statement = db.prepare(&plan)?;
        let details = statement
            .query_map([], |row| row.get::<_, String>(3))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("managed_residency_event_scan")),
            "query plan should use the bounded event index: {details:?}"
        );
        Ok(())
    }
}

#[cfg(test)]
mod archived_capacity_history_tests {
    use super::*;

    #[test]
    fn history_beyond_old_receipt_limit_is_validated_without_hiding_later_unknowns() -> Result<()> {
        let store = Store::in_memory()?;
        let endpoint = "http://127.0.0.1:1234/api/v1/models";
        {
            let mut db = store.lock()?;
            let tx = db.transaction()?;
            for _ in 0..2050 {
                let task = Uuid::new_v4().to_string();
                tx.execute("INSERT INTO tasks(id,query,mode,status,created,updated) VALUES(?1,'legacy operator mutation','fast','completed',1,1)", [&task])?;
                for state in ["requested", "server_confirmed"] {
                    let payload =
                        json!({"state":state,"endpoint":endpoint,"managed":false}).to_string();
                    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'model_residency','human-cli',?2,1)", params![task,payload])?;
                }
            }
            tx.commit()?;
        }
        pending_endpoint_operation(&store, endpoint)?;
        let task = store.create_task("unresolved after old history ceiling", "fast")?;
        store.event(
            &task,
            "model_residency",
            "human-cli",
            json!({"state":"requested","endpoint":endpoint,"managed":false}),
        )?;
        assert!(pending_endpoint_operation(&store, endpoint).is_err());
        Ok(())
    }
}

/// Validate one pinned active receipt against the complete freshly observed
/// inventory. This returns metadata only and never refreshes release authority.
pub(crate) fn validate_task_binding(
    store: &Store,
    receipt_id: &str,
    base: &str,
    inventory: &[Model],
) -> Result<Receipt> {
    let native = endpoint(base)?;
    let saved = load_receipt(store, receipt_id)?;
    let digest = inventory_digest(inventory)?;
    if saved.state != "active"
        || saved.receipt.endpoint != native.as_str()
        || saved.current_inventory_sha256 != digest
        || model_instance(inventory, &saved.receipt).is_none()
    {
        bail!("Task native-instance binding has drifted or lost release eligibility")
    }
    active_cohort(store, native.as_str(), inventory, &digest)?;
    Ok(saved.receipt)
}
