//! Reviewed hot-ledger compaction; archived evidence never grants release authority.
use super::*;
use rusqlite::{params, OptionalExtension};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format_version: u32,
    receipt_id: String,
    endpoint: String,
    instance_id: String,
    receipt_json: String,
    receipt_sha256: String,
    current_inventory_sha256: String,
    state: String,
    load_task_id: String,
    release_task_id: Option<String>,
    created: f64,
    updated: f64,
    archived_at: f64,
    retirement_task_id: String,
}
impl Snapshot {
    fn decoded(&self) -> Result<StoredReceipt> {
        if self.format_version != 1
            || !matches!(self.state.as_str(), "released" | "retired")
            || [self.created, self.updated, self.archived_at]
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0)
            || Uuid::parse_str(&self.retirement_task_id).is_err()
        {
            bail!("Invalid archived receipt metadata")
        }
        decode_row(ReceiptRow {
            receipt_id: self.receipt_id.clone(),
            endpoint: self.endpoint.clone(),
            instance_id: self.instance_id.clone(),
            receipt_json: self.receipt_json.clone(),
            receipt_sha256: self.receipt_sha256.clone(),
            current_inventory_sha256: self.current_inventory_sha256.clone(),
            state: self.state.clone(),
            load_task_id: self.load_task_id.clone(),
            release_task_id: self.release_task_id.clone(),
        })
    }
    fn digest(&self) -> Result<String> {
        Ok(sha256(&serde_json::to_vec(self)?))
    }
    fn event(&self, state: &str, digest: &str) -> Value {
        json!({"format_version":1,"state":state,"receipt_id":self.receipt_id,
            "receipt_state":self.state,"endpoint":self.endpoint,"archive_sha256":digest,
            "retirement_task_id":self.retirement_task_id,"provider_contacted":false,
            "model_mutation_performed":false,"permissions_granted":false})
    }
}

