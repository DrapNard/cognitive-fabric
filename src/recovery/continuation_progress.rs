//! Reconstruct finite-domain progress from bounded reasoning and verifier events.
use crate::config::Config;
use anyhow::{bail, Result};
use serde_json::Value;
use std::collections::BTreeMap;

type VerificationAudit = (i64, String, bool, Option<String>, Option<String>);

fn validate_published_memory_check(
    query: &str,
    criterion: &crate::verification::Criterion,
    source: &str,
    event_id: i64,
    context_anchor: Option<i64>,
    payload: &Value,
) -> Result<()> {
    let fields = [
        "criterion",
        "verified",
        "origin",
        "memory_id",
        "expert",
        "objective_hash",
        "answer_hash",
        "reason",
    ];
    if source != "verifier-r"
        || payload.as_object().is_none_or(|object| {
            object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field))
        })
        || context_anchor.is_none_or(|anchor| event_id >= anchor)
        || payload["criterion"] != criterion.name()
        || payload["verified"] != false
        || payload["origin"] != "published_memory"
        || payload["objective_hash"] != crate::store::sha256(query.as_bytes())
    {
        bail!("Malformed or out-of-order published-memory verification")
    }
    let memory_id = payload["memory_id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 64)
        .ok_or_else(|| anyhow::anyhow!("Invalid published-memory identity"))?;
    uuid::Uuid::parse_str(memory_id)?;
    if !payload["expert"].is_null() && !payload["expert"].is_object() {
        bail!("Malformed published-memory checker identity")
    }
    if payload["answer_hash"].as_str().is_none_or(|hash| {
        hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }) {
        bail!("Invalid published-memory answer hash")
    }
    if payload["reason"]
        .as_str()
        .is_none_or(|reason| reason.is_empty() || reason.len() > 16384)
    {
        bail!("Invalid published-memory verification reason")
    }
    Ok(())
}

/// Rebuild the progress state from solver text and independent verifier events. Stored
/// progress audits are treated as claims and must exactly match this reconstruction.
pub(super) fn validate(
    db: &rusqlite::Connection,
    task: &str,
    query: &str,
    cfg: &Config,
    boundary: Option<&Value>,
) -> Result<crate::reasoning::progress::History> {
    validate_before(db, task, query, cfg, boundary, None, None)
}

pub(super) fn validate_until(
    db: &rusqlite::Connection,
    task: &str,
    query: &str,
    cfg: &Config,
    boundary: Option<&Value>,
    before_id: Option<i64>,
) -> Result<crate::reasoning::progress::History> {
    validate_before(db, task, query, cfg, boundary, before_id, None)
}

