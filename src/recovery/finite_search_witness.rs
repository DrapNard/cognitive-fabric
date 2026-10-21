//! Validate and expose an atomic direct finite-search witness receipt.
use super::finite_search::{exact_fields, read_events, MAX_EVENT_BYTES};
use crate::{config::Config, store::sha256, verification::Criterion};
use anyhow::{bail, Result};
use serde_json::{json, Value};

/// A direct witness persisted atomically with the completed search receipt.
/// The report metadata deliberately omits the answer body.
#[derive(Clone, Debug)]
pub(super) struct Candidate {
    pub(super) answer: String,
    pub(super) history: Value,
    pub(super) receipt_id: i64,
}

/// Recover a direct search witness only after validating its original bounded
/// authorization and receipt chain. This is metadata validation; the caller
/// must reserve and run the original full criterion checker before returning it.
pub(super) fn candidate(
    db: &rusqlite::Connection,
    task: &str,
    query: &str,
) -> Result<Option<Candidate>> {
    let criterion = Criterion::from_query(query)?;
    if !criterion
        .as_ref()
        .is_some_and(|criterion| criterion.name() == "finite_domain/v1")
    {
        return Ok(None);
    }

    let events = read_events(db, task)?;
    let receipts = events
        .iter()
        .filter(|event| event.kind == "finite_search")
        .collect::<Vec<_>>();
    let witness_receipts = receipts
        .iter()
        .filter(|event| {
            event.payload["outcome"] == "witness"
                || event.payload["format_version"].as_u64() == Some(2)
        })
        .collect::<Vec<_>>();
    if witness_receipts.is_empty() {
        // A v1 limit receipt remains the existing saved-solver recovery path's
        // responsibility. No direct witness candidate is present.
        return Ok(None);
    }
    if receipts.len() != 1 || witness_receipts.len() != 1 {
        bail!("Direct finite-search witness has duplicate or conflicting receipts")
    }
    let receipt = witness_receipts[0];

    let settings_count: u64 = db.query_row(
        "SELECT count(*) FROM events WHERE task_id=?1 AND kind='execution_settings'",
        [task],
        |row| row.get(0),
    )?;
    let (settings_id, settings_source, raw_settings): (i64, String, String) = db.query_row(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 1",
        [task],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if settings_count != 1 || settings_source != "runtime" {
        bail!("Direct finite-search witness lacks runtime execution settings")
    }
    let settings = crate::strict_json::value(raw_settings.as_bytes(), MAX_EVENT_BYTES)?;
    let setting_fields = [
        "format_version",
        "resource_reservations_version",
        "configuration",
        "mode",
        "execution_clock",
        "call_limit",
        "cycle_limit",
        "backend_configuration_attested",
        "weights_attested",
        "runtime_role_prompts_sha256",
        "permissions_restorable_from_settings",
    ];
    if !exact_fields(&settings, &setting_fields)
        || settings["format_version"] != 1
        || settings["resource_reservations_version"] != 1
        || settings["backend_configuration_attested"] != false
        || settings["weights_attested"] != false
        || settings["permissions_restorable_from_settings"] != false
    {
        bail!("Direct finite-search execution settings are malformed")
    }
    let cfg: Config = serde_json::from_value(settings["configuration"].clone())?;
    cfg.validate()?;
    if serde_json::to_value(&cfg)? != settings["configuration"] {
        bail!("Direct finite-search configuration is incomplete or noncanonical")
    }
    let search_settings = cfg.finite_search.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Direct finite-search receipt has no enabled configuration")
    })?;
    search_settings.validate()?;

    let contexts = events
        .iter()
        .filter(|event| event.kind == "reasoning_context")
        .collect::<Vec<_>>();
    let context = contexts
        .first()
        .ok_or_else(|| anyhow::anyhow!("Original reasoning context is missing"))?;
    if contexts.len() != 1 || context.id <= settings_id || context.source != "jev-r" {
        bail!("Direct finite-search receipt is not anchored to the original reasoning context")
    }
    let context_fields = [
        "format_version",
        "prompt_format",
        "objective",
        "context",
        "objective_hash",
        "context_hash",
        "prompt_base_hash",
        "calls_before",
        "cycles_before",
        "untrusted_data",
        "permissions_restorable",
    ];
    let context_body = context.payload["context"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Original reasoning context body is missing"))?;
    let calls_before = events
        .iter()
        .filter(|event| event.kind == "model_dispatch" && event.id < context.id)
        .count() as u64;
    let expected_prompt_base = crate::reasoning::Session::prompt_base(query, context_body, true)?;
    if !exact_fields(&context.payload, &context_fields)
        || context.payload["format_version"] != 1
        || context.payload["prompt_format"] != crate::reasoning::PROMPT_FORMAT
        || context.payload["objective"] != query
        || context_body.len() > 96_000
        || context.payload["objective_hash"] != sha256(query.as_bytes())
        || context.payload["context_hash"] != sha256(context_body.as_bytes())
        || context.payload["prompt_base_hash"] != sha256(expected_prompt_base.as_bytes())
        || context.payload["calls_before"].as_u64() != Some(calls_before)
        || context.payload["cycles_before"] != 0
        || context.payload["untrusted_data"] != true
        || context.payload["permissions_restorable"] != false
    {
        bail!("Original reasoning context does not match the direct finite-search objective")
    }

    let policies = events
        .iter()
        .filter(|event| {
            event.kind == "policy" && event.payload["capability"] == "constraints.solve"
        })
        .collect::<Vec<_>>();
    let charges = events
        .iter()
        .filter(|event| {
            event.kind == "tool_reservation" && event.payload["capability"] == "constraints.solve"
        })
        .collect::<Vec<_>>();
    if policies.len() != 1 || charges.len() != 1 {
        bail!("Direct finite-search witness lacks a unique authorization and charge")
    }
    let policy = policies[0];
    let charge = charges[0];
    let expected_policy = json!({
        "task_id": task,
        "actor": "jev-plus",
        "capability": "constraints.solve",
        "allowed": true,
        "reason": null
    });
    if policy.source != "security-kernel"
        || !exact_fields(
            &policy.payload,
            &["task_id", "actor", "capability", "allowed", "reason"],
        )
        || policy.payload != expected_policy
        || charge.source != "security-kernel"
        || !exact_fields(
            &charge.payload,
            &["format_version", "invocation", "actor", "capability"],
        )
        || charge.payload["format_version"] != 1
        || charge.payload["actor"] != "jev-plus"
        || charge.payload["capability"] != "constraints.solve"
        || context.id >= policy.id
        || policy.id >= charge.id
        || charge.id >= receipt.id
        || settings_id >= context.id
    {
        bail!("Direct finite-search authorization and receipt are malformed or out of order")
    }
    let phase = events
        .iter()
        .filter(|event| context.id <= event.id && event.id <= receipt.id)
        .collect::<Vec<_>>();
    if phase.len() != 4
        || phase[0].id != context.id
        || phase[1].id != policy.id
        || phase[2].id != charge.id
        || phase[3].id != receipt.id
        || phase[0].kind != "reasoning_context"
        || phase[1].kind != "policy"
        || phase[2].kind != "tool_reservation"
        || phase[3].kind != "finite_search"
    {
        bail!("Direct finite-search receipt is not the exact context-policy-charge-receipt chain")
    }

    let prior_tool_charges = events
        .iter()
        .filter(|event| event.kind == "tool_reservation" && event.id <= charge.id)
        .count() as u64;
    let max_nodes = search_settings.max_nodes as u64;
    if charge.payload["invocation"].as_u64() != Some(prior_tool_charges)
        || prior_tool_charges > (cfg.max_tool_calls as u64).saturating_sub(1)
        || receipt.source != "jev-r"
        || !exact_fields(
            &receipt.payload,
            &[
                "format_version",
                "criterion",
                "objective_sha256",
                "max_nodes",
                "explored_nodes",
                "outcome",
                "verified",
                "permissions_granted",
                "answer",
                "answer_sha256",
            ],
        )
        || receipt.payload["format_version"] != 2
        || receipt.payload["criterion"] != "finite_domain/v1"
        || receipt.payload["objective_sha256"] != sha256(query.as_bytes())
        || receipt.payload["max_nodes"].as_u64() != Some(max_nodes)
        || receipt.payload["outcome"] != "witness"
        || receipt.payload["verified"] != false
        || receipt.payload["permissions_granted"] != false
    {
        bail!("Direct finite-search witness receipt does not match original limits and charges")
    }
    receipt.payload["explored_nodes"]
        .as_u64()
        .filter(|nodes| *nodes > 0 && *nodes <= max_nodes)
        .ok_or_else(|| anyhow::anyhow!("Direct finite-search node count is outside its bound"))?;
    let answer = receipt.payload["answer"]
        .as_str()
        .filter(|answer| !answer.is_empty() && answer.len() <= 8192)
        .ok_or_else(|| anyhow::anyhow!("Direct finite-search assignment exceeds its byte bound"))?
        .to_owned();
    let assignment = crate::strict_json::value(answer.as_bytes(), 8192)?;
    if !assignment.as_object().is_some_and(|values| {
        !values.is_empty()
            && values.len() <= 32
            && values.values().all(|value| value.as_i64().is_some())
    }) || receipt.payload["answer_sha256"] != sha256(answer.as_bytes())
    {
        bail!("Direct finite-search answer is not a hashed strict integer assignment")
    }

    // Search's verifier and any successful task completion must be a later,
    // separately authorized action. No model or graph work can follow its result.
    let recorded_late_work: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND id>?2 AND kind IN ('model_dispatch','cycle_reservation','subtask','subtask_schedule_intent','subtask_schedule','plan','reasoning_boundary'))",
        rusqlite::params![task, receipt.id],
        |row| row.get(0),
    )?;
    let late_policies = events
        .iter()
        .filter(|event| event.id > receipt.id && event.kind == "policy")
        .collect::<Vec<_>>();
    if late_policies.iter().any(|event| {
        event.source != "security-kernel"
            || !exact_fields(
                &event.payload,
                &["task_id", "actor", "capability", "allowed", "reason"],
            )
            || event.payload["task_id"] != task
            || event.payload["actor"] != "jev-plus"
            || event.payload["capability"] != "constraints.verify"
            || event.payload["allowed"] != true
            || !event.payload["reason"].is_null()
    }) {
        bail!("Unexpected post-search policy decision follows a direct witness")
    }
    let late_charges = events
        .iter()
        .filter(|event| event.id > receipt.id && event.kind == "tool_reservation")
        .collect::<Vec<_>>();
    let mut invocation = prior_tool_charges;
    if late_charges.iter().any(|event| {
        invocation += 1;
        event.source != "security-kernel"
            || !((exact_fields(
                &event.payload,
                &["format_version", "invocation", "actor", "capability"],
            ) && event.payload["origin"].is_null())
                || (exact_fields(
                    &event.payload,
                    &[
                        "format_version",
                        "invocation",
                        "actor",
                        "capability",
                        "origin",
                    ],
                ) && event.payload["origin"] == "operator_recovery"))
            || event.payload["format_version"] != 1
            || event.payload["invocation"].as_u64() != Some(invocation)
            || event.payload["actor"] != "jev-plus"
            || event.payload["capability"] != "constraints.verify"
    }) {
        bail!("Unexpected post-search tool reservation follows a direct witness")
    }
    if invocation > cfg.max_tool_calls as u64
        || recorded_late_work
        || events.iter().any(|event| {
            event.id > receipt.id
                && matches!(
                    event.kind.as_str(),
                    "model_dispatch" | "cycle_reservation" | "subtask" | "reasoning_boundary"
                )
        })
    {
        bail!("Reasoning or graph work follows a direct finite-search witness")
    }

    let mut history = receipt.payload.clone();
    history.as_object_mut().unwrap().remove("answer");
    history["event_id"] = json!(receipt.id);
    history["charge_event_id"] = json!(charge.id);
    history["automatic_replay"] = json!(false);
    Ok(Some(Candidate {
        answer,
        history,
        receipt_id: receipt.id,
    }))
}
