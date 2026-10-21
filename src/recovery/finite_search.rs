//! Validate the durable, completed finite-search phase without replaying it.
use crate::{config::Config, store::sha256, verification::Criterion};
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};

const MAX_EVENTS: usize = 512;
pub(super) const MAX_EVENT_BYTES: usize = 262_144;
const MAX_HISTORY_BYTES: usize = 8_000_000;

/// A validated limit receipt is evidence that a bounded search phase completed,
/// not evidence that a satisfying assignment exists or that search may be rerun.
#[derive(Clone, Debug, Serialize)]
pub(super) struct Receipt {
    pub(super) format_version: u8,
    pub(super) event_id: i64,
    pub(super) charge_event_id: i64,
    pub(super) objective_sha256: String,
    pub(super) max_nodes: u64,
    pub(super) explored_nodes: u64,
    pub(super) outcome: String,
    pub(super) automatic_replay: bool,
    pub(super) verified: bool,
    pub(super) permissions_granted: bool,
}

#[derive(Debug)]
pub(super) struct Event {
    pub(super) id: i64,
    pub(super) kind: String,
    pub(super) source: String,
    pub(super) payload: Value,
}

pub(super) fn exact_fields(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|field| object.contains_key(*field))
    })
}

pub(super) fn read_events(db: &rusqlite::Connection, task: &str) -> Result<Vec<Event>> {
    let mut statement = db.prepare(
        "SELECT id,kind,
            CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,
            CASE WHEN length(CAST(payload AS BLOB))<=?2 THEN payload END
         FROM events
         WHERE task_id=?1 AND kind IN (
            'reasoning_context','policy','tool_reservation','finite_search',
            'model_dispatch','cycle_reservation','subtask','reasoning_boundary')
         ORDER BY id LIMIT ?3",
    )?;
    let rows = statement.query_map(
        rusqlite::params![task, MAX_EVENT_BYTES, MAX_EVENTS + 1],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    let mut events = Vec::new();
    let mut bytes = 0usize;
    for row in rows {
        let (id, kind, source, raw) = row?;
        if events.len() >= MAX_EVENTS {
            bail!("Finite-search recovery history exceeds its event bound")
        }
        let source =
            source.ok_or_else(|| anyhow::anyhow!("Finite-search event source exceeds bounds"))?;
        let raw =
            raw.ok_or_else(|| anyhow::anyhow!("Finite-search event payload exceeds bounds"))?;
        bytes = bytes
            .checked_add(source.len())
            .and_then(|sum| sum.checked_add(raw.len()))
            .ok_or_else(|| anyhow::anyhow!("Finite-search event history size overflow"))?;
        if bytes > MAX_HISTORY_BYTES {
            bail!("Finite-search recovery history exceeds its byte bound")
        }
        events.push(Event {
            id,
            kind,
            source,
            payload: crate::strict_json::value(raw.as_bytes(), MAX_EVENT_BYTES)?,
        });
    }
    Ok(events)
}

/// Validate the original search receipt and its authorization/charge chain.
/// This function only inspects durable metadata. It never reconstructs a search
/// tree, repeats a tool call, or returns proof of satisfiability.
pub(super) fn validate(
    db: &rusqlite::Connection,
    task: &str,
    query: &str,
    cfg: &Config,
    reasoning_context_id: i64,
    boundary_id: Option<i64>,
) -> Result<Option<Receipt>> {
    let events = read_events(db, task)?;
    let contexts = events
        .iter()
        .filter(|event| event.kind == "reasoning_context")
        .collect::<Vec<_>>();
    let original_context = contexts
        .first()
        .ok_or_else(|| anyhow::anyhow!("Original reasoning context is missing"))?;
    if original_context.id != reasoning_context_id || original_context.source != "jev-r" {
        bail!("Finite-search recovery is not anchored to the original reasoning context")
    }

    let criterion = Criterion::from_query(query)?;
    let finite_objective = criterion
        .as_ref()
        .is_some_and(|criterion| criterion.name() == "finite_domain/v1");
    let expected = cfg.finite_search.is_some() && finite_objective;
    let solve_policies = events
        .iter()
        .filter(|event| {
            event.kind == "policy" && event.payload["capability"] == "constraints.solve"
        })
        .collect::<Vec<_>>();
    let solve_charges = events
        .iter()
        .filter(|event| {
            event.kind == "tool_reservation" && event.payload["capability"] == "constraints.solve"
        })
        .collect::<Vec<_>>();
    let receipts = events
        .iter()
        .filter(|event| event.kind == "finite_search")
        .collect::<Vec<_>>();

    if !expected {
        if !solve_policies.is_empty() || !solve_charges.is_empty() || !receipts.is_empty() {
            bail!("Unexpected finite-search authorization or receipt")
        }
        return Ok(None);
    }

    // Search enabled for a finite objective but no completed receipt includes
    // skipped, denied, interrupted, and non-limit outcomes. None can be resumed
    // by pretending the phase either ran or did not run.
    if solve_policies.len() != 1 || solve_charges.len() != 1 || receipts.len() != 1 {
        bail!("Enabled finite-search checkpoint lacks one complete authorization, charge, and receipt")
    }
    let policy = solve_policies[0];
    let charge = solve_charges[0];
    let receipt_event = receipts[0];
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
        || policy.id >= charge.id
        || original_context.id >= policy.id
    {
        bail!("Finite-search authorization or charge record is malformed or out of order")
    }

    let prior_tool_charges = events
        .iter()
        .filter(|event| event.kind == "tool_reservation" && event.id <= charge.id)
        .count() as u64;
    if charge.payload["invocation"].as_u64() != Some(prior_tool_charges)
        || prior_tool_charges > (cfg.max_tool_calls as u64).saturating_sub(1)
    {
        bail!("Finite-search charge does not match invocation order or preserved checker budget")
    }

    let settings = cfg
        .finite_search
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Finite-search settings are missing"))?;
    settings.validate()?;
    let objective_hash = sha256(query.as_bytes());
    let max_nodes = settings.max_nodes as u64;
    let expected_receipt = json!({
        "format_version": 1,
        "criterion": "finite_domain/v1",
        "objective_sha256": objective_hash,
        "max_nodes": max_nodes,
        "explored_nodes": receipt_event.payload["explored_nodes"],
        "outcome": "limit_reached",
        "verified": false,
        "permissions_granted": false
    });
    if receipt_event.source != "jev-r"
        || !exact_fields(
            &receipt_event.payload,
            &[
                "format_version",
                "criterion",
                "objective_sha256",
                "max_nodes",
                "explored_nodes",
                "outcome",
                "verified",
                "permissions_granted",
            ],
        )
        || receipt_event.payload != expected_receipt
        || receipt_event.payload["explored_nodes"].as_u64() != Some(max_nodes)
        || charge.id >= receipt_event.id
        || original_context.id >= receipt_event.id
    {
        bail!("Finite-search receipt is malformed, incomplete, or not a limit result")
    }

    let search_phase = events
        .iter()
        .filter(|event| original_context.id <= event.id && event.id <= receipt_event.id)
        .collect::<Vec<_>>();
    if search_phase.len() != 4
        || search_phase[0].kind != "reasoning_context"
        || search_phase[1].id != policy.id
        || search_phase[1].kind != "policy"
        || search_phase[2].id != charge.id
        || search_phase[2].kind != "tool_reservation"
        || search_phase[3].id != receipt_event.id
        || search_phase[3].kind != "finite_search"
    {
        bail!("Finite-search receipt is not preceded by the exact original context-policy-charge chain")
    }

    let first_following_execution = events
        .iter()
        .filter(|event| {
            event.id > original_context.id
                && matches!(
                    event.kind.as_str(),
                    "model_dispatch" | "cycle_reservation" | "subtask" | "reasoning_boundary"
                )
        })
        .map(|event| event.id)
        .min();
    if first_following_execution.is_some_and(|id| receipt_event.id >= id)
        || boundary_id.is_some_and(|id| receipt_event.id >= id)
    {
        bail!("Finite-search receipt follows resumed reasoning work or its boundary")
    }

    Ok(Some(Receipt {
        format_version: 1,
        event_id: receipt_event.id,
        charge_event_id: charge.id,
        objective_sha256: objective_hash,
        max_nodes,
        explored_nodes: max_nodes,
        outcome: "limit_reached".into(),
        automatic_replay: false,
        verified: false,
        permissions_granted: false,
    }))
}

/// Validate the original completed search only when the finite-domain task
/// enabled or entered that phase. Search-disabled legacy tasks keep their
/// prior saved-result behavior.
pub(super) fn validate_saved_history(
    db: &rusqlite::Connection,
    task: &str,
    query: &str,
) -> Result<Option<Receipt>> {
    let criterion = Criterion::from_query(query)?;
    if !criterion
        .as_ref()
        .is_some_and(|criterion| criterion.name() == "finite_domain/v1")
    {
        return Ok(None);
    }

    use rusqlite::OptionalExtension;
    let settings: Option<String> = db
        .query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 1",
            [task],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let cfg = match settings {
        Some(raw) => {
            let payload = crate::strict_json::value(raw.as_bytes(), 262144)?;
            let cfg: Config = serde_json::from_value(payload["configuration"].clone())?;
            cfg.validate()?;
            cfg
        }
        None => Config::default(),
    };
    let search_recorded: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND ((kind IN ('policy','tool_reservation') AND json_extract(payload,'$.capability')='constraints.solve') OR kind='finite_search'))",
        [task],
        |row| row.get(0),
    )?;
    if cfg.finite_search.is_none() && !search_recorded {
        return Ok(None);
    }
    let context_id: Option<i64> = db.query_row(
        "SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_context'",
        [task],
        |row| row.get(0),
    )?;
    let context_id =
        context_id.ok_or_else(|| anyhow::anyhow!("Original reasoning context is missing"))?;
    validate(db, task, query, &cfg, context_id, None)
}

