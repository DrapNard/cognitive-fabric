//! Reconcile a fully recorded revision handoff without replaying inference.
use super::{continuation, inference, ExecutionLease};
use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Critique {
    revise: bool,
    reason: String,
}

fn critique_reason(text: &str) -> Result<String> {
    let critique = crate::structured::parse::<Critique>(text, 8192)?;
    if !critique.revise || critique.reason.trim().is_empty() || critique.reason.len() > 1000 {
        bail!("Critique does not admit a revision")
    }
    Ok(critique.reason)
}

pub(super) fn validate_revision_boundary(
    db: &rusqlite::Connection,
    task: &str,
    boundary: &Value,
    audit: &Value,
) -> Result<()> {
    let mut stmt=db.prepare("SELECT CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' ORDER BY id DESC LIMIT 2")?;
    let rows = stmt.query_map([task], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut saved = Vec::new();
    for row in rows {
        let (source, raw) = row?;
        if source != "jev-r" {
            bail!("Invalid reflector boundary source")
        };
        saved.push(crate::strict_json::value(raw.as_bytes(), 262144)?);
    }
    let critic = boundary["phase"] == "ready_critic";
    let required = if critic { 1 } else { 2 };
    saved.truncate(required);
    let call = audit["reserved_calls"].as_u64().unwrap();
    if saved.len() != required
        || call < required as u64
        || saved[0]["node"]["role"] != if critic { "solver" } else { "critic" }
        || saved[0]["call"] != call
        || (!critic && (saved[1]["node"]["role"] != "solver" || saved[1]["call"] != call - 1))
        || saved
            .iter()
            .any(|v| v["node"]["cycle"] != boundary["cycles_before"])
        || saved[required - 1]["text"] != boundary["previous_answer"]
        || boundary["strategy"] != ""
        || (if critic {
            !boundary["critique_reason"].is_null()
        } else {
            boundary["critique_reason"] != critique_reason(saved[0]["text"].as_str().unwrap())?
        })
    {
        bail!("Revision boundary differs from recorded solver/critic handoff")
    }
    Ok(())
}

impl Store {
    pub fn recover_revision_result(&self, task: &str, human_approve: bool) -> Result<Value> {
        security::authorize("human-cli", "task.recover")?;
        if !human_approve {
            bail!("Revision recovery requires explicit operator approval")
        }
        uuid::Uuid::parse_str(task)?;
        let path = self
            .runtime_lock
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Revision recovery requires a persistent store"))?;
        let _lease = ExecutionLease::acquire(path, true).map_err(|e| {
            e.context("Cannot acquire exclusive runtime lease for revision reconciliation")
        })?;
        let mut db = self.lock()?;
        let started = Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(5)),
        );
        let result = (|| -> Result<Value> {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            super::validate_verifier_recovery(&tx, task, None)?;
            let query:String=tx.query_row("SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END FROM tasks WHERE id=?1",[task],|r|r.get(0))?;
            if crate::verification::Criterion::from_query(&query)?.is_some() {
                bail!("Structured objectives require independent result recovery")
            }
            let audit = inference::audit_from(&tx, task)?;
            if audit["results_not_recorded"] != 0
                || audit["legacy_results_without_call_identity"] != 0
            {
                bail!("Unresolved inference prevents revision recovery")
            }
            let mut stmt=tx.prepare("SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' ORDER BY id DESC LIMIT 3")?;
            let rows = stmt.query_map([task], |r| {
                Ok((
                    r.get::<_, u64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            let mut results = Vec::new();
            for row in rows {
                let (id, source, raw) = row?;
                if source != "jev-r" {
                    bail!("Invalid revision result source")
                };
                results.push((id, crate::strict_json::value(raw.as_bytes(), 262144)?));
            }
            drop(stmt);
            let required = match results
                .first()
                .and_then(|(_, v)| v["node"]["role"].as_str())
            {
                Some("solver") => 1,
                Some("critic") => 2,
                Some("reflector") => 3,
                _ => bail!("No recorded revision handoff"),
            };
            let needs_critic = required == 1;
            let needs_reflector = required == 2;
            if results.len() < required {
                bail!("No complete recorded revision handoff")
            }
            results.truncate(required);
            results.reverse();
            let solver = &results[0].1;
            let cycles: u64 = tx.query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='cycle_reservation'",
                [task],
                |r| r.get(0),
            )?;
            let last = audit["reserved_calls"].as_u64().unwrap();
            if cycles == 0 || last < required as u64 {
                bail!("Missing revision charges")
            }
            for (offset, (_, saved)) in results.iter().enumerate() {
                if saved["node"]["role"] != ["solver", "critic", "reflector"][offset]
                    || saved["node"]["cycle"] != cycles
                    || saved["call"] != last - required as u64 + 1 + offset as u64
                {
                    bail!("Latest calls are not a complete same-cycle revision")
                }
            }
            let reason = if needs_critic {
                String::new()
            } else {
                critique_reason(results[1].1["text"].as_str().unwrap())?
            };
            let previous = solver["text"].as_str().unwrap();
            let strategy: String = if required < 3 {
                String::new()
            } else {
                results[2].1["text"]
                    .as_str()
                    .unwrap()
                    .chars()
                    .take(4000)
                    .collect()
            };
            let (boundary_id,boundary_source,raw):(u64,String,String)=tx.query_row("SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning_boundary' ORDER BY id DESC LIMIT 1",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let mut boundary = crate::strict_json::value(raw.as_bytes(), 262144)?;
            let prior_reflector_boundary = required == 3 && boundary["phase"] == "ready_reflector";
            let prior_critic_boundary = !needs_critic && boundary["phase"] == "ready_critic";
            let prior_valid = if prior_critic_boundary {
                boundary["cycles_before"] == cycles
                    && boundary["calls_before"] == last - required as u64 + 1
                    && boundary["previous_answer"] == previous
                    && boundary["last_answer_hash"] == sha256(previous.as_bytes())
                    && boundary["critique_reason"].is_null()
            } else if prior_reflector_boundary {
                boundary["cycles_before"] == cycles
                    && boundary["calls_before"] == last - 1
                    && boundary["previous_answer"] == previous
                    && boundary["last_answer_hash"] == sha256(previous.as_bytes())
                    && boundary["critique_reason"] == reason
            } else {
                boundary["phase"] == "ready_solver"
                    && boundary["cycles_before"] == cycles - 1
                    && boundary["calls_before"] == last - required as u64
                    && (if cycles == 1 {
                        boundary["previous_answer"] == "" && boundary["last_answer_hash"].is_null()
                    } else {
                        boundary["previous_answer"].as_str().is_some_and(|answer| {
                            boundary["last_answer_hash"] == sha256(answer.as_bytes())
                        })
                    })
                    && boundary["last_answer_hash"] != sha256(previous.as_bytes())
            };
            let boundary_before_result = if prior_critic_boundary {
                boundary_id < results[1].0
            } else if prior_reflector_boundary {
                boundary_id < results[2].0
            } else {
                boundary_id < results[0].0
            };
            if boundary_source != "jev-r"
                || !boundary_before_result
                || boundary["format_version"] != 1
                || !prior_valid
            {
                bail!("Revision boundary is missing, consumed differently or already reconciled")
            }
            let tools: u64 = tx.query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='tool_reservation'",
                [task],
                |r| r.get(0),
            )?;
            boundary["phase"] = json!(if needs_critic {
                "ready_critic"
            } else if needs_reflector {
                "ready_reflector"
            } else {
                "ready_solver"
            });
            if needs_reflector {
                boundary["critique_reason"] = json!(reason);
            } else {
                boundary.as_object_mut().unwrap().remove("critique_reason");
            }
            boundary["strategy"] = json!(strategy);
            boundary["previous_answer"] = json!(previous);
            boundary["last_answer_hash"] = json!(sha256(previous.as_bytes()));
            boundary["calls_before"] = json!(last);
            boundary["cycles_before"] = json!(cycles);
            boundary["tool_calls_before"] = json!(tools);
            boundary["reserved_output_tokens"] = audit["reserved_output_tokens"].clone();
            boundary["charged_prompt_bytes"] = audit["charged_prompt_bytes"].clone();
            boundary["origin"] = json!("operator_revision_recovery");
            let encoded = serde_json::to_string(&boundary)?;
            if encoded.len() > 262144 {
                bail!("Recovered revision checkpoint exceeds bounds")
            }
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'reasoning_boundary','jev-r',?2,unixepoch())",params![task,encoded])?;
            continuation::validate_checkpoint(&tx, task)?;
            let report = json!({"format_version":1,"task_id":task,"cycle":cycles,"call":last,"strategy_sha256":sha256(strategy.as_bytes()),"previous_answer_sha256":sha256(previous.as_bytes()),"revision_ready":true,"phase":boundary["phase"],"inference_replayed":false,"original_budget_preserved":true,"result_verified":false,"resume_authorized":false});
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'revision_recovery','human-cli',?2,unixepoch())",params![task,report.to_string()])?;
            if started.elapsed() > Duration::from_secs(5) {
                bail!("Revision recovery admission deadline exhausted")
            }
            tx.commit()?;
            Ok(report)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::{Generation, InferenceBackend},
        config::Config,
        reasoning::Session,
    };
    use std::{
        collections::VecDeque,
        future::Future,
        pin::Pin,
        sync::{Arc, Mutex},
    };
    struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
    impl InferenceBackend for Script {
        fn generate<'a>(
            &'a self,
            role: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            let (expected, text) = self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("Unexpected inference replay");
            assert_eq!(role, expected);
            Box::pin(async move {
                Ok(Generation {
                    text: text.into(),
                    model: "fixture/v1".into(),
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    elapsed_ms: 1.0,
                    queue_wait_ms: None,
                    provider_metrics: None,
                })
            })
        }
    }
    async fn fixture() -> (Arc<Store>, String, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("fabric-revision-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(&root.join("state.db")).unwrap());
        let task = store.create_task("Explain ownership", "deep").unwrap();
        let cfg = Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            max_calls: 6,
            max_tokens: 2000,
            ..Config::default()
        };
        let backend = Arc::new(Script(Mutex::new(
            vec![
                ("planner", "invalid plan"),
                ("solver", "first answer"),
                (
                    "critic",
                    r#"{"revise":true,"reason":"Add lifetime constraints"}"#,
                ),
                ("reflector", "Explain lifetime constraints"),
                ("solver", "second answer"),
            ]
            .into(),
        )));
        store.event(&task,"execution_options","runtime",json!({"format_version":1,"profile":"full","explain":false,"conversation_ref":null})).unwrap();
        store.event(&task,"module_snapshot","fabric",json!({"modules":[{"id":"inference.local","version":"1.0.0","roles":crate::fabric::protocol::ROLES}]})).unwrap();
        let mut session = Session::new(cfg, store.clone(), backend, task.clone(), "deep").unwrap();
        session
            .solve("Explain ownership", "", "deep")
            .await
            .unwrap();
        let db = store.lock().unwrap();
        let cutoff:u64=db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='reflector'",[&task],|r|r.get(0)).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            params![task, cutoff],
        )
        .unwrap();
        drop(db);
        drop(session);
        (store, task, root)
    }
    #[tokio::test]
    async fn revision_recovery_refuses_uncertain_changed_expired_and_live_state_atomically() {
        for variant in 0..10 {
            let (store, task, root) = fixture().await;
            if variant == 0 {
                let before = store.task(&task).unwrap();
                assert!(store.recover_revision_result(&task, false).is_err());
                let lease = store.execution_lease().unwrap();
                assert!(store.recover_revision_result(&task, true).is_err());
                drop(lease);
                assert_eq!(store.task(&task).unwrap(), before);
            } else {
                let sql=match variant {
                    1=>"DELETE FROM events WHERE kind='reasoning' AND json_extract(payload,'$.node.role')='reflector'",
                    2=>"UPDATE events SET payload=json_set(payload,'$.text','changed') WHERE kind='reasoning' AND json_extract(payload,'$.node.role')='reflector'",
                    3=>"UPDATE events SET payload=json_set(payload,'$.node.cycle',0) WHERE kind='reasoning' AND json_extract(payload,'$.node.role')='reflector'",
                    4=>"UPDATE events SET payload=json_set(payload,'$.execution_clock',NULL) WHERE kind='execution_settings'",
                    5=>"UPDATE events SET payload=json_set(payload,'$.context','changed') WHERE kind='reasoning_context'",
                    6=>"CREATE TRIGGER reject_recovery BEFORE INSERT ON events WHEN NEW.kind='revision_recovery' BEGIN SELECT RAISE(ABORT,'fixture recovery audit failure'); END",
                    7=>"UPDATE tasks SET status='completed'",
                    8=>"UPDATE events SET source='foreign' WHERE kind='reasoning_boundary'",
                    _=>"UPDATE events SET payload=json_set(payload,'$.last_answer_hash','ca5edb98a63a1c94c5b357f2b2e409cbbbe496d0b267b056ed9f7d52ae167f48') WHERE kind='reasoning_boundary'",

                };
                store.lock().unwrap().execute_batch(sql).unwrap();
                let before = store.task(&task).unwrap();
                assert!(
                    store.recover_revision_result(&task, true).is_err(),
                    "accepted variant {variant}"
                );
                assert_eq!(store.task(&task).unwrap(), before);
            }
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn revision_recovery_rejects_nonadmitting_critique_even_with_consistent_hashes() {
        for text in [
            r#"{"revise":false,"reason":"No useful revision"}"#,
            r#"{"revise":true,"reason":" "}"#,
            "not JSON",
        ] {
            let (store, task, root) = fixture().await;
            store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.text',?1,'$.node.content_hash',?2) WHERE kind='reasoning' AND json_extract(payload,'$.node.role')='critic'",params![text,sha256(text.as_bytes())]).unwrap();
            let before = store.task(&task).unwrap();
            assert!(store.recover_revision_result(&task, true).is_err());
            assert_eq!(store.task(&task).unwrap(), before);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn recorded_reflection_resumes_through_http_without_replaying_the_cycle() {
        http_revision(1, None).await;
    }
    #[tokio::test]
    async fn recorded_critic_resumes_reflection_and_solver_without_replaying_prior_calls() {
        http_revision(2, None).await;
    }
    #[tokio::test]
    async fn recorded_solver_resumes_critic_reflector_and_solver_without_replaying_answer() {
        http_revision(3, None).await;
    }
    #[tokio::test]
    async fn resumed_critic_stops_unverified_on_invalid_or_nonadmitting_output() {
        for text in [
            r#"{"revise":false,"reason":"No useful change"}"#,
            "not JSON",
        ] {
            http_revision(3, Some(text)).await;
        }
    }
    fn truncate_after_solver(store: &Store, task: &str) {
        let db = store.lock().unwrap();
        let id:u64=db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver' AND json_extract(payload,'$.node.cycle')=1 ORDER BY id LIMIT 1",[task],|r|r.get(0)).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            params![task, id],
        )
        .unwrap();
    }
    fn truncate_after_critic(store: &Store, task: &str) {
        let db = store.lock().unwrap();
        let id:u64=db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='critic'",[task],|r|r.get(0)).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            params![task, id],
        )
        .unwrap();
    }
    #[tokio::test]
    async fn critic_recovery_rejects_insufficient_budget_and_changed_reason_without_writes() {
        for mutation in 0..4 {
            let (store, task, root) = fixture().await;
            truncate_after_critic(&store, &task);
            if mutation < 3 {
                let expression=match mutation {
                    0=>"json_set(payload,'$.configuration.max_calls',4,'$.call_limit',4)",
                    1=>"json_set(payload,'$.configuration.max_tokens',1000)",
                    _=>"json_set(payload,'$.configuration.max_input_bytes',json_extract(payload,'$.configuration.max_input_bytes')/2)",
                };
                // The input ceiling must fit past calls but fail the future pair.
                if mutation == 2 {
                    let bytes = store.inference_audit(&task).unwrap()["charged_prompt_bytes"]
                        .as_u64()
                        .unwrap();
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.max_input_bytes',?1) WHERE kind='execution_settings'",[bytes+100]).unwrap();
                } else {
                    store.lock().unwrap().execute_batch(&format!("UPDATE events SET payload={expression} WHERE kind='execution_settings'")).unwrap();
                }
                let before = store.task(&task).unwrap();
                assert!(store.recover_revision_result(&task, true).is_err());
                assert_eq!(store.task(&task).unwrap(), before);
            } else {
                store.recover_revision_result(&task, true).unwrap();
                store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.critique_reason','Changed reason') WHERE kind='reasoning_boundary' AND json_extract(payload,'$.phase')='ready_reflector'",[]).unwrap();
                let before = store.task(&task).unwrap();
                assert!(continuation::preview(&store, &task).is_err());
                assert_eq!(store.task(&task).unwrap(), before);
            }
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn interruption_after_recovered_reflection_requires_its_recorded_result() {
        for result_recorded in [false, true] {
            let (store, task, root) = fixture().await;
            let records = {
                let db = store.lock().unwrap();
                let mut stmt=db.prepare("SELECT kind,source,payload FROM events WHERE task_id=?1 AND id>(SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='critic') ORDER BY id").unwrap();
                let rows = stmt
                    .query_map([&task], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .unwrap();
                rows.collect::<std::result::Result<Vec<_>, _>>().unwrap()
            };
            truncate_after_critic(&store, &task);
            let report = store.recover_revision_result(&task, true).unwrap();
            assert_eq!(report["phase"], "ready_reflector");
            for (kind, source, raw) in records {
                if kind != "reasoning" || result_recorded {
                    store
                        .event(&task, &kind, &source, serde_json::from_str(&raw).unwrap())
                        .unwrap();
                }
            }
            let before = store.task(&task).unwrap();
            let result = store.recover_revision_result(&task, true);
            if result_recorded {
                assert_eq!(result.unwrap()["phase"], "ready_solver");
                assert_eq!(
                    continuation::preview(&store, &task).unwrap()["phase"],
                    "ready_solver"
                );
                assert_eq!(store.inference_audit(&task).unwrap()["reserved_calls"], 4);
            } else {
                assert!(result.is_err());
                assert_eq!(store.task(&task).unwrap(), before);
            }
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn interruption_after_recovered_critic_requires_its_recorded_result() {
        for result_recorded in [false, true] {
            let (store, task, root) = fixture().await;
            let records = {
                let db = store.lock().unwrap();
                let mut stmt=db.prepare("SELECT kind,source,payload FROM events WHERE task_id=?1 AND id>(SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver' AND json_extract(payload,'$.node.cycle')=1 ORDER BY id LIMIT 1) AND id<=(SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='critic') ORDER BY id").unwrap();
                let rows = stmt
                    .query_map([&task], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .unwrap();
                rows.collect::<std::result::Result<Vec<_>, _>>().unwrap()
            };
            truncate_after_solver(&store, &task);
            assert_eq!(
                store.recover_revision_result(&task, true).unwrap()["phase"],
                "ready_critic"
            );
            for (kind, source, raw) in records {
                if kind != "reasoning" || result_recorded {
                    store
                        .event(&task, &kind, &source, serde_json::from_str(&raw).unwrap())
                        .unwrap();
                }
            }
            let before = store.task(&task).unwrap();
            let result = store.recover_revision_result(&task, true);
            if result_recorded {
                assert_eq!(result.unwrap()["phase"], "ready_reflector");
                assert_eq!(
                    continuation::preview(&store, &task).unwrap()["phase"],
                    "ready_reflector"
                );
                assert_eq!(store.inference_audit(&task).unwrap()["reserved_calls"], 3);
            } else {
                assert!(result.is_err());
                assert_eq!(store.task(&task).unwrap(), before);
            }
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn solver_recovery_requires_the_complete_revision_budget_without_writes() {
        for mutation in 0..3 {
            let (store, task, root) = fixture().await;
            truncate_after_solver(&store, &task);
            match mutation {
                0 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.max_calls',4,'$.call_limit',4) WHERE kind='execution_settings'",[]).unwrap();
                }
                1 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.max_tokens',1000) WHERE kind='execution_settings'",[]).unwrap();
                }
                _ => {
                    let bytes = store.inference_audit(&task).unwrap()["charged_prompt_bytes"]
                        .as_u64()
                        .unwrap();
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.max_input_bytes',?1) WHERE kind='execution_settings'",[bytes+100]).unwrap();
                }
            }
            let before = store.task(&task).unwrap();
            let error = store.recover_revision_result(&task, true).unwrap_err();
            assert!(
                error.to_string().contains("Insufficient original budget"),
                "{error}"
            );
            assert_eq!(store.task(&task).unwrap(), before);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    async fn http_revision(missing_calls: usize, stopped_critique: Option<&'static str>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (store, task, root) = fixture().await;
        if missing_calls == 2 {
            truncate_after_critic(&store, &task);
        }
        if missing_calls == 3 {
            truncate_after_solver(&store, &task);
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",params![task,endpoint]).unwrap();
        let before = store.inference_audit(&task).unwrap();
        let report = store.recover_revision_result(&task, true).unwrap();
        assert_eq!(report["revision_ready"], true);
        assert_eq!(report["result_verified"], false);
        assert!(!report.to_string().contains("first answer"));
        let after = store.inference_audit(&task).unwrap();
        for field in [
            "reserved_calls",
            "reserved_output_tokens",
            "charged_prompt_bytes",
        ] {
            assert_eq!(before[field], after[field]);
        }
        assert!(store.recover_revision_result(&task, true).is_err());
        let server = tokio::spawn(async move {
            let answers = if let Some(review) = stopped_critique {
                vec![review]
            } else {
                match missing_calls {
                    3 => vec![
                        r#"{"revise":true,"reason":"Add lifetime constraints"}"#,
                        "Explain lifetime constraints",
                        "second answer",
                    ],
                    2 => vec!["Explain lifetime constraints", "second answer"],
                    _ => vec!["second answer"],
                }
            };
            for (index, answer) in answers.into_iter().enumerate() {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut bytes = [0; 4096];
                    let n = socket.read(&mut bytes).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&bytes[..n]);
                    assert!(request.len() < 131072);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let length = headers
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|n| n.parse::<usize>().ok())
                            })
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            let body: Value =
                                serde_json::from_slice(&request[end + 4..end + 4 + length])
                                    .unwrap();
                            let text = body["messages"].to_string();
                            let expected = if missing_calls == 3 && index == 0 {
                                "Critique your candidate"
                            } else if (missing_calls == 2 && index == 0)
                                || (missing_calls == 3 && index == 1)
                            {
                                "Add lifetime constraints"
                            } else {
                                "Explain lifetime constraints"
                            };
                            assert!(text.contains(expected));
                            assert!(text.contains("first answer"));
                            break;
                        }
                    }
                }
                let body = json!({"choices":[{"message":{"content":answer}}]}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
            }
        });
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            continuation::resume(store.clone(), &task, true, std::future::pending()),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result["status"], "unverified");
        assert_eq!(
            result["new_calls"],
            if stopped_critique.is_some() {
                1
            } else {
                missing_calls
            }
        );
        server.await.unwrap();
        let audit = store.inference_audit(&task).unwrap();
        assert_eq!(
            audit["reserved_calls"],
            if stopped_critique.is_some() { 3 } else { 5 }
        );
        assert_eq!(
            audit["reserved_output_tokens"],
            if stopped_critique.is_some() {
                768
            } else {
                1280
            }
        );
        if stopped_critique.is_some() {
            assert_eq!(store.task(&task).unwrap()["task"]["answer"], "first answer");
        }
        assert!(store.recover_revision_result(&task, true).is_err());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