pub(super) fn validate_before(
    db: &rusqlite::Connection,
    task: &str,
    query: &str,
    cfg: &Config,
    boundary: Option<&Value>,
    before_id: Option<i64>,
    before_boundary_id: Option<i64>,
) -> Result<crate::reasoning::progress::History> {
    use crate::verification::Criterion;
    let enabled = cfg.progress_stopping.is_some();
    let criterion = Criterion::from_query(query)?;
    let active = enabled
        && criterion
            .as_ref()
            .is_some_and(|c| c.name() == "finite_domain/v1");
    if !active {
        if boundary.is_some_and(|v| v.get("progress_state").is_some()) {
            bail!("Unexpected progress checkpoint state")
        }
        return Ok(crate::reasoning::progress::History::default());
    }
    let settings = cfg.progress_stopping.as_ref().unwrap();
    settings.validate()?;
    let criterion = criterion.unwrap();
    if let Some(boundary_id) = before_boundary_id {
        let mut statement = db.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND id>?2 AND kind IN ('reasoning','verification','reasoning_progress') ORDER BY id LIMIT 34")?;
        let rows = statement.query_map(rusqlite::params![task, boundary_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut count = 0usize;
        for row in rows {
            count += 1;
            if count > 32 {
                bail!("Post-boundary progress history exceeds bounds")
            }
            let (kind, source, raw) = row?;
            let payload = crate::strict_json::value(raw.as_bytes(), 262144)?;
            match kind.as_str() {
                "reasoning_progress" => {
                    bail!("Future progress assessment follows the saved boundary")
                }
                "verification" if payload["criterion"] == criterion.name() => {
                    bail!("Future independent verification follows the saved boundary")
                }
                "reasoning"
                    if payload["node"]["role"] == "solver"
                        && payload["node"]["cycle"].as_u64().unwrap_or(0) > 0 =>
                {
                    if source != "jev-r" {
                        bail!("Invalid future whole-goal solver source")
                    }
                    bail!("Future whole-goal solver result follows the saved boundary")
                }
                _ => {}
            }
        }
    }
    let context_anchor: Option<i64> = db.query_row(
        "SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_context'",
        [task],
        |row| row.get(0),
    )?;
    let mut stmt = db.prepare("SELECT id,kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('reasoning','verification','reasoning_progress','reasoning_context') AND (?2 IS NULL OR id<?2) AND (?3 IS NULL OR id<?3) ORDER BY id LIMIT 210")?;
    let rows = stmt.query_map(
        rusqlite::params![task, before_id, before_boundary_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let mut solvers = BTreeMap::<u32, (i64, String, String)>::new();
    let mut verifications = BTreeMap::<u32, Vec<VerificationAudit>>::new();
    let mut progress_events = BTreeMap::<u32, (i64, Value)>::new();
    let mut event_count = 0usize;
    let mut total_bytes = 0usize;
    for row in rows {
        event_count += 1;
        let (event_id, kind, source, raw) = row?;
        total_bytes = total_bytes.saturating_add(raw.len());
        if event_count > 209 || total_bytes > 8_000_000 {
            bail!("Progress continuation history exceeds bounds")
        }
        let payload = crate::strict_json::value(raw.as_bytes(), 262144)?;
        match kind.as_str() {
            "reasoning" => {
                if source != "jev-r" {
                    bail!("Invalid progress solver source")
                }
                if payload["node"]["role"] != "solver" {
                    continue;
                }
                let node: crate::reasoning::Node = serde_json::from_value(payload["node"].clone())?;
                if payload.as_object().is_none_or(|object| {
                    object.keys().any(|key| {
                        !matches!(
                            key.as_str(),
                            "call"
                                | "node"
                                | "text"
                                | "input_tokens"
                                | "output_tokens"
                                | "inference_ms"
                                | "queue_wait_ms"
                        )
                    })
                }) {
                    bail!("Unexpected field in saved progress solver event")
                }
                let cycle = payload["node"]["cycle"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| anyhow::anyhow!("Invalid solver cycle in progress history"))?;
                if cycle == 0 {
                    continue;
                }
                let answer = payload["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing whole-goal solver text"))?;
                let hash = crate::store::sha256(answer.as_bytes());
                if answer.len() > 96_000
                    || node.cycle != cycle
                    || node.content_hash != hash
                    || solvers
                        .insert(cycle, (event_id, answer.to_owned(), hash))
                        .is_some()
                {
                    bail!("Duplicate or altered whole-goal solver result")
                }
            }
            "verification" => {
                if source != "verifier-r" {
                    bail!("Invalid progress verifier source")
                }
                if payload["origin"] == "published_memory" {
                    validate_published_memory_check(
                        query,
                        &criterion,
                        &source,
                        event_id,
                        context_anchor,
                        &payload,
                    )?;
                    continue;
                }
                if payload["criterion"] != criterion.name() {
                    continue;
                }
                if payload.as_object().is_none_or(|object| {
                    object.keys().any(|key| {
                        !matches!(
                            key.as_str(),
                            "criterion"
                                | "verified"
                                | "expert"
                                | "objective_hash"
                                | "answer_hash"
                                | "reason"
                                | "cycle"
                                | "origin"
                        )
                    })
                }) {
                    bail!("Unexpected field in independent progress verification")
                }
                let cycle = payload["cycle"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| anyhow::anyhow!("Invalid verification cycle"))?;
                if payload["objective_hash"] != crate::store::sha256(query.as_bytes()) {
                    bail!("Progress verifier objective differs from the saved task")
                }
                let hash = payload["answer_hash"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing verifier answer hash"))?
                    .to_owned();
                let verified = payload["verified"]
                    .as_bool()
                    .ok_or_else(|| anyhow::anyhow!("Missing verifier result"))?;
                let reason = payload["reason"].as_str().map(str::to_owned);
                let origin = payload["origin"].as_str().map(str::to_owned);
                if payload
                    .get("reason")
                    .is_some_and(|value| !value.is_null() && !value.is_string())
                    || payload
                        .get("origin")
                        .is_some_and(|value| !value.is_string())
                    || payload
                        .get("expert")
                        .is_some_and(|value| !value.is_null() && !value.is_object())
                {
                    bail!("Malformed independent progress verification")
                }
                verifications
                    .entry(cycle)
                    .or_default()
                    .push((event_id, hash, verified, reason, origin));
            }
            "reasoning_progress" => {
                if source != "verifier-r" {
                    bail!("Invalid progress audit source")
                }
                let cycle = payload["cycle"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| anyhow::anyhow!("Invalid progress audit cycle"))?;
                if progress_events.insert(cycle, (event_id, payload)).is_some() {
                    bail!("Duplicate progress assessment")
                }
            }
            "reasoning_context" => {
                if source != "jev-r" {
                    bail!("Invalid progress context anchor source")
                }
            }
            _ => unreachable!(),
        }
    }
    drop(stmt);

    if solvers.len() > 16 || progress_events.len() > 16 {
        bail!("Progress continuation exceeds the cycle cap")
    }
    let mut history = crate::reasoning::progress::History::default();
    let mut cycles = solvers.keys().copied().collect::<Vec<_>>();
    cycles.sort_unstable();
    if cycles
        .iter()
        .enumerate()
        .any(|(index, cycle)| *cycle as usize != index + 1)
    {
        bail!("Whole-goal progress solver cycles are gapped")
    }
    for (position, cycle) in cycles.iter().copied().enumerate() {
        let (solver_id, answer, answer_hash) = &solvers[&cycle];
        let checks = verifications.remove(&cycle).ok_or_else(|| {
            anyhow::anyhow!("Missing independent verification for progress solver")
        })?;
        let verifier_reason = criterion
            .check(answer)
            .err()
            .map(|error| error.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!("Recorded candidate passes its independent criterion")
            })?;
        let canonical_checks = checks.iter().filter(|check| check.4.is_none()).count();
        let recovery_checks = checks
            .iter()
            .filter(|check| check.4.as_deref() == Some("saved_solver_recovery"))
            .count();
        let supported_check_set = (checks.len() == 1
            && (canonical_checks == 1 || recovery_checks == 1))
            || (checks.len() == 2 && canonical_checks == 1 && recovery_checks == 1);
        if !supported_check_set
            || checks.iter().any(|check| {
                check.1 != *answer_hash
                    || check.2
                    || check.0 <= *solver_id
                    || cycles
                        .get(position + 1)
                        .is_some_and(|next| check.0 >= solvers[next].0)
                    || check.3.as_deref() != Some(verifier_reason.as_str())
            })
        {
            bail!("Progress assessment lacks the supported matching failed verification history")
        }
        let expected = history.observe(query, cycle, answer, settings)?;
        let (assessment_id, recorded) = progress_events
            .remove(&cycle)
            .ok_or_else(|| anyhow::anyhow!("Missing durable progress assessment"))?;
        let first_check_id = checks.iter().map(|check| check.0).min().unwrap();
        if recorded != expected
            || assessment_id <= first_check_id
            || cycles
                .get(position + 1)
                .is_some_and(|next| assessment_id >= solvers[next].0)
        {
            bail!("Recorded progress assessment differs from independent recomputation")
        }
        if expected["stop"] == true && boundary.is_some() {
            bail!("A plateaued finite-domain task cannot continue reasoning")
        }
    }
    if !progress_events.is_empty() {
        bail!("Progress assessment has no corresponding solver output")
    }
    if !verifications.is_empty() {
        bail!("Independent verification has no corresponding whole-goal solver output")
    }
    if let Some(boundary) = boundary {
        let snapshot = boundary
            .get("progress_state")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing progress checkpoint state"))?;
        let fields = [
            "objective_hash",
            "observed_cycles",
            "best_score",
            "non_improving_cycles",
            "last_score",
        ];
        if snapshot.as_object().is_none_or(|object| {
            object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field))
        }) {
            bail!("Malformed progress checkpoint snapshot")
        }
        let saved: crate::reasoning::progress::History = serde_json::from_value(snapshot)?;
        if saved != history {
            bail!("Progress checkpoint differs from recomputed failed-answer history")
        }
    }
    Ok(history)
}
