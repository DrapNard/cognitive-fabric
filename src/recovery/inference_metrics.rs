//! Attach bounded provider-reported measurements to the public inference audit.
//!
//! These observations are descriptive only. They do not change audit authority,
//! task replay, verification status, or resource admission.

use crate::{backend::ProviderMetrics, store::sha256};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Instant};

const MAX_REASONING_ROWS: usize = 129;
const MAX_METRIC_ROWS: usize = 64;
const MAX_REASONING_BYTES: usize = 262_144;
const MAX_METRIC_BYTES: usize = 4_096;

#[derive(Clone)]
struct ResultRecord {
    event_id: i64,
    role: String,
    answer_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetricEvent {
    format_version: u16,
    call: u64,
    role: String,
    answer_sha256: String,
    provider_reported: bool,
    physical_resource_measurement: bool,
    result_verified: bool,
    metrics: ProviderMetrics,
}

/// Attach validated provider metrics to the public `Store::inference_audit`
/// report. Recovery continues to consume `audit_from` directly and therefore
/// does not treat these observations as continuation evidence or authority.
pub(super) fn attach(
    db: &rusqlite::Connection,
    task: &str,
    report: &mut Value,
    deadline: Instant,
) -> Result<()> {
    if Instant::now() >= deadline {
        bail!("Inference metrics audit deadline exhausted")
    }
    // The public Store method owns one progress handler for the whole audit;
    // replacing or clearing it here would weaken that shared deadline.
    attach_inner(db, task, report, deadline)
}

fn attach_inner(
    db: &rusqlite::Connection,
    task: &str,
    report: &mut Value,
    deadline: Instant,
) -> Result<()> {
    let reasoning = load_reasoning(db, task, deadline)?;
    let metrics = load_metrics(db, task, deadline)?;

    let reserved_calls = report["reserved_calls"]
        .as_u64()
        .context("Inference audit has invalid reservation count")?;
    let recorded_results = report["recorded_results"]
        .as_u64()
        .context("Inference audit has invalid recorded-result count")?;
    let legacy_results = report["legacy_results_without_call_identity"]
        .as_u64()
        .context("Inference audit has invalid legacy-result count")?;
    let calls = report
        .get_mut("calls")
        .and_then(Value::as_array_mut)
        .context("Inference audit is missing its call list")?;
    if calls.len() as u64 != reserved_calls {
        bail!("Inference audit call count does not match its reservations")
    }
    if reasoning.legacy_results != legacy_results {
        bail!("Inference result history disagrees with its public audit")
    }
    if reasoning.results.len() as u64 != recorded_results {
        bail!("Inference result history disagrees with its recorded-result count")
    }

    let mut call_roles = BTreeMap::new();
    for call in calls.iter_mut() {
        let call_id = call["call"]
            .as_u64()
            .context("Inference audit call is missing its identity")?;
        let role = call["role"]
            .as_str()
            .context("Inference audit call is missing its role")?;
        if call_roles.insert(call_id, role.to_owned()).is_some() {
            bail!("Inference audit contains duplicate call identities")
        }
        call["provider_metrics"] = Value::Null;
    }
    if call_roles.len() as u64 != reserved_calls {
        bail!("Inference audit call identities are incomplete")
    }
    for (call_id, role) in &call_roles {
        let call = calls
            .iter()
            .find(|call| call["call"].as_u64() == Some(*call_id))
            .context("Inference audit call disappeared")?;
        if call["state"] == "result_recorded" {
            let result = reasoning
                .results
                .get(call_id)
                .context("Inference audit result is missing from durable history")?;
            if result.role != *role {
                bail!("Inference result role does not match its reservation")
            }
        } else if call["state"] != "result_not_recorded" {
            bail!("Inference audit contains an unknown call state")
        }
    }

    let mut attached = 0u64;
    for (call_id, (event_id, event)) in metrics {
        let result = reasoning
            .results
            .get(&call_id)
            .ok_or_else(|| anyhow!("Provider metrics have no recorded inference result"))?;
        if event_id <= result.event_id {
            bail!("Provider metrics precede their recorded inference result")
        }
        let reserved_role = call_roles
            .get(&call_id)
            .ok_or_else(|| anyhow!("Provider metrics have no reserved inference call"))?;
        if event.call != call_id
            || event.role.as_str() != result.role.as_str()
            || event.role.as_str() != reserved_role.as_str()
            || event.answer_sha256.as_str() != result.answer_sha256.as_str()
        {
            bail!("Provider metrics do not match their inference result")
        }
        let call = calls
            .iter_mut()
            .find(|call| call["call"].as_u64() == Some(call_id))
            .context("Provider metrics call is absent from the inference audit")?;
        if call["state"] != "result_recorded" || !call["provider_metrics"].is_null() {
            bail!("Provider metrics call is not a unique recorded result")
        }
        call["provider_metrics"] = serde_json::to_value(event.metrics)?;
        attached = attached
            .checked_add(1)
            .ok_or_else(|| anyhow!("Provider metrics count overflow"))?;
        check_deadline(deadline)?;
    }

    let complete = reserved_calls > 0
        && recorded_results == reserved_calls
        && attached == reserved_calls
        && legacy_results == 0;
    report["provider_metrics_coverage"] = json!({
        "reported_calls": attached,
        "recorded_results": recorded_results,
        "reserved_calls": reserved_calls,
        "complete": complete,
        "provider_metrics_independently_verified": false,
        "physical_resource_measurement": false,
    });
    Ok(())
}

struct ReasoningHistory {
    results: BTreeMap<u64, ResultRecord>,
    legacy_results: u64,
}

fn load_reasoning(
    db: &rusqlite::Connection,
    task: &str,
    deadline: Instant,
) -> Result<ReasoningHistory> {
    let mut stmt = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' ORDER BY id LIMIT 130",
    )?;
    let mut rows = stmt.query([task])?;
    let mut results = BTreeMap::new();
    let mut legacy_results = 0u64;
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        check_deadline(deadline)?;
        count += 1;
        if count > MAX_REASONING_ROWS {
            bail!("Inference metrics audit exceeds bounded reasoning history")
        }
        let event_id: i64 = row.get(0)?;
        let source: Option<String> = row.get(1)?;
        let raw: Option<String> = row.get(2)?;
        if source.as_deref() != Some("jev-r") {
            bail!("Unexpected inference result audit source")
        }
        let raw = raw.context("Inference result audit exceeds its byte limit")?;
        let payload = crate::strict_json::value(raw.as_bytes(), MAX_REASONING_BYTES)
            .context("Malformed inference result audit")?;
        let Some(call) = payload.get("call").and_then(Value::as_u64) else {
            if payload.get("call").is_some_and(|value| !value.is_null()) {
                bail!("Invalid inference result call identity")
            }
            legacy_results += 1;
            continue;
        };
        if call == 0 || call > 64 {
            bail!("Inference result call identity exceeds bounds")
        }
        let role = payload["node"]["role"]
            .as_str()
            .filter(|role| role.len() <= 64)
            .context("Inference result role is invalid")?
            .to_owned();
        let text = payload["text"]
            .as_str()
            .filter(|text| !text.is_empty() && text.len() <= 96_000)
            .context("Inference result text is invalid")?;
        let hash = payload["node"]["content_hash"]
            .as_str()
            .context("Inference result hash is missing")?;
        if hash != sha256(text.as_bytes())
            || results
                .insert(
                    call,
                    ResultRecord {
                        event_id,
                        role,
                        answer_sha256: hash.to_owned(),
                    },
                )
                .is_some()
        {
            bail!("Duplicate or tampered inference result history")
        }
    }
    check_deadline(deadline)?;
    Ok(ReasoningHistory {
        results,
        legacy_results,
    })
}

