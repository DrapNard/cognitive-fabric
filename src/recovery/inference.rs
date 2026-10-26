//! Read-only reconstruction of durable inference reservations, never replay authority.
use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
#[path = "inference_metrics.rs"]
mod metrics;
impl Store {
    pub fn inference_audit(&self, task: &str) -> Result<Value> {
        security::authorize("human-cli", "task.observe")?;
        uuid::Uuid::parse_str(task)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut db = self.lock()?;
        if Instant::now() >= deadline {
            bail!("Inference audit deadline exhausted")
        }
        db.progress_handler(1000, Some(move || Instant::now() >= deadline));
        let result = (|| -> Result<Value> {
            let tx = db.transaction()?;
            let mut report = audit_from(&tx, task)?;
            metrics::attach(&tx, task, &mut report, deadline)?;
            if Instant::now() >= deadline {
                bail!("Inference audit deadline exhausted")
            }
            tx.commit()?;
            Ok(report)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        result
    }
}

pub(super) fn audit_from(db: &rusqlite::Connection, task: &str) -> Result<Value> {
    let status: String = db.query_row(
        "SELECT CASE WHEN length(status)<=64 THEN status END FROM tasks WHERE id=?1",
        [task],
        |r| r.get(0),
    )?;
    let mut calls = BTreeMap::<u64, Value>::new();
    let mut legacy = 0usize;
    let mut settings: Option<(crate::config::Config, u64, u64, bool)> = None;
    let mut execution_clock = None;
    let mut language_boundary = None;
    let mut presentation_dispatched = false;
    let mut presentation_recorded = false;
    let mut presentation_completed = false;
    let mut language_recovery_check = false;
    let mut result_hashes = BTreeMap::<u64, String>::new();
    let mut tools = 0u64;
    let mut cycles = 0u64;
    {
        let mut stmt=db.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('execution_settings','model_dispatch','reasoning','tool_reservation','cycle_reservation','language_boundary_reservation','language_boundary_presentation') ORDER BY id LIMIT 212")?;
        let mut rows = stmt.query([task])?;
        let mut count = 0;
        let mut inference_count = 0;
        while let Some(row) = rows.next()? {
            count += 1;
            if count > 211 {
                bail!("Inference audit exceeds bounded call history")
            }
            let kind: String = row.get(0)?;
            let source: String = row.get(1)?;
            let raw: String = row.get(2)?;
            let payload: Value = serde_json::from_str(&raw)?;
            if kind == "execution_settings" {
                if source != "runtime"
                    || settings.is_some()
                    || !calls.is_empty()
                    || legacy != 0
                    || payload["format_version"] != 1
                {
                    bail!("Invalid execution settings history")
                }
                let cfg: crate::config::Config =
                    serde_json::from_value(payload["configuration"].clone())?;
                cfg.validate()?;
                let mode = payload["mode"].as_str().unwrap_or("");
                let (calls, cycles) = match mode {
                    "fast" => (1, 1),
                    "standard" => (4, 1),
                    "deep" => (10, 3),
                    "research" => (18, 4),
                    _ => bail!("Invalid saved reasoning mode"),
                };
                let task_mode: String = db.query_row(
                    "SELECT CASE WHEN length(mode)<=32 THEN mode END FROM tasks WHERE id=?1",
                    [task],
                    |r| r.get(0),
                )?;
                if mode != task_mode
                    || payload["call_limit"].as_u64() != Some(calls.min(cfg.max_calls as u64))
                    || payload["cycle_limit"].as_u64() != Some(cycles.min(cfg.max_cycles as u64))
                {
                    bail!("Invalid saved execution ceilings")
                }
                let durable = match payload.get("resource_reservations_version") {
                    None => false,
                    Some(v) if *v == 1 => true,
                    _ => bail!("Unsupported resource reservation version"),
                };
                if let Some(clock) = payload.get("execution_clock").filter(|v| !v.is_null()) {
                    let clock: crate::execution_clock::Snapshot =
                        serde_json::from_value(clock.clone())?;
                    clock.validate(cfg.max_wall_seconds)?;
                    execution_clock = Some(clock);
                }
                settings = Some((
                    cfg,
                    payload["call_limit"].as_u64().unwrap(),
                    payload["cycle_limit"].as_u64().unwrap(),
                    durable,
                ));
                continue;
            }
            if kind == "language_boundary_reservation" {
                let Some((cfg, limit, _, true)) = settings.as_mut() else {
                    bail!("Language reservation has no durable execution settings")
                };
                if source != "jev-plus"
                    || language_boundary.is_some()
                    || !calls.is_empty()
                    || tools != 0
                    || cycles != 0
                    || payload
                        != crate::reasoning::language_reservation_payload(cfg, *limit as usize)?
                {
                    bail!("Invalid or repeated language boundary reservation")
                }
                *limit = payload["root_call_limit"].as_u64().unwrap();
                language_boundary = Some(payload);
                continue;
            }
            if kind == "language_boundary_presentation" {
                let call = calls.len() as u64;
                let result = calls
                    .get(&call)
                    .ok_or_else(|| anyhow::anyhow!("Presentation has no dispatch"))?;
                let status = payload["status"].as_str().unwrap_or("");
                let original = payload["original_answer_sha256"].as_str().unwrap_or("");
                if source != "interface"
                    || language_boundary.is_none()
                    || presentation_recorded
                    || !presentation_dispatched
                    || result["role"] != "synthesizer"
                    || result["state"] != "result_recorded"
                    || payload.as_object().is_none_or(|object| object.len() != 5)
                    || result_hashes.get(&call).map(String::as_str)
                        != payload["answer_sha256"].as_str()
                    || original.len() != 64
                    || !original.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || !matches!(status, "completed" | "unverified" | "rejected")
                    || payload["verified_bytes_preserved"] != (status == "completed")
                    || payload["new_claims_verified"] != false
                    || (status == "completed"
                        && payload["answer_sha256"] != payload["original_answer_sha256"])
                {
                    bail!("Invalid language presentation history")
                }
                presentation_recorded = true;
                presentation_completed = status == "completed";
                continue;
            }
            if matches!(kind.as_str(), "tool_reservation" | "cycle_reservation") {
                if presentation_dispatched
                    && !(presentation_recorded
                        && presentation_completed
                        && !language_recovery_check
                        && kind == "tool_reservation"
                        && payload["origin"] == "operator_language_result_recovery"
                        && matches!(
                            payload["capability"].as_str(),
                            Some(
                                "ordering.verify"
                                    | "sorting.verify"
                                    | "path.verify"
                                    | "constraints.verify"
                            )
                        ))
                {
                    bail!("Resource work followed terminal language presentation")
                }
                if presentation_dispatched {
                    let query: String = db.query_row(
                        "SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END FROM tasks WHERE id=?1",
                        [task], |row| row.get(0))?;
                    let criterion = crate::verification::Criterion::from_query(&query)?
                        .ok_or_else(|| {
                            anyhow::anyhow!("Language result recovery has no compiled criterion")
                        })?;
                    if payload["capability"] != criterion.capability() {
                        bail!("Language recovery charged a different compiled checker")
                    }
                    language_recovery_check = true;
                }
                let Some((cfg, _, cycle_limit, true)) = settings.as_ref() else {
                    bail!("Resource reservation has no compatible settings")
                };
                if payload["format_version"] != 1 {
                    bail!("Unsupported resource reservation")
                }
                if kind == "tool_reservation" {
                    if source != "security-kernel"
                        || payload["actor"] != "jev-plus"
                        || payload["invocation"].as_u64() != Some(tools + 1)
                        || tools >= cfg.max_tool_calls as u64
                        || !matches!(
                            payload["capability"].as_str(),
                            Some(
                                "knowledge.search"
                                    | "math.evaluate"
                                    | "sql.query"
                                    | "ordering.verify"
                                    | "sorting.verify"
                                    | "path.verify"
                                    | "constraints.verify"
                                    | "constraints.solve"
                            )
                        )
                    {
                        bail!("Invalid durable tool reservation")
                    }
                    tools += 1;
                } else {
                    if source != "jev-r"
                        || payload["cycle"].as_u64() != Some(cycles + 1)
                        || cycles >= *cycle_limit
                    {
                        bail!("Invalid durable cycle reservation")
                    }
                    cycles += 1;
                }
                continue;
            }
            inference_count += 1;
            if inference_count > 128 {
                bail!("Inference audit exceeds bounded call history")
            }
            if source != "jev-r" {
                bail!("Unexpected inference audit source")
            }
            if kind == "model_dispatch" {
                let call = payload["call"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing inference call identity"))?;
                let role = payload["role"].as_str().unwrap_or("");
                let allocation = payload["allocation"].as_u64().unwrap_or(0);
                let prompt_bytes = payload["prompt_bytes"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing inference prompt charge"))?;
                if call != calls.len() as u64 + 1
                    || call > 64
                    || !(1..=16384).contains(&allocation)
                    || prompt_bytes > 32000
                    || !matches!(
                        role,
                        "interface"
                            | "synthesizer"
                            | "planner"
                            | "solver"
                            | "critic"
                            | "reflector"
                            | "explorer"
                    )
                {
                    bail!("Invalid inference reservation history")
                }
                if let Some(boundary) = &language_boundary {
                    let (cfg, limit, _, _) = settings.as_ref().unwrap();
                    let interpretation_ready = calls.get(&1).is_some_and(|entry| {
                        entry["role"] == "interface" && entry["state"] == "result_recorded"
                    });
                    if presentation_dispatched
                        || (call == 1
                            && (role != "interface" || allocation != cfg.interface_tokens as u64))
                        || (call > 1 && (!interpretation_ready || role == "interface"))
                    {
                        bail!("Language phases are out of order")
                    }
                    let presenting = role == "synthesizer";
                    let held_calls = u64::from(!presenting);
                    let held_output = if presenting {
                        0
                    } else {
                        boundary["presentation_output_tokens"].as_u64().unwrap()
                    };
                    let held_input = if presenting {
                        0
                    } else {
                        boundary["presentation_prompt_bytes"].as_u64().unwrap()
                    };
                    let used_output: u64 = calls
                        .values()
                        .map(|entry| entry["reserved_output_tokens"].as_u64().unwrap())
                        .sum();
                    let used_input: u64 = calls
                        .values()
                        .map(|entry| entry["prompt_bytes"].as_u64().unwrap())
                        .sum();
                    if call.saturating_add(held_calls) > *limit
                        || used_output
                            .saturating_add(allocation)
                            .saturating_add(held_output)
                            > cfg.max_tokens as u64
                        || used_input
                            .saturating_add(prompt_bytes)
                            .saturating_add(held_input)
                            > cfg.max_input_bytes as u64
                        || (presenting
                            && (allocation != cfg.interface_tokens as u64
                                || prompt_bytes
                                    > boundary["presentation_prompt_bytes"].as_u64().unwrap()))
                    {
                        bail!("Language phase exceeded its original reserved capacity")
                    }
                    presentation_dispatched = presenting;
                }
                calls.insert(call,json!({"call":call,"role":role,"reserved_output_tokens":allocation,"prompt_bytes":prompt_bytes,"state":"result_not_recorded","model":null,"input_tokens":null,"output_tokens":null}));
            } else {
                let Some(call) = payload["call"].as_u64() else {
                    if !payload["call"].is_null() {
                        bail!("Invalid inference result call identity")
                    }
                    legacy += 1;
                    continue;
                };
                let record = calls
                    .get_mut(&call)
                    .ok_or_else(|| anyhow::anyhow!("Inference result has no reservation"))?;
                let text = payload["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing recorded inference text"))?;
                let model = payload["node"]["model"].as_str().unwrap_or("");
                if record["state"] != "result_not_recorded"
                    || record["role"] != payload["node"]["role"]
                    || model.is_empty()
                    || model.len() > 256
                    || text.len() > 96000
                    || payload["node"]["content_hash"] != sha256(text.as_bytes())
                {
                    bail!("Invalid recorded inference result identity")
                }
                for field in ["input_tokens", "output_tokens"] {
                    if !payload[field].is_null() && payload[field].as_u64().is_none() {
                        bail!("Invalid inference usage metric")
                    }
                    record[field] = payload[field].clone();
                }
                if record["output_tokens"]
                    .as_u64()
                    .is_some_and(|n| n > record["reserved_output_tokens"].as_u64().unwrap())
                {
                    bail!("Recorded output exceeds reservation")
                }
                if language_boundary.is_some() {
                    settings
                        .as_ref()
                        .unwrap()
                        .0
                        .require_role_model(record["role"].as_str().unwrap(), model)?;
                }
                result_hashes.insert(call, sha256(text.as_bytes()));
                record["model"] = json!(model);
                record["state"] = json!("result_recorded");
            }
        }
    }
    let recorded = calls
        .values()
        .filter(|r| r["state"] == "result_recorded")
        .count();
    let complete = legacy == 0
        && !calls.is_empty()
        && recorded == calls.len()
        && calls
            .values()
            .all(|r| r["input_tokens"].as_u64().is_some() && r["output_tokens"].as_u64().is_some());
    let sum = |field: &str| -> Result<u64> {
        calls.values().try_fold(0u64, |total, r| {
            total
                .checked_add(r[field].as_u64().unwrap_or(0))
                .ok_or_else(|| anyhow::anyhow!("Inference audit metric overflow"))
        })
    };
    let remaining = if let Some((cfg, limit, cycle_limit, durable)) = settings {
        let used_calls = calls.len() as u64;
        let reserved = sum("reserved_output_tokens")?;
        let input = sum("prompt_bytes")?;
        if legacy != 0
            || used_calls > limit
            || reserved > cfg.max_tokens as u64
            || input > cfg.max_input_bytes as u64
        {
            bail!("Recorded inference exceeds saved execution limits or has uncorrelated results")
        }
        let holding = language_boundary.is_some() && !presentation_dispatched;
        let held_calls = u64::from(holding);
        let held_output = if holding {
            cfg.interface_tokens as u64
        } else {
            0
        };
        let held_input = if holding { 8192 } else { 0 };
        Some(
            json!({"calls":if presentation_dispatched {0} else {limit.saturating_sub(used_calls).saturating_sub(held_calls)},"reserved_output_tokens":if presentation_dispatched {0} else {(cfg.max_tokens as u64).saturating_sub(reserved).saturating_sub(held_output)},"prompt_bytes":if presentation_dispatched {0} else {(cfg.max_input_bytes as u64).saturating_sub(input).saturating_sub(held_input)},"wall_time":execution_clock.as_ref().map(|clock| clock.remaining()).transpose()?,"cycles":if durable {Some(cycle_limit-cycles)} else {None},"tool_calls":if durable {Some(cfg.max_tool_calls as u64-tools)} else {None},"resume_authorized":false}),
        )
    } else {
        None
    };
    Ok(
        json!({"format_version":1,"task_id":task,"task_status":status,"reserved_calls":calls.len(),"recorded_results":recorded,"results_not_recorded":calls.len()-recorded,"legacy_results_without_call_identity":legacy,
            "reserved_output_tokens":sum("reserved_output_tokens")?,"charged_prompt_bytes":sum("prompt_bytes")?,"reported_usage_complete":complete,
            "input_tokens":if complete {Some(sum("input_tokens")?)} else {None},"output_tokens":if complete {Some(sum("output_tokens")?)} else {None},
            "remaining_inference_admission":remaining,"calls":calls.values().collect::<Vec<_>>(),"language_boundary":language_boundary.as_ref().map(|reservation| json!({
                "reservation":reservation,"phase":if presentation_recorded {"presented"}
                    else if presentation_dispatched {"presentation_uncommitted"}
                    else if calls.get(&1).is_some_and(|call| call["state"] == "result_recorded") {"reasoning"}
                    else {"interpretation_uncommitted"},
                "presentation_capacity_held":!presentation_dispatched,
                "continuation_supported":false,"resume_authorized":false
            })),"automatic_replay":false,"resume_authorized":false,"weights_attested":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dispatch(store: &Store, task: &str, call: u64) {
        store
            .event(
                task,
                "model_dispatch",
                "jev-r",
                json!({"call":call,"role":"solver","allocation":384,"prompt_bytes":123}),
            )
            .unwrap();
    }
    fn response(call: u64) -> Value {
        json!({"call":call,"node":{"role":"solver","model":"fixture","content_hash":sha256(b"private answer")},"text":"private answer","input_tokens":7,"output_tokens":3})
    }
    #[test]
    fn malformed_advisory_metrics_do_not_change_recovery_ledger_authority() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("private goal", "fast").unwrap();
        dispatch(&store, &task, 1);
        store
            .event(&task, "reasoning", "jev-r", response(1))
            .unwrap();
        store
            .event(
                &task,
                "inference_provider_metrics",
                "runtime",
                json!({"format_version":99}),
            )
            .unwrap();
        assert!(store.inference_audit(&task).is_err());
        let db = store.lock().unwrap();
        let ledger = audit_from(&db, &task).unwrap();
        assert_eq!(ledger["reserved_calls"], 1);
        assert_eq!(ledger["resume_authorized"], false);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM events WHERE task_id=?1",
                [&task],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            3
        );
    }

    #[test]
    fn pairs_reservations_without_inventing_legacy_or_unrecorded_usage() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("private goal", "fast").unwrap();
        dispatch(&store, &task, 1);
        store
            .event(&task, "reasoning", "jev-r", response(1))
            .unwrap();
        let report = store.inference_audit(&task).unwrap();
        assert_eq!(report["reported_usage_complete"], true);
        assert_eq!(report["input_tokens"], 7);
        assert_eq!(report["reserved_output_tokens"], 384);
        assert!(!report.to_string().contains("private"));
        dispatch(&store, &task, 2);
        let report = store.inference_audit(&task).unwrap();
        assert_eq!(report["results_not_recorded"], 1);
        assert!(report["input_tokens"].is_null());
        assert_eq!(report["reserved_output_tokens"], 768);
        let mut legacy = response(2);
        legacy.as_object_mut().unwrap().remove("call");
        store.event(&task, "reasoning", "jev-r", legacy).unwrap();
        let report = store.inference_audit(&task).unwrap();
        assert_eq!(report["legacy_results_without_call_identity"], 1);
        assert_eq!(report["results_not_recorded"], 1);
        assert_eq!(report["resume_authorized"], false);
    }
    fn settings(store: &Store, task: &str, cfg: &crate::config::Config, call_limit: u64) {
        store.event(task, "execution_settings", "runtime", json!({"format_version":1,"configuration":cfg,"mode":"standard","call_limit":call_limit,"cycle_limit":1})).unwrap();
    }
    #[test]
    fn remaining_admission_keeps_unrecorded_calls_charged() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("fixture", "standard").unwrap();
        let cfg = crate::config::Config::default();
        settings(&store, &task, &cfg, 4);
        dispatch(&store, &task, 1);
        let report = store.inference_audit(&task).unwrap();
        assert_eq!(report["remaining_inference_admission"]["calls"], 3);
        assert_eq!(
            report["remaining_inference_admission"]["reserved_output_tokens"],
            cfg.max_tokens - 384
        );
        assert_eq!(
            report["remaining_inference_admission"]["prompt_bytes"],
            cfg.max_input_bytes - 123
        );
        assert!(report["remaining_inference_admission"]["wall_time"].is_null());
        assert!(report["input_tokens"].is_null());
        assert_eq!(report["results_not_recorded"], 1);
    }
    #[test]
    fn durable_tool_and_cycle_charges_are_bounded_and_never_imply_results() {
        for variant in 0..6 {
            let store = Store::in_memory().unwrap();
            let task = store.create_task("fixture", "standard").unwrap();
            let cfg = crate::config::Config::default();
            let mut snapshot = json!({"format_version":1,"configuration":cfg,"mode":"standard","call_limit":4,"cycle_limit":1});
            if variant != 1 {
                snapshot["resource_reservations_version"] = json!(1);
            }
            store
                .event(&task, "execution_settings", "runtime", snapshot)
                .unwrap();
            store.event(&task,"tool_reservation",if variant == 2 {"jev-r"} else {"security-kernel"},json!({"format_version":1,"actor":"jev-plus","capability":if variant == 3 {"candidate.promote"} else {"knowledge.search"},"invocation":if variant == 4 {2} else {1}})).unwrap();
            store
                .event(
                    &task,
                    "cycle_reservation",
                    "jev-r",
                    json!({"format_version":1,"cycle":1}),
                )
                .unwrap();
            if variant == 5 {
                store
                    .event(
                        &task,
                        "cycle_reservation",
                        "jev-r",
                        json!({"format_version":1,"cycle":2}),
                    )
                    .unwrap();
            }
            let result = store.inference_audit(&task);
            if variant == 0 {
                let report = result.unwrap();
                assert_eq!(
                    report["remaining_inference_admission"]["tool_calls"],
                    cfg.max_tool_calls - 1
                );
                assert_eq!(report["remaining_inference_admission"]["cycles"], 0);
                assert_eq!(report["resume_authorized"], false);
            } else {
                assert!(result.is_err(), "variant {variant}");
            }
        }
    }
    #[test]
    fn rejects_invalid_snapshots_and_overdrawn_original_budgets() {
        for variant in 0..7 {
            let store = Store::in_memory().unwrap();
            let task = store
                .create_task("fixture", if variant == 0 { "fast" } else { "standard" })
                .unwrap();
            let cfg = crate::config::Config {
                max_calls: if variant == 1 { 1 } else { 8 },
                max_tokens: if variant == 2 { 100 } else { 1600 },
                max_input_bytes: if variant == 3 { 100 } else { 128000 },
                ..crate::config::Config::default()
            };
            if variant == 4 {
                dispatch(&store, &task, 1);
            }
            settings(&store, &task, &cfg, if variant == 1 { 1 } else { 4 });
            if variant == 5 {
                settings(&store, &task, &cfg, 4);
            }
            if variant != 4 {
                dispatch(&store, &task, 1);
            }
            if variant == 1 {
                dispatch(&store, &task, 2);
            }
            if variant == 6 {
                let mut old = response(1);
                old.as_object_mut().unwrap().remove("call");
                store.event(&task, "reasoning", "jev-r", old).unwrap();
            }
            assert!(store.inference_audit(&task).is_err(), "variant {variant}");
        }
    }
    #[test]
    fn rejects_duplicate_unmatched_and_tampered_call_results() {
        for variant in 0..5 {
            let store = Store::in_memory().unwrap();
            let task = store.create_task("invalid", "fast").unwrap();
            dispatch(&store, &task, 1);
            let mut result = response(if variant == 0 { 2 } else { 1 });
            if variant == 1 {
                result["node"]["content_hash"] = json!("forged");
            }
            if variant == 2 {
                result["output_tokens"] = json!(385);
            }
            if variant == 3 {
                result["call"] = json!("forged");
            }
            store.event(&task, "reasoning", "jev-r", result).unwrap();
            if variant == 4 {
                store
                    .event(&task, "reasoning", "jev-r", response(1))
                    .unwrap();
            }
            assert!(store.inference_audit(&task).is_err());
        }
    }
    #[tokio::test]
    async fn runtime_records_result_call_identity_without_exposing_answer_in_summary() {
        let store = std::sync::Arc::new(Store::in_memory().unwrap());
        let cfg = crate::config::Config {
            interpret_goals: false,
            ..crate::config::Config::default()
        };
        let outcome = crate::engine::Engine::new(cfg, store.clone())
            .run("Explain ownership", "fast")
            .await
            .unwrap();
        let report = store.inference_audit(&outcome.task_id).unwrap();
        assert_eq!(report["reserved_calls"], outcome.calls);
        assert_eq!(report["recorded_results"], outcome.calls);
        assert_eq!(report["results_not_recorded"], 0);
        assert_eq!(report["reported_usage_complete"], false);
        assert_eq!(report["remaining_inference_admission"]["calls"], 0);
        assert_eq!(report["remaining_inference_admission"]["cycles"], 0);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod clock_tests {
    use super::*;
    #[test]
    fn saved_clock_is_validated_and_reboot_never_grants_a_new_allowance() {
        for variant in 0..6 {
            let store = Store::in_memory().unwrap();
            let task = store.create_task("clock fixture", "standard").unwrap();
            let cfg = crate::config::Config {
                max_wall_seconds: 1,
                ..Default::default()
            };
            let start = crate::execution_clock::ExecutionStart::new(1);
            let mut clock =
                serde_json::to_value(start.clock.expect("Linux clock unavailable")).unwrap();
            match variant {
                1 => clock["format_version"] = json!(2),
                2 => clock["deadline_ns"] = json!(clock["deadline_ns"].as_u64().unwrap() + 1),
                3 => clock["boot_id"] = json!("not-a-boot-id"),
                4 => clock["boot_id"] = json!(uuid::Uuid::new_v4().to_string()),
                5 => clock["permissions"] = json!(["shell"]),
                _ => {}
            }
            store.event(&task,"execution_settings","runtime",json!({"format_version":1,"configuration":cfg,"mode":"standard","call_limit":4,"cycle_limit":1,"execution_clock":clock})).unwrap();
            let report = store.inference_audit(&task);
            if variant == 0 {
                let report = report.unwrap();
                let time = &report["remaining_inference_admission"]["wall_time"];
                assert_eq!(time["status"], "same_boot");
                assert!(time["remaining_ns"].as_u64().unwrap() <= 1_000_000_000);
                assert_eq!(report["resume_authorized"], false);
            } else if variant == 4 {
                let report = report.unwrap();
                let time = &report["remaining_inference_admission"]["wall_time"];
                assert_eq!(time["status"], "different_boot");
                assert!(time["remaining_ns"].is_null());
            } else {
                assert!(report.is_err(), "variant {variant}");
            }
        }
    }
}