/// Require a saved candidate to be the durable output of its original solver
/// call, following the validated search receipt. A present positive reasoning
/// cycle must also have its own preceding reservation; cycle zero is valid for
/// advisory solver results produced before whole-goal cycle scheduling.
pub(super) fn validate_saved_solver(
    db: &rusqlite::Connection,
    task: &str,
    saved: &Value,
    audit: &Value,
    receipt: &Receipt,
) -> Result<()> {
    let node: crate::reasoning::Node = serde_json::from_value(saved["node"].clone())?;
    let call = saved["call"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Saved solver result has no call identity"))?;
    if node.role != "solver"
        || node.cycle > 16
        || audit["calls"].as_array().is_none_or(|calls| {
            !calls.iter().any(|entry| {
                entry["call"].as_u64() == Some(call)
                    && entry["role"] == "solver"
                    && entry["state"] == "result_recorded"
            })
        })
    {
        bail!("Saved result has no ledger-backed solver call identity")
    }
    let (solver_id, source, raw): (i64, String, String) = db.query_row(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')=?2 AND json_extract(payload,'$.node.role')='solver' ORDER BY id DESC LIMIT 1",
        rusqlite::params![task, call],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let recorded = crate::strict_json::value(raw.as_bytes(), 262144)?;
    if source != "jev-r" || recorded != *saved || solver_id <= receipt.event_id {
        bail!("Saved solver result is not the recorded post-search result")
    }
    if node.cycle > 0 {
        let cycle_recorded: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='cycle_reservation' AND json_extract(payload,'$.cycle')=?2 AND id<?3)",
            rusqlite::params![task, node.cycle, solver_id],
            |row| row.get(0),
        )?;
        if !cycle_recorded {
            bail!("Saved solver cycle has no preceding durable reservation")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TASK: &str = "00000000-0000-4000-8000-000000000001";
    const QUERY: &str = "csp:{\"variables\":[{\"id\":\"x\",\"domain\":[0]}],\"constraints\":[{\"kind\":\"linear\",\"terms\":[{\"variable\":\"x\",\"coefficient\":1}],\"relation\":\"eq\",\"rhs\":0}]}";

    fn database() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE events (
                id INTEGER PRIMARY KEY,
                task_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                source TEXT NOT NULL,
                payload TEXT NOT NULL
            );",
        )
        .unwrap();
        db
    }

    fn event(db: &rusqlite::Connection, kind: &str, source: &str, payload: &str) -> i64 {
        db.execute(
            "INSERT INTO events(task_id,kind,source,payload) VALUES(?1,?2,?3,?4)",
            rusqlite::params![TASK, kind, source, payload],
        )
        .unwrap();
        db.last_insert_rowid()
    }

    fn finite_config(max_nodes: usize) -> Config {
        Config {
            finite_search: Some(crate::reasoning::finite_search::Settings { max_nodes }),
            ..Config::default()
        }
    }

    fn valid_history(db: &rusqlite::Connection) -> (i64, i64) {
        let context_id = event(db, "reasoning_context", "jev-r", "{}");
        event(
            db,
            "policy",
            "security-kernel",
            &json!({
                "task_id": TASK,
                "actor": "jev-plus",
                "capability": "constraints.solve",
                "allowed": true,
                "reason": null
            })
            .to_string(),
        );
        let charge_id = event(
            db,
            "tool_reservation",
            "security-kernel",
            &json!({
                "format_version": 1,
                "invocation": 1,
                "actor": "jev-plus",
                "capability": "constraints.solve"
            })
            .to_string(),
        );
        event(
            db,
            "finite_search",
            "jev-r",
            &json!({
                "format_version": 1,
                "criterion": "finite_domain/v1",
                "objective_sha256": sha256(QUERY.as_bytes()),
                "max_nodes": 1,
                "explored_nodes": 1,
                "outcome": "limit_reached",
                "verified": false,
                "permissions_granted": false
            })
            .to_string(),
        );
        (context_id, charge_id)
    }

    #[test]
    fn accepts_only_a_completed_bounded_limit_receipt_and_marks_no_replay() {
        let db = database();
        let (context_id, charge_id) = valid_history(&db);
        let receipt = validate(&db, TASK, QUERY, &finite_config(1), context_id, None)
            .unwrap()
            .unwrap();
        assert_eq!(receipt.format_version, 1);
        assert!(receipt.event_id > receipt.charge_event_id);
        assert_eq!(receipt.charge_event_id, charge_id);
        assert_eq!(receipt.objective_sha256, sha256(QUERY.as_bytes()));
        assert_eq!(receipt.max_nodes, 1);
        assert_eq!(receipt.explored_nodes, 1);
        assert_eq!(receipt.outcome, "limit_reached");
        assert!(!receipt.automatic_replay);
        assert!(!receipt.verified);
        assert!(!receipt.permissions_granted);
    }

    #[test]
    fn rejects_receipt_that_spends_the_reserved_checker_charge() {
        let db = database();
        let (context_id, _) = valid_history(&db);
        let mut cfg = finite_config(1);
        cfg.max_tool_calls = 1;
        assert!(validate(&db, TASK, QUERY, &cfg, context_id, None).is_err());
    }

    #[test]
    fn rejects_configuration_or_receipt_cap_mismatch() {
        let db = database();
        let (context_id, _) = valid_history(&db);
        assert!(validate(&db, TASK, QUERY, &finite_config(2), context_id, None).is_err());
    }

    #[test]
    fn refuses_enabled_search_without_a_completed_receipt_including_skips() {
        let db = database();
        let context_id = event(&db, "reasoning_context", "jev-r", "{}");
        assert!(validate(&db, TASK, QUERY, &finite_config(1), context_id, None).is_err());
    }

    #[test]
    fn accepts_disabled_tasks_without_search_history_and_rejects_unexpected_history() {
        let db = database();
        let context_id = event(&db, "reasoning_context", "jev-r", "{}");
        assert!(
            validate(&db, TASK, QUERY, &Config::default(), context_id, None)
                .unwrap()
                .is_none()
        );
        valid_history(&db);
        assert!(validate(&db, TASK, QUERY, &Config::default(), context_id, None).is_err());
    }

    #[test]
    fn rejects_duplicate_json_keys_and_unknown_receipt_fields() {
        let db = database();
        let context_id = event(&db, "reasoning_context", "jev-r", "{}");
        event(
            &db,
            "policy",
            "security-kernel",
            &json!({
                "task_id": TASK,
                "actor": "jev-plus",
                "capability": "constraints.solve",
                "allowed": true,
                "reason": null
            })
            .to_string(),
        );
        event(
            &db,
            "tool_reservation",
            "security-kernel",
            &json!({
                "format_version": 1,
                "invocation": 1,
                "actor": "jev-plus",
                "capability": "constraints.solve"
            })
            .to_string(),
        );
        let raw = format!(
            "{{\"format_version\":1,\"criterion\":\"finite_domain/v1\",\"objective_sha256\":\"{}\",\"max_nodes\":1,\"explored_nodes\":1,\"outcome\":\"limit_reached\",\"verified\":false,\"permissions_granted\":false,\"extra\":true}}",
            sha256(QUERY.as_bytes())
        );
        event(&db, "finite_search", "jev-r", &raw);
        assert!(validate(&db, TASK, QUERY, &finite_config(1), context_id, None).is_err());

        let db = database();
        let context_id = event(&db, "reasoning_context", "jev-r", "{}");
        event(
            &db,
            "policy",
            "security-kernel",
            &json!({
                "task_id": TASK,
                "actor": "jev-plus",
                "capability": "constraints.solve",
                "allowed": true,
                "reason": null
            })
            .to_string(),
        );
        event(
            &db,
            "tool_reservation",
            "security-kernel",
            &json!({
                "format_version": 1,
                "invocation": 1,
                "actor": "jev-plus",
                "capability": "constraints.solve"
            })
            .to_string(),
        );
        let duplicated = format!(
            "{{\"format_version\":1,\"criterion\":\"finite_domain/v1\",\"objective_sha256\":\"{}\",\"max_nodes\":1,\"explored_nodes\":1,\"outcome\":\"limit_reached\",\"verified\":false,\"verified\":true,\"permissions_granted\":false}}",
            sha256(QUERY.as_bytes())
        );
        event(&db, "finite_search", "jev-r", &duplicated);
        assert!(validate(&db, TASK, QUERY, &finite_config(1), context_id, None).is_err());
    }

    #[test]
    fn rejects_excessive_selected_event_history() {
        let db = database();
        for _ in 0..=MAX_EVENTS {
            event(&db, "policy", "security-kernel", "{}");
        }
        assert!(read_events(&db, TASK).is_err());
    }

    #[test]
    fn rejects_excessive_selected_event_bytes() {
        let db = database();
        let padding = "x".repeat(250_000);
        let payload = json!({"padding": padding}).to_string();
        for _ in 0..33 {
            event(&db, "policy", "security-kernel", &payload);
        }
        assert!(read_events(&db, TASK).is_err());
    }
}
