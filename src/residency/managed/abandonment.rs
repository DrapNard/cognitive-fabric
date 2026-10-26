//! Explicit retirement of confirmed receipts after an externally emptied provider.
//! This discards local release authority; it never adopts, loads or unloads weights.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    format_version: u32,
    receipt_id: String,
    receipt_sha256: String,
    endpoint: String,
    load_task_id: String,
    prior_inventory_sha256: String,
    observed_empty_inventory_sha256: String,
    operator_approved: bool,
    provider_stop_acknowledged: bool,
}
impl Proof {
    fn event(&self, state: &str) -> Result<Value> {
        Ok(
            json!({"state":state,"proof":self,"proof_sha256":sha256(&serde_json::to_vec(self)?),
            "provider_contacted":true,"model_mutation_performed":false,"release_authority_retired":true,
            "provider_stop_proven":false,"original_load_outcome":"server_confirmed"}),
        )
    }
}

/// The original load task anchors bounded evidence and remains retention protected.
pub(super) fn validate_retired(store: &Store, id: &str) -> Result<bool> {
    let saved = load_receipt_evidence(store, id)?;
    let rows = {
        let db = store.lock()?;
        let mut stmt = db.prepare("SELECT source,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind='managed_residency_abandonment' ORDER BY id LIMIT 3")?;
        let rows = stmt
            .query_map([&saved.receipt.load_task_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if rows.is_empty() {
        return Ok(false);
    }
    if rows.len() != 2 || saved.state != "retired" || saved.release_task_id.is_some() {
        bail!("Invalid absent-instance retirement history")
    }
    let first = crate::strict_json::value(rows[0].1.as_bytes(), 4096)?;
    let proof: Proof = serde_json::from_value(first["proof"].clone())?;
    if proof.format_version != 1
        || !proof.operator_approved
        || !proof.provider_stop_acknowledged
        || proof.receipt_id != id
        || proof.receipt_sha256 != receipt_hash(&saved.receipt)?.1
        || proof.endpoint != saved.receipt.endpoint
        || proof.load_task_id != saved.receipt.load_task_id
        || proof.prior_inventory_sha256 != saved.current_inventory_sha256
        || !valid_digest(&proof.observed_empty_inventory_sha256)
    {
        bail!("Absent-instance retirement proof differs from its receipt")
    }
    let db = store.lock()?;
    let completed: bool = db.query_row(
        "SELECT status='completed' FROM tasks WHERE id=?1",
        [&proof.load_task_id],
        |r| r.get(0),
    )?;
    if !completed {
        bail!("Absent-instance retirement lost original confirmed task")
    }
    for ((source, raw), state) in rows
        .iter()
        .zip(["abandon_requested", "authority_abandoned"])
    {
        if source != "human-cli"
            || crate::strict_json::value(raw.as_bytes(), 4096)? != proof.event(state)?
        {
            bail!("Absent-instance retirement audit changed")
        }
    }
    Ok(true)
}

/// Requires a fresh wholly empty provider and explicit operator stop testimony.
pub async fn abandon(
    store: &Store,
    base: &str,
    authenticate: bool,
    id: &str,
    human_approve: bool,
    provider_stop_acknowledged: bool,
) -> Result<Value> {
    security::authorize("human-cli", "model.residency.change")?;
    if !human_approve || !provider_stop_acknowledged {
        bail!("Receipt abandonment requires operator approval and provider-stop acknowledgement")
    }
    Uuid::parse_str(id)?;
    let native_endpoint = endpoint(base)?;
    let _leases = mutation_leases(store)?;
    pending_endpoint_operation(store, native_endpoint.as_str())?;
    let saved = load_receipt(store, id)?;
    if saved.state != "active"
        || saved.release_task_id.is_some()
        || saved.receipt.endpoint != native_endpoint.as_str()
    {
        bail!("Only an active confirmed receipt at the selected endpoint can be abandoned")
    }
    let observed = status(base, authenticate).await?;
    let models = parse(&serde_json::to_vec(&observed)?)?;
    if models
        .iter()
        .any(|model| !model.loaded_instances.is_empty())
    {
        bail!("Receipt abandonment requires a provider with no loaded instances")
    }
    let proof = Proof {
        format_version: 1,
        receipt_id: id.into(),
        receipt_sha256: receipt_hash(&saved.receipt)?.1,
        endpoint: native_endpoint.to_string(),
        load_task_id: saved.receipt.load_task_id.clone(),
        prior_inventory_sha256: saved.current_inventory_sha256.clone(),
        observed_empty_inventory_sha256: inventory_digest(&models)?,
        operator_approved: true,
        provider_stop_acknowledged: true,
    };
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    crate::storage::admit(&tx)?;
    let completed: bool = tx.query_row(
        "SELECT status='completed' FROM tasks WHERE id=?1",
        [&proof.load_task_id],
        |r| r.get(0),
    )?;
    let existing:bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='managed_residency_abandonment')",[&proof.load_task_id],|r|r.get(0))?;
    if !completed || existing {
        bail!("Receipt abandonment history is not eligible")
    }
    let changed = tx.execute("UPDATE managed_residency_receipts SET state='retired',updated=unixepoch() WHERE receipt_id=?1 AND state='active' AND receipt_sha256=?2 AND current_inventory_sha256=?3 AND release_task_id IS NULL",params![id,proof.receipt_sha256,proof.prior_inventory_sha256])?;
    if changed != 1 {
        bail!("Receipt changed before abandonment")
    }
    for state in ["abandon_requested", "authority_abandoned"] {
        let raw = proof.event(state)?.to_string();
        if raw.len() > 4096 {
            bail!("Receipt abandonment audit exceeds limits")
        }
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'managed_residency_abandonment','human-cli',?2,unixepoch())",params![proof.load_task_id,raw])?;
    }
    tx.commit()?;
    Ok(
        json!({"receipt_id":id,"release_authority_retired":true,"model_mutation_performed":false,
        "provider_stop_acknowledged":true,"provider_stop_proven":false,"original_load_outcome":"server_confirmed",
        "automatic_retry":false,"ownership_established":false}),
    )
}
