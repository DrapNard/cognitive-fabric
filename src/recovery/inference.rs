//! Read-only reconstruction of durable inference reservations, never replay authority.
use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
impl Store {
    pub fn inference_audit(&self, task: &str) -> Result<Value> {
        security::authorize("human-cli", "task.observe")?;
        uuid::Uuid::parse_str(task)?;
        let mut db = self.lock()?;
        let tx = db.transaction()?;
        let status: String = tx.query_row(
            "SELECT CASE WHEN length(status)<=64 THEN status END FROM tasks WHERE id=?1",
            [task],
            |r| r.get(0),
        )?;
        let mut calls = BTreeMap::<u64, Value>::new();
        let mut legacy = 0usize;
        {
            let mut stmt=tx.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('model_dispatch','reasoning') ORDER BY id LIMIT 129")?;
            let mut rows = stmt.query([task])?;
            let mut count = 0;
            while let Some(row) = rows.next()? {
                count += 1;
                if count > 128 {
                    bail!("Inference audit exceeds bounded call history")
                }
                let kind: String = row.get(0)?;
                let source: String = row.get(1)?;
                if source != "jev-r" {
                    bail!("Unexpected inference audit source")
                }
                let raw: String = row.get(2)?;
                let payload: Value = serde_json::from_str(&raw)?;
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
                    record["model"] = json!(model);
                    record["state"] = json!("result_recorded");
                }
            }
        }
        tx.commit()?;
        let recorded = calls
            .values()
            .filter(|r| r["state"] == "result_recorded")
            .count();
        let complete = legacy == 0
            && !calls.is_empty()
            && recorded == calls.len()
            && calls.values().all(|r| {
                r["input_tokens"].as_u64().is_some() && r["output_tokens"].as_u64().is_some()
            });
        let sum = |field: &str| -> Result<u64> {
            calls.values().try_fold(0u64, |total, r| {
                total
                    .checked_add(r[field].as_u64().unwrap_or(0))
                    .ok_or_else(|| anyhow::anyhow!("Inference audit metric overflow"))
            })
        };
        Ok(
            json!({"format_version":1,"task_id":task,"task_status":status,"reserved_calls":calls.len(),"recorded_results":recorded,"results_not_recorded":calls.len()-recorded,"legacy_results_without_call_identity":legacy,
            "reserved_output_tokens":sum("reserved_output_tokens")?,"charged_prompt_bytes":sum("prompt_bytes")?,"reported_usage_complete":complete,
            "input_tokens":if complete {Some(sum("input_tokens")?)} else {None},"output_tokens":if complete {Some(sum("output_tokens")?)} else {None},
            "calls":calls.values().collect::<Vec<_>>(),"automatic_replay":false,"resume_authorized":false,"weights_attested":false}),
        )
    }
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
    }
}
