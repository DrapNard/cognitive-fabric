//! Exact language-phase state for explicit mid-reasoning continuation.
use super::*;

pub(super) struct Restored {
    pub reservation: Value,
    pub base_calls: usize,
    pub evidence: Vec<String>,
}

pub(super) fn prepare(
    db: &rusqlite::Connection,
    task: &str,
    cfg: &Config,
    settings: &Value,
    query: &str,
    context: &str,
    audit: &Value,
) -> Result<Restored> {
    let saved = &audit["language_boundary"];
    let base_calls = settings["call_limit"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing original call ceiling"))?
        as usize;
    if saved["phase"] != "reasoning"
        || saved["reservation"] != crate::reasoning::language_reservation_payload(cfg, base_calls)?
        || saved["presentation_capacity_held"] != true
    {
        bail!("Language continuation requires a recorded reasoning phase and its original reservation")
    }
    let root_calls = saved["reservation"]["root_call_limit"].as_u64().unwrap();
    if audit["reserved_calls"]
        .as_u64()
        .is_none_or(|used| used > root_calls - 1)
        || audit["reserved_output_tokens"]
            .as_u64()
            .is_none_or(|used| used > (cfg.max_tokens - cfg.interface_tokens) as u64)
        || audit["charged_prompt_bytes"]
            .as_u64()
            .is_none_or(|used| used > (cfg.max_input_bytes - 8192) as u64)
    {
        bail!("Saved reasoning consumed its held presentation capacity")
    }
    let first = audit["calls"]
        .as_array()
        .and_then(|calls| calls.first())
        .ok_or_else(|| anyhow::anyhow!("Missing original interpretation call"))?;
    if first["call"] != 1 || first["role"] != "interface" || first["state"] != "result_recorded" {
        bail!("Original interpretation is not fully recorded")
    }
    let mut stmt = db.prepare("SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=65792 THEN payload END FROM events WHERE task_id=?1 AND kind='language_evidence_snapshot' ORDER BY id LIMIT 2")?;
    let mut rows = stmt.query([task])?;
    let row = rows
        .next()?
        .ok_or_else(|| anyhow::anyhow!("Missing original language evidence snapshot"))?;
    let id: i64 = row.get(0)?;
    let source: String = row.get(1)?;
    let raw: String = row.get(2)?;
    let snapshot = crate::strict_json::value(raw.as_bytes(), 65792)?;
    if source != "jev-plus" || rows.next()?.is_some() {
        bail!("Invalid or repeated language evidence snapshot")
    }
    let context_id: i64 = db.query_row(
        "SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_context'",
        [task],
        |row| row.get(0),
    )?;
    if id >= context_id {
        bail!("Language evidence snapshot must precede original reasoning")
    }
    let evidence = crate::reasoning::language_boundary::restore_evidence_snapshot(
        cfg, query, context, &snapshot,
    )?;
    Ok(Restored {
        reservation: saved["reservation"].clone(),
        base_calls,
        evidence,
    })
}