pub(super) fn load_archive(store: &Store, id: &str) -> Result<Option<StoredReceipt>> {
    Uuid::parse_str(id)?;
    let db = store.lock()?;
    let value = db.query_row("SELECT receipt_id,endpoint,instance_id,CASE WHEN length(CAST(receipt_json AS BLOB))<=8192 THEN receipt_json END,receipt_sha256,current_inventory_sha256,state,load_task_id,release_task_id,created,updated,archived_at,retirement_task_id,archive_sha256 FROM managed_residency_receipt_archive WHERE receipt_id=?1",[id],|r| {
        Ok((Snapshot { format_version:1,receipt_id:r.get(0)?,endpoint:r.get(1)?,instance_id:r.get(2)?,receipt_json:r.get(3)?,receipt_sha256:r.get(4)?,current_inventory_sha256:r.get(5)?,state:r.get(6)?,load_task_id:r.get(7)?,release_task_id:r.get(8)?,created:r.get(9)?,updated:r.get(10)?,archived_at:r.get(11)?,retirement_task_id:r.get(12)? },r.get::<_,String>(13)?))
    }).optional()?;
    let Some((snapshot, digest)) = value else {
        return Ok(None);
    };
    let decoded = snapshot.decoded()?;
    if snapshot.digest()? != digest {
        bail!("Archived receipt digest mismatch")
    }
    let state: String = db.query_row(
        "SELECT status FROM tasks WHERE id=?1",
        [&snapshot.retirement_task_id],
        |r| r.get(0),
    )?;
    if state != "completed" {
        bail!("Archived receipt has no completed retirement task")
    }
    let mut stmt=db.prepare("SELECT source,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind='managed_residency_audit' ORDER BY id LIMIT 3")?;
    let rows = stmt
        .query_map([&snapshot.retirement_task_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 2 {
        bail!("Archived receipt audit is missing or duplicated")
    }
    for ((source, raw), label) in rows
        .iter()
        .zip(["receipt_archive_requested", "receipt_archived"])
    {
        if source != "human-cli"
            || crate::strict_json::value(raw.as_bytes(), 4096)? != snapshot.event(label, &digest)
        {
            bail!("Archived receipt audit differs from its immutable snapshot")
        }
    }
    Ok(Some(decoded))
}

/// Archive exactly one terminal receipt, without contacting the provider.
pub fn archive(store: &Store, id: &str, human_approve: bool) -> Result<Value> {
    security::authorize("human-cli", "model.residency.change")?;
    if !human_approve {
        bail!("Managed receipt archival requires explicit operator approval")
    }
    Uuid::parse_str(id)?;
    let _leases = mutation_leases(store)?;
    let saved = load_receipt(store, id)?;
    if !matches!(saved.state.as_str(), "released" | "retired") {
        bail!("Only terminal managed receipts can be archived")
    }
    pending_endpoint_operation(store, &saved.receipt.endpoint)?;
    if saved.state == "retired" {
        resolution::validate_retired(store, id)?;
    }
    let task = Uuid::new_v4().to_string();
    let archived_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs_f64();
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    crate::storage::admit(&tx)?;
    crate::store::admit_managed_residency_archive(&tx)?;
    let (row,created,updated)=tx.query_row("SELECT receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,current_inventory_sha256,state,load_task_id,release_task_id,created,updated FROM managed_residency_receipts WHERE receipt_id=?1",[id],|r|Ok((receipt_row_from_sql(r)?,r.get::<_,f64>(9)?,r.get::<_,f64>(10)?)))?;
    let decoded = decode_row(row)?;
    if serde_json::to_value(&decoded.receipt)? != serde_json::to_value(&saved.receipt)?
        || decoded.state != saved.state
        || decoded.current_inventory_sha256 != saved.current_inventory_sha256
        || decoded.release_task_id != saved.release_task_id
    {
        bail!("Managed receipt changed during archive admission")
    }
    let (receipt_json, receipt_sha256) = receipt_hash(&decoded.receipt)?;
    let snapshot = Snapshot {
        format_version: 1,
        receipt_id: id.into(),
        endpoint: decoded.receipt.endpoint,
        instance_id: decoded.receipt.instance_id,
        receipt_json,
        receipt_sha256,
        current_inventory_sha256: decoded.current_inventory_sha256,
        state: decoded.state,
        load_task_id: decoded.receipt.load_task_id,
        release_task_id: decoded.release_task_id,
        created,
        updated,
        archived_at,
        retirement_task_id: task.clone(),
    };
    snapshot.decoded()?;
    let digest = snapshot.digest()?;
    tx.execute("INSERT INTO tasks(id,query,mode,status,answer,created,updated) VALUES(?1,'Operator-approved managed receipt archival','fast','completed','Archived local evidence; no provider mutation',?2,?2)",params![task,archived_at])?;
    for label in ["receipt_archive_requested", "receipt_archived"] {
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'managed_residency_audit','human-cli',?2,?3)",params![task,snapshot.event(label,&digest).to_string(),archived_at])?;
    }
    let deleted=tx.execute("DELETE FROM managed_residency_receipts WHERE receipt_id=?1 AND state IN ('released','retired')",[id])?;
    if deleted != 1 {
        bail!("Managed receipt could not be archived")
    }
    tx.execute("INSERT INTO managed_residency_receipt_archive(receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,current_inventory_sha256,state,load_task_id,release_task_id,created,updated,archived_at,retirement_task_id,archive_sha256) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",params![snapshot.receipt_id,snapshot.endpoint,snapshot.instance_id,snapshot.receipt_json,snapshot.receipt_sha256,snapshot.current_inventory_sha256,snapshot.state,snapshot.load_task_id,snapshot.release_task_id,snapshot.created,snapshot.updated,snapshot.archived_at,snapshot.retirement_task_id,digest])?;
    tx.commit()?;
    Ok(
        json!({"receipt_id":id,"archived":true,"archive_sha256":digest,"task_id":task,"provider_contacted":false,"model_mutation_performed":false,"ownership_established":false}),
    )
}
