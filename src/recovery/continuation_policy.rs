//! Validate an original policy allowance before restoring a custom revision.
use crate::{config::Config, escalation};
use anyhow::{bail, Result};
use serde_json::Value;

pub(super) fn validate_saved_decision(
    db: &rusqlite::Connection,
    task: &str,
    boundary_id: u64,
    boundary: &Value,
    policy: &escalation::Handle,
    cfg: &Config,
    context: &str,
) -> Result<()> {
    let mut statement=db.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND id<?2 AND kind IN ('escalation_intent','escalation_decision') ORDER BY id LIMIT 33")?;
    let mut rows = statement.query(rusqlite::params![task, boundary_id])?;
    let mut count = 0;
    let mut selected = Vec::new();
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 32 {
            bail!("Excessive saved escalation decisions")
        }
        let kind: String = row.get(0)?;
        let source: String = row.get(1)?;
        let bytes: String = row.get(2)?;
        let payload = crate::strict_json::value(bytes.as_bytes(), 16384)?;
        let cycle = if kind == "escalation_intent" {
            &payload["request"]["cycle"]
        } else {
            &payload["cycle"]
        };
        if cycle == &boundary["cycles_before"] {
            if source != "jev-plus"
                || payload["format_version"] != 1
                || payload["policy"] != serde_json::to_value(policy.descriptor())?
                || payload["permissions_granted"] != false
            {
                bail!("Saved escalation policy decision identity changed")
            }
            selected.push((kind, payload));
        }
    }
    if selected.len() != 2
        || selected[0].0 != "escalation_intent"
        || selected[1].0 != "escalation_decision"
    {
        bail!("Missing or ambiguous original escalation allowance")
    }
    let request = &selected[0].1["request"];
    let decision = &selected[1].1;
    let query = db.query_row::<String, _, _>(
        "SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END FROM tasks WHERE id=?1",
        [task],
        |r| r.get(0),
    )?;
    let structured = crate::verification::Criterion::from_query(&query)?;
    let criterion = structured.as_ref().map(|value| value.name());
    let (reason, calls, outputs, tools) = if criterion.is_some() {
        ("independent_rejection", 1, 384, 1)
    } else {
        ("unverified_answer", 3, 640, 0)
    };
    let reason_code = decision["decision"]["reason_code"].as_str().unwrap_or("");
    let base = crate::reasoning::Session::prompt_base(&query, context, true)?;
    let answer = boundary["previous_answer"].as_str().unwrap();
    let bounded = |text: &str, count: usize| text.chars().take(count).collect::<String>();
    let prompt_bytes = if criterion.is_some() {
        format!(
            "{base}\nUnverified strategy: {}\nPrevious unverified answer: {}",
            bounded(boundary["strategy"].as_str().unwrap(), 4000),
            bounded(answer, 6000)
        )
        .len()
    } else {
        let critique=format!("{base}\nUnverified candidate: {}\nCritique your candidate. Return only JSON with revise (boolean) and reason (string). Never claim verification or permissions.",bounded(answer,6000));
        let reflection = format!(
            "{base}\nCandidate flaw (unverified advice): {}\nCandidate: {}",
            "x".repeat(1000),
            bounded(answer, 4000)
        );
        let solver = format!(
            "{base}\nUnverified strategy: {}\nPrevious unverified answer: {}",
            "x".repeat(16000),
            bounded(answer, 6000)
        );
        critique
            .len()
            .saturating_add(reflection.len())
            .saturating_add(solver.len())
    };
    if request["mode"] != boundary["mode"]
        || request["candidate_sha256"] != boundary["last_answer_hash"]
        || decision["candidate_sha256"] != boundary["last_answer_hash"]
        || decision["decision"]["allow"] != true
        || request["verification_criterion"] != serde_json::to_value(criterion)?
        || request["configuration_sha256"] != escalation::empirical::configuration_fingerprint(cfg)?
        || request["role_prompts_sha256"] != crate::backend::runtime_role_prompts_sha256()?
        || request["runtime_sources_sha256"] != escalation::empirical::runtime_sources_fingerprint()
        || request["reason"] != reason
        || request["required_calls"] != calls
        || request["required_output_tokens"] != outputs
        || request["required_tool_calls"] != tools
        || request["required_prompt_bytes"] != prompt_bytes
        || request["remaining_calls"]
            .as_u64()
            .is_none_or(|n| n < calls)
        || request["remaining_output_tokens"]
            .as_u64()
            .is_none_or(|n| n < outputs)
        || request["remaining_tool_calls"]
            .as_u64()
            .is_none_or(|n| n < tools)
        || request["remaining_prompt_bytes"]
            .as_u64()
            .is_none_or(|n| n < prompt_bytes as u64)
        || request["remaining_ms"].as_u64().is_none_or(|n| n == 0)
        || !crate::registry::identifier(reason_code)
        || reason_code.len() > 64
    {
        bail!("Original escalation allowance does not cover this revision")
    }
    Ok(())
}
