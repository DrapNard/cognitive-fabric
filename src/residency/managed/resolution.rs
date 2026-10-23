//! Explicit operator resolution of unknown effects; never retries provider mutations.
use super::*;
use rusqlite::{params, OptionalExtension};

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub human_approve: bool,
    pub provider_stop_acknowledged: bool,
    pub retire_stale_cohort: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    receipt_id: String,
    receipt_sha256: String,
    prior_state: String,
    prior_inventory_sha256: String,
    load_task_id: String,
    release_task_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format_version: u32,
    resolution_id: String,
    resolver_task_id: String,
    original_task_id: String,
    endpoint: String,
    operation: String,
    query_evidence_sha256: String,
    intent_sha256: String,
    inventory_sha256: String,
    model_key: Option<String>,
    model_type: Option<String>,
    context_length: Option<u32>,
    receipt_id: Option<String>,
    instance_id: Option<String>,
    operator_approved: bool,
    provider_stop_acknowledged: bool,
    retire_stale_cohort: bool,
    retired_count: usize,
    members_sha256: String,
    original_outcome: String,
    provider_stop_proven: bool,
}
struct Original {
    intent: Value,
    action: Mutation,
    evidence: String,
    endpoint: String,
    state: String,
}
fn original_db(db: &rusqlite::Connection, task: &str) -> Result<Original> {
    Uuid::parse_str(task)?;
    let (query,state):(String,String)=db.query_row("SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END,status FROM tasks WHERE id=?1",[task],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if !matches!(state.as_str(), "running" | "interrupted") {
        bail!("Only unresolved local operations can be resolved")
    }
    let mut stmt=db.prepare("SELECT source,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind='model_residency' ORDER BY id LIMIT 3")?;
    let rows = stmt
        .query_map([task], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() || rows.len() > 2 {
        bail!("Ambiguous original residency history")
    }
    let mut values = Vec::new();
    for (source, raw) in rows {
        let value = crate::strict_json::value(raw.as_bytes(), 4096)?;
        if source != "human-cli"
            || value["managed"] != true
            || value["adapter"] != "lmstudio-native-v1"
        {
            bail!("Resolution requires original managed operator evidence")
        }
        values.push(value);
    }
    let intent = values[0].clone();
    if intent["state"] != "requested"
        || intent["external_outcome"] != "unknown"
        || intent["permissions_granted"] != false
    {
        bail!("Invalid original managed intent")
    }
    let endpoint = intent["endpoint"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing original endpoint"))?
        .to_owned();
    if canonical_inventory_endpoint(&endpoint)? != endpoint
        || values.iter().any(|v| v["endpoint"] != endpoint)
    {
        bail!("Original endpoint history changed")
    }
    if values.len() == 2 && values[1]["state"] != "outcome_unknown" {
        bail!("Confirmed operations cannot be resolved as unknown")
    }
    let action: Mutation = serde_json::from_value(intent["action"].clone())?;
    super::super::validate_action(&action)?;
    let evidence = sha256(&serde_json::to_vec(
        &json!({"domain":"managed-resolution-original/v1","task":task,"query":query,"events":values}),
    )?);
    Ok(Original {
        intent,
        action,
        evidence,
        endpoint,
        state,
    })
}
fn members_digest(members: &[Member]) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(members)?))
}
fn event(record: &Record, state: &str, digest: &str) -> Value {
    json!({"format_version":1,"state":state,"resolution_id":record.resolution_id,
        "resolver_task_id":record.resolver_task_id,"original_task_id":record.original_task_id,
        "payload_sha256":digest,"provider_contacted":true,"model_mutation_performed":false,
        "original_outcome":"unknown","provider_stop_proven":false,"permissions_granted":false})
}

/// Read and fully validate durable resolution evidence without contacting a provider.
type ResolutionRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