fn load_metrics(
    db: &rusqlite::Connection,
    task: &str,
    deadline: Instant,
) -> Result<BTreeMap<u64, (i64, MetricEvent)>> {
    let mut stmt = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind='inference_provider_metrics' ORDER BY id LIMIT 65",
    )?;
    let mut rows = stmt.query([task])?;
    let mut events = BTreeMap::new();
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        check_deadline(deadline)?;
        count += 1;
        if count > MAX_METRIC_ROWS {
            bail!("Inference metrics audit exceeds bounded provider history")
        }
        let event_id: i64 = row.get(0)?;
        let source: Option<String> = row.get(1)?;
        let raw: Option<String> = row.get(2)?;
        if source.as_deref() != Some("runtime") {
            bail!("Unexpected provider metrics audit source")
        }
        let raw = raw.context("Provider metrics audit exceeds its byte limit")?;
        let event: MetricEvent = serde_json::from_value(
            crate::strict_json::value(raw.as_bytes(), MAX_METRIC_BYTES)
                .context("Malformed provider metrics audit")?,
        )
        .context("Invalid provider metrics audit schema")?;
        event.metrics.validate()?;
        if event.format_version != 1
            || event.call == 0
            || event.call > 64
            || event.role.is_empty()
            || event.role.len() > 64
            || event.answer_sha256.len() != 64
            || !event
                .answer_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !event.provider_reported
            || event.physical_resource_measurement
            || event.result_verified
            || events.insert(event.call, (event_id, event)).is_some()
        {
            bail!("Invalid, duplicate, or untrusted provider metrics event")
        }
    }
    check_deadline(deadline)?;
    Ok(events)
}

fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        bail!("Inference metrics audit deadline exhausted")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{ProviderMetricSource, ProviderMetrics};
    use rusqlite::params;
    use std::time::Duration;

    fn db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE events(
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

    fn metrics() -> ProviderMetrics {
        let mut value = ProviderMetrics::new(ProviderMetricSource::LmStudioNative);
        value.time_to_first_token_ms = Some(14.5);
        value
    }

    fn reasoning(call: u64, text: &str) -> Value {
        json!({
            "call": call,
            "node": {
                "role": "solver",
                "content_hash": sha256(text.as_bytes())
            },
            "text": text
        })
    }

    fn metric(call: u64, text: &str) -> Value {
        json!({
            "format_version": 1,
            "call": call,
            "role": "solver",
            "answer_sha256": sha256(text.as_bytes()),
            "provider_reported": true,
            "physical_resource_measurement": false,
            "result_verified": false,
            "metrics": metrics()
        })
    }

    fn insert(db: &rusqlite::Connection, kind: &str, source: &str, payload: &str) {
        db.execute(
            "INSERT INTO events(task_id,kind,source,payload) VALUES('task',?1,?2,?3)",
            params![kind, source, payload],
        )
        .unwrap();
    }

    fn report() -> Value {
        json!({
            "reserved_calls": 1,
            "recorded_results": 1,
            "legacy_results_without_call_identity": 0,
            "calls": [{
                "call": 1,
                "role": "solver",
                "state": "result_recorded"
            }]
        })
    }

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(2)
    }

    #[test]
    fn attaches_linked_provider_observations_without_authority() {
        let db = db();
        insert(
            &db,
            "reasoning",
            "jev-r",
            &reasoning(1, "answer").to_string(),
        );
        insert(
            &db,
            "inference_provider_metrics",
            "runtime",
            &metric(1, "answer").to_string(),
        );
        let mut audit = report();
        attach(&db, "task", &mut audit, deadline()).unwrap();
        assert_eq!(
            audit["calls"][0]["provider_metrics"]["time_to_first_token_ms"],
            14.5
        );
        assert_eq!(
            audit["provider_metrics_coverage"],
            json!({
                "reported_calls": 1,
                "recorded_results": 1,
                "reserved_calls": 1,
                "complete": true,
                "provider_metrics_independently_verified": false,
                "physical_resource_measurement": false
            })
        );
        assert_eq!(audit["calls"][0]["state"], "result_recorded");
    }

    #[test]
    fn absent_provider_metrics_are_null_and_incomplete() {
        let db = db();
        insert(
            &db,
            "reasoning",
            "jev-r",
            &reasoning(1, "answer").to_string(),
        );
        let mut audit = report();
        attach(&db, "task", &mut audit, deadline()).unwrap();
        assert!(audit["calls"][0]["provider_metrics"].is_null());
        assert_eq!(audit["provider_metrics_coverage"]["reported_calls"], 0);
        assert_eq!(audit["provider_metrics_coverage"]["complete"], false);
    }

    #[test]
    fn rejects_duplicate_or_orphan_metrics_and_early_or_malformed_events() {
        let valid_result = reasoning(1, "answer").to_string();
        let valid_metric = metric(1, "answer").to_string();
        let variants = [
            // Duplicate call event.
            vec![
                ("reasoning", "jev-r", valid_result.clone()),
                (
                    "inference_provider_metrics",
                    "runtime",
                    valid_metric.clone(),
                ),
                (
                    "inference_provider_metrics",
                    "runtime",
                    valid_metric.clone(),
                ),
            ],
            // Metrics cannot be attached before the result event exists.
            vec![
                (
                    "inference_provider_metrics",
                    "runtime",
                    valid_metric.clone(),
                ),
                ("reasoning", "jev-r", valid_result.clone()),
            ],
            // A different source cannot publish trusted runtime metrics.
            vec![
                ("reasoning", "jev-r", valid_result.clone()),
                (
                    "inference_provider_metrics",
                    "provider",
                    valid_metric.clone(),
                ),
            ],
            // Duplicate JSON keys are rejected instead of last-value-wins.
            vec![
                ("reasoning", "jev-r", valid_result.clone()),
                (
                    "inference_provider_metrics",
                    "runtime",
                    r#"{"format_version":1,"call":1,"call":1}"#.into(),
                ),
            ],
            // Unknown fields are refused by the typed event schema.
            vec![
                ("reasoning", "jev-r", valid_result.clone()),
                (
                    "inference_provider_metrics",
                    "runtime",
                    r#"{"format_version":1,"call":1,"unknown":true}"#.into(),
                ),
            ],
            // Unknown source strings and non-finite JSON numbers are refused.
            vec![
                ("reasoning", "jev-r", valid_result.clone()),
                (
                    "inference_provider_metrics",
                    "runtime",
                    valid_metric.replace("lm_studio_native", "unrecognized_backend"),
                ),
            ],
            vec![
                ("reasoning", "jev-r", valid_result.clone()),
                (
                    "inference_provider_metrics",
                    "runtime",
                    valid_metric.replace("14.5", "1e400"),
                ),
            ],
            // Unmatched call identity is an orphan.
            vec![
                ("reasoning", "jev-r", valid_result.clone()),
                (
                    "inference_provider_metrics",
                    "runtime",
                    metric(2, "answer").to_string(),
                ),
            ],
            // Content hash does not identify the recorded answer.
            vec![
                ("reasoning", "jev-r", valid_result.clone()),
                (
                    "inference_provider_metrics",
                    "runtime",
                    metric(1, "different answer").to_string(),
                ),
            ],
        ];

        for rows in variants {
            let db = db();
            for (kind, source, payload) in rows {
                insert(&db, kind, source, &payload);
            }
            assert!(attach(&db, "task", &mut report(), deadline()).is_err());
        }
    }

    #[test]
    fn provider_history_and_payload_sizes_are_bounded() {
        let db = db();
        for call in 1..=64 {
            insert(
                &db,
                "inference_provider_metrics",
                "runtime",
                &metric(call, "answer").to_string(),
            );
        }
        assert_eq!(load_metrics(&db, "task", deadline()).unwrap().len(), 64);
        insert(
            &db,
            "inference_provider_metrics",
            "runtime",
            &metric(64, "answer").to_string(),
        );
        assert!(load_metrics(&db, "task", deadline())
            .err()
            .unwrap()
            .to_string()
            .contains("bounded provider history"));
        let db = self::db();
        let oversized = format!("{}{}", metric(1, "answer"), " ".repeat(4096));
        insert(&db, "inference_provider_metrics", "runtime", &oversized);
        assert!(load_metrics(&db, "task", deadline()).is_err());
    }

    #[test]
    fn unrecorded_calls_and_legacy_results_keep_coverage_incomplete() {
        let db = db();
        insert(
            &db,
            "reasoning",
            "jev-r",
            &reasoning(1, "answer").to_string(),
        );
        insert(&db, "reasoning", "jev-r", r#"{"text":"legacy"}"#);
        insert(
            &db,
            "inference_provider_metrics",
            "runtime",
            &metric(1, "answer").to_string(),
        );
        let mut report = report();
        report["reserved_calls"] = json!(2);
        report["legacy_results_without_call_identity"] = json!(1);
        report["calls"]
            .as_array_mut()
            .unwrap()
            .push(json!({"call":2,"role":"solver","state":"result_not_recorded"}));
        attach(&db, "task", &mut report, deadline()).unwrap();
        assert_eq!(report["provider_metrics_coverage"]["reported_calls"], 1);
        assert_eq!(report["provider_metrics_coverage"]["complete"], false);
        assert!(report["calls"][1]["provider_metrics"].is_null());
    }

    #[test]
    fn rejects_expired_deadline() {
        let db = db();
        assert!(attach(
            &db,
            "task",
            &mut report(),
            Instant::now() - Duration::from_millis(1)
        )
        .is_err());
    }
}