fn validated(store: &Store, task: &str) -> Result<Option<Record>> {
    let db = store.lock()?;
    let stored:Option<ResolutionRow>=db.query_row(
        "SELECT CASE WHEN length(CAST(payload_json AS BLOB))<=4096 THEN payload_json END,payload_sha256,resolver_task_id,endpoint,operation,query_evidence_sha256,intent_sha256,inventory_sha256,resolution_id FROM managed_residency_resolutions WHERE original_task_id=?1",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).optional()?;
    let Some((raw, digest, resolver, endpoint, operation, evidence, intent_hash, inventory, id)) =
        stored
    else {
        return Ok(None);
    };
    let record: Record = serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 4096)?)?;
    let columns_match: bool = db.query_row(
        "SELECT model_key IS ?2 AND model_type IS ?3 AND context_length IS ?4 AND receipt_id IS ?5 AND instance_id IS ?6 AND operator_approved=1 AND provider_stop_acknowledged=1 FROM managed_residency_resolutions WHERE original_task_id=?1",
        params![task, record.model_key, record.model_type, record.context_length, record.receipt_id, record.instance_id],
        |row| row.get(0),
    )?;
    if !columns_match {
        bail!("Resolution identity columns differ from their immutable payload")
    }
    let original = original_db(&db, task)?;
    if serde_json::to_string(&record)? != raw
        || sha256(raw.as_bytes()) != digest
        || record.format_version != 1
        || record.original_task_id != task
        || record.resolver_task_id != resolver
        || record.endpoint != endpoint
        || record.endpoint != original.endpoint
        || record.operation != operation
        || record.query_evidence_sha256 != evidence
        || evidence != original.evidence
        || record.intent_sha256 != intent_hash
        || intent_hash != sha256(&serde_json::to_vec(&original.intent)?)
        || record.inventory_sha256 != inventory
        || !valid_digest(&inventory)
        || record.resolution_id != id
        || !record.operator_approved
        || !record.provider_stop_acknowledged
        || record.provider_stop_proven
        || record.original_outcome != "unknown"
        || record.retired_count > 1024
        || original.state != "interrupted"
        || Uuid::parse_str(&id).is_err()
        || Uuid::parse_str(&resolver).is_err()
    {
        bail!("Resolution binding or acknowledgement changed")
    }
    match original.action {
        Mutation::Load {
            ref model,
            context_length,
        } => {
            if record.operation != "load_absent"
                || record.model_key.as_ref() != Some(model)
                || record.context_length != Some(context_length)
                || !matches!(
                    record.model_type.as_deref(),
                    Some("llm" | "embedding" | "decision")
                )
                || record.receipt_id.is_some()
                || record.instance_id.is_some()
            {
                bail!("Resolution differs from original load")
            }
        }
        Mutation::Unload { ref instance_id } => {
            if record.operation != "release_absent"
                || record.instance_id.as_ref() != Some(instance_id)
                || record.receipt_id.as_deref() != original.intent["receipt_id"].as_str()
                || record.receipt_id.is_none()
                || record.model_key.is_some()
                || record.model_type.is_some()
                || record.context_length.is_some()
            {
                bail!("Resolution differs from original release")
            }
        }
    }
    let status: String =
        db.query_row("SELECT status FROM tasks WHERE id=?1", [&resolver], |r| {
            r.get(0)
        })?;
    if status != "completed" {
        bail!("Resolver task is not complete")
    }
    let mut stmt=db.prepare("SELECT source,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind='managed_residency_resolution' ORDER BY id LIMIT 3")?;
    let events = stmt
        .query_map([&resolver], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if events.len() != 2 {
        bail!("Resolution audit missing or duplicated")
    }
    for ((source, raw), state) in events
        .iter()
        .zip(["resolution_requested", "operator_resolved"])
    {
        if source != "human-cli"
            || crate::strict_json::value(raw.as_bytes(), 4096)? != event(&record, state, &digest)
        {
            bail!("Resolution audit mismatch")
        }
    }
    drop(stmt);
    let mut stmt=db.prepare("SELECT CASE WHEN length(CAST(member_json AS BLOB))<=16384 THEN member_json END,member_sha256,receipt_id FROM managed_residency_resolution_members WHERE resolution_id=?1 ORDER BY receipt_id LIMIT 1025")?;
    let rows = stmt
        .query_map([&id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != record.retired_count {
        bail!("Resolution member count changed")
    }
    let mut members = Vec::new();
    for (raw, hash, receipt_id) in rows {
        let member: Member =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 16384)?)?;
        if serde_json::to_string(&member)? != raw
            || member.receipt_id != receipt_id
            || sha256(raw.as_bytes()) != hash
            || !valid_digest(&member.prior_inventory_sha256)
            || !matches!(
                member.prior_state.as_str(),
                "active" | "quarantined" | "release_requested" | "retired"
            )
        {
            bail!("Resolution member changed")
        }
        members.push(member);
    }
    if members_digest(&members)? != record.members_sha256 {
        bail!("Resolution membership digest changed")
    }
    drop(stmt);
    drop(db);
    for member in &members {
        let saved = load_receipt_evidence(store, &member.receipt_id)?;
        if saved.state != "retired"
            || receipt_hash(&saved.receipt)?.1 != member.receipt_sha256
            || saved.receipt.load_task_id != member.load_task_id
            || saved.release_task_id != member.release_task_id
            || saved.current_inventory_sha256 != member.prior_inventory_sha256
            || saved.receipt.endpoint != record.endpoint
        {
            bail!("Resolution member has no matching inactive receipt")
        }
    }
    if record.operation == "release_absent"
        && !members
            .iter()
            .any(|m| Some(m.receipt_id.as_str()) == record.receipt_id.as_deref())
    {
        bail!("Release resolution lacks its retired receipt")
    }
    Ok(Some(record))
}
pub(super) fn is_resolved(store: &Store, task: &str, endpoint: &str) -> Result<bool> {
    Ok(validated(store, task)?.is_some_and(|record| record.endpoint == endpoint))
}
pub(super) fn validate_retired(store: &Store, id: &str) -> Result<()> {
    let tasks = {
        let db = store.lock()?;
        let mut s=db.prepare("SELECT r.original_task_id FROM managed_residency_resolution_members m JOIN managed_residency_resolutions r ON r.resolution_id=m.resolution_id WHERE m.receipt_id=?1 ORDER BY r.resolution_id LIMIT 1025")?;
        let rows = s
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if tasks.is_empty() || tasks.len() > 1024 {
        bail!("Retired receipt has no bounded resolution proof")
    }
    for task in tasks {
        validated(store, &task)?.ok_or_else(|| anyhow::anyhow!("Missing resolution proof"))?;
    }
    Ok(())
}

/// Resolve an unknown effect using explicit testimony and one fresh inventory GET.
/// This retires release authority only, never models or their original history.
pub async fn resolve(
    store: &Store,
    base: &str,
    authenticate: bool,
    task: &str,
    options: Options,
) -> Result<Value> {
    security::authorize("human-cli", "model.residency.change")?;
    if !options.human_approve || !options.provider_stop_acknowledged {
        bail!("Resolution requires approval and an explicit provider-stop acknowledgement")
    }
    let native = endpoint(base)?;
    let _leases = mutation_leases(store)?;
    {
        let db = store.lock()?;
        crate::store::admit_managed_residency_resolution(&db)?;
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM managed_residency_resolutions WHERE original_task_id=?1)",
            [task],
            |r| r.get(0),
        )?;
        if exists {
            bail!("Operation already has a resolution; no replay is permitted")
        }
    }
    let original = {
        let db = store.lock()?;
        original_db(&db, task)?
    };
    if original.endpoint != native.as_str() {
        bail!("Resolution endpoint differs from original operation")
    }
    let inventory = parse(&serde_json::to_vec(&status(base, authenticate).await?)?)?;
    let digest = inventory_digest(&inventory)?;
    let mut record = Record {
        format_version: 1,
        resolution_id: Uuid::new_v4().to_string(),
        resolver_task_id: Uuid::new_v4().to_string(),
        original_task_id: task.into(),
        endpoint: original.endpoint.clone(),
        operation: String::new(),
        query_evidence_sha256: original.evidence.clone(),
        intent_sha256: sha256(&serde_json::to_vec(&original.intent)?),
        inventory_sha256: digest.clone(),
        model_key: None,
        model_type: None,
        context_length: None,
        receipt_id: None,
        instance_id: None,
        operator_approved: true,
        provider_stop_acknowledged: true,
        retire_stale_cohort: options.retire_stale_cohort,
        retired_count: 0,
        members_sha256: String::new(),
        original_outcome: "unknown".into(),
        provider_stop_proven: false,
    };
    let relevant = match &original.action {
        Mutation::Load {
            model,
            context_length,
        } => {
            let model = inventory.iter().find(|m| &m.key == model).ok_or_else(|| {
                anyhow::anyhow!("Original model is missing from native inventory")
            })?;
            if !model.loaded_instances.is_empty() {
                bail!("Unknown load cannot be resolved while its model has loaded instances")
            }
            record.operation = "load_absent".into();
            record.model_key = Some(model.key.clone());
            record.model_type = Some(model.kind.clone());
            record.context_length = Some(*context_length);
            None
        }
        Mutation::Unload { instance_id } => {
            if inventory
                .iter()
                .any(|m| m.loaded_instances.iter().any(|i| &i.id == instance_id))
            {
                bail!("Unknown release instance remains present")
            }
            let id = original.intent["receipt_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Original release lacks receipt"))?;
            let saved = load_receipt_evidence(store, id)?;
            if saved.receipt.endpoint != record.endpoint
                || saved.receipt.instance_id != *instance_id
                || saved.release_task_id.as_deref() != Some(task)
                || !matches!(
                    saved.state.as_str(),
                    "quarantined" | "release_requested" | "retired"
                )
            {
                bail!("Unknown release receipt does not match the original task")
            }
            record.operation = "release_absent".into();
            record.receipt_id = Some(id.into());
            record.instance_id = Some(instance_id.clone());
            Some(saved)
        }
    };
    let mut members = Vec::new();
    for saved in receipt_rows_for_endpoint(store, &record.endpoint)? {
        let is_relevant = record.receipt_id.as_deref() == Some(saved.receipt.receipt_id.as_str());
        if matches!(saved.state.as_str(), "released" | "retired") && !is_relevant {
            continue;
        }
        let stable = saved.state == "active"
            && saved.current_inventory_sha256 == digest
            && model_instance(&inventory, &saved.receipt).is_some();
        if !is_relevant && stable {
            continue;
        }
        if !is_relevant && !options.retire_stale_cohort {
            bail!("Stale endpoint receipts require explicit cohort-authority retirement")
        }
        members.push(member(&saved)?);
    }
    if let Some(saved) = relevant {
        if !members
            .iter()
            .any(|m| m.receipt_id == saved.receipt.receipt_id)
        {
            members.push(member(&saved)?);
        }
    }
    members.sort_by(|a, b| a.receipt_id.cmp(&b.receipt_id));
    if members.len() > 1024 {
        bail!("Resolution cohort exceeds bounds")
    }
    record.retired_count = members.len();
    record.members_sha256 = members_digest(&members)?;
    let raw = serde_json::to_string(&record)?;
    if raw.len() > 4096 {
        bail!("Resolution metadata exceeds bounds")
    }
    let hash = sha256(raw.as_bytes());
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if original_db(&tx, task)?.evidence != record.query_evidence_sha256 {
        bail!("Original operation changed during resolution")
    }
    tx.execute("UPDATE tasks SET status='interrupted',updated=unixepoch() WHERE id=?1 AND status IN ('running','interrupted')",[task])?;
    crate::store::admit_managed_residency_resolution(&tx)?;
    crate::storage::admit(&tx)?;
    tx.execute("INSERT INTO tasks(id,query,mode,status,answer,created,updated) VALUES(?1,'Operator resolution of unknown native residency effect','fast','completed','Original provider outcome remains unknown; release authority retired',unixepoch(),unixepoch())",[&record.resolver_task_id])?;
    tx.execute("INSERT INTO managed_residency_resolutions(resolution_id,original_task_id,resolver_task_id,endpoint,operation,query_evidence_sha256,intent_sha256,inventory_sha256,model_key,model_type,context_length,receipt_id,instance_id,operator_approved,provider_stop_acknowledged,payload_json,payload_sha256,created) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,1,1,?14,?15,unixepoch())",params![record.resolution_id,task,record.resolver_task_id,record.endpoint,record.operation,record.query_evidence_sha256,record.intent_sha256,record.inventory_sha256,record.model_key,record.model_type,record.context_length,record.receipt_id,record.instance_id,raw,hash])?;
    for member in &members {
        let raw = serde_json::to_string(member)?;
        tx.execute("INSERT INTO managed_residency_resolution_members(resolution_id,receipt_id,member_json,member_sha256) VALUES(?1,?2,?3,?4)",params![record.resolution_id,member.receipt_id,raw,sha256(raw.as_bytes())])?;
        let changed=tx.execute("UPDATE managed_residency_receipts SET state='retired',updated=unixepoch() WHERE receipt_id=?1 AND state=?2 AND current_inventory_sha256=?3 AND receipt_sha256=?4",params![member.receipt_id,member.prior_state,member.prior_inventory_sha256,member.receipt_sha256])?;
        if changed!=1 && !(member.prior_state=="retired" && tx.query_row("SELECT EXISTS(SELECT 1 FROM managed_residency_receipt_archive WHERE receipt_id=?1 AND state='retired' AND receipt_sha256=?2)",params![member.receipt_id,member.receipt_sha256],|r|r.get::<_,bool>(0))?) {bail!("Resolution cohort changed before commit")}
    }
    for state in ["resolution_requested", "operator_resolved"] {
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'managed_residency_resolution','human-cli',?2,unixepoch())",params![record.resolver_task_id,event(&record,state,&hash).to_string()])?;
    }
    tx.commit()?;
    Ok(
        json!({"resolution_id":record.resolution_id,"resolver_task_id":record.resolver_task_id,"original_task_id":task,"retired_receipts":record.retired_count,"original_outcome":"unknown","provider_stop_acknowledged":true,"provider_stop_proven":false,"model_mutation_performed":false,"automatic_retry":false,"ownership_established":false}),
    )
}
fn member(saved: &StoredReceipt) -> Result<Member> {
    Ok(Member {
        receipt_id: saved.receipt.receipt_id.clone(),
        receipt_sha256: receipt_hash(&saved.receipt)?.1,
        prior_state: saved.state.clone(),
        prior_inventory_sha256: saved.current_inventory_sha256.clone(),
        load_task_id: saved.receipt.load_task_id.clone(),
        release_task_id: saved.release_task_id.clone(),
    })
}
