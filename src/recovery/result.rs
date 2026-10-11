//! Operator recovery of independently checkable saved answers; never inference replay.
use super::{inference, ExecutionLease};
use crate::{
    security,
    store::{sha256, Store},
    verification::Criterion,
};
use anyhow::{bail, Result};
use rusqlite::params;
use serde_json::{json, Value};
impl Store {
    pub fn recover_verified_result(&self, task: &str) -> Result<Value> {
        security::authorize("human-cli", "task.recover")?;
        uuid::Uuid::parse_str(task)?;
        let path = self
            .runtime_lock
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Result recovery requires a persistent store"))?;
        let _lease = ExecutionLease::acquire(path, true).map_err(|e| {
            anyhow::anyhow!("Cannot recover a result while runtime leases are held: {e}")
        })?;
        let mut db = self.lock()?;
        let started = std::time::Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > std::time::Duration::from_secs(5)),
        );
        let result = (|| -> Result<(Value, crate::events::Progress)> {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let (query,status): (String,String) = tx.query_row("SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END,CASE WHEN length(status)<=64 THEN status END FROM tasks WHERE id=?1",[task],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if !matches!(status.as_str(), "running" | "interrupted") {
                bail!("Only abandoned running or interrupted tasks permit result recovery")
            }
            let criterion = Criterion::from_query(&query)?.ok_or_else(|| {
                anyhow::anyhow!("No independent structured checker for this task")
            })?;
            security::TaskPolicy::reasoning(task)?.authorize(
                task,
                "jev-plus",
                criterion.capability(),
            )?;
            let audit = inference::audit_from(&tx, task)?;
            let remaining = audit["remaining_inference_admission"]["tool_calls"]
                .as_u64()
                .filter(|n| *n > 0)
                .ok_or_else(|| {
                    anyhow::anyhow!("No proven remaining verification tool allowance")
                })?;
            if audit["results_not_recorded"] != 0
                || audit["legacy_results_without_call_identity"] != 0
            {
                bail!("Unresolved inference reservations prevent result recovery")
            }
            let raw: String = tx.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver' ORDER BY id DESC LIMIT 1",[task],|r|r.get(0))?;
            let saved: Value = serde_json::from_str(&raw)?;
            let answer = saved["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing saved solver answer"))?;
            // The ledger has checked call identity, source, result hash and reservation.
            let tools: u64 = tx.query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='tool_reservation'",
                [task],
                |r| r.get(0),
            )?;
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'tool_reservation','security-kernel',?2,unixepoch())",params![task,json!({"format_version":1,"invocation":tools+1,"actor":"jev-plus","capability":criterion.capability(),"origin":"operator_recovery"}).to_string()])?;
            let checked = criterion.check(answer);
            let verified = checked.is_ok();
            let proof = json!({"criterion":criterion.name(),"verified":verified,"origin":"saved_solver_recovery","objective_hash":sha256(query.as_bytes()),"answer_hash":sha256(answer.as_bytes()),"reason":checked.err().map(|e|e.to_string())});
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'verification','verifier-r',?2,unixepoch())",params![task,proof.to_string()])?;
            if verified {
                tx.execute(
                    "UPDATE tasks SET status='completed',answer=?2,updated=unixepoch() WHERE id=?1",
                    params![task, answer],
                )?;
            }
            let report = json!({"format_version":1,"task_id":task,"status":if verified {"completed"} else {status.as_str()},"verified":verified,"criterion":criterion.name(),"answer_hash":sha256(answer.as_bytes()),"automatic_replay":false,"inference_repeated":false,"remaining_tool_calls":remaining-1,"resource_usage_complete":false,"reported_inference_usage_complete":audit["reported_usage_complete"],"subtask_results_verified":false});
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'recovery','human-cli',?2,unixepoch())",params![task,report.to_string()])?;
            let progress = crate::events::Progress::from_audit(
                tx.last_insert_rowid() as u64,
                task,
                "recovery",
                "human-cli",
                &report,
            )?
            .expect("Recovery has a typed event");
            if started.elapsed() > std::time::Duration::from_secs(5) {
                bail!("Result recovery deadline exhausted")
            }
            tx.commit()?;
            Ok((report, progress))
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        let (report, progress) = result?;
        self.events.publish(&progress)?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, sync::Arc};
    fn fixture(query: &str, answer: &str, tools: u32) -> (PathBuf, Arc<Store>, String) {
        let root =
            std::env::temp_dir().join(format!("fabric-result-recovery-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(&root.join("state.db")).unwrap());
        let task = store.create_task(query, "fast").unwrap();
        let cfg = crate::config::Config {
            max_tool_calls: tools,
            ..Default::default()
        };
        let _session = crate::reasoning::Session::new(
            cfg.clone(),
            store.clone(),
            Arc::new(crate::backend::LocalBackend(cfg)),
            task.clone(),
            "fast",
        )
        .unwrap();
        store
            .event(
                &task,
                "model_dispatch",
                "jev-r",
                json!({"call":1,"role":"solver","allocation":384,"prompt_bytes":100}),
            )
            .unwrap();
        store.event(&task,"reasoning","jev-r",json!({"call":1,"node":{"role":"solver","model":"fixture","content_hash":sha256(answer.as_bytes())},"text":answer,"input_tokens":null,"output_tokens":null})).unwrap();
        store
            .finish_task(&task, "interrupted", "Unknown prior completion")
            .unwrap();
        (root, store, task)
    }
    #[test]
    fn completes_only_after_fresh_check_without_repeating_inference() {
        let (root, store, task) = fixture(r#"sort:{"values":[2,1,2]}"#, "[1,2,2]", 4);
        store
            .finish_task(&task, "running", "Abandoned before terminal commit")
            .unwrap();
        let lease = store.execution_lease().unwrap();
        assert!(store.recover_verified_result(&task).is_err());
        drop(lease);
        let report = store.recover_verified_result(&task).unwrap();
        assert_eq!(report["verified"], true);
        assert_eq!(report["inference_repeated"], false);
        assert_eq!(report["resource_usage_complete"], false);
        assert_eq!(store.task(&task).unwrap()["task"]["answer"], "[1,2,2]");
        let audit = store.inference_audit(&task).unwrap();
        assert_eq!(audit["reserved_calls"], 1);
        assert_eq!(audit["remaining_inference_admission"]["tool_calls"], 3);
        assert!(store.recover_verified_result(&task).is_err());
        assert!(security::authorize("jev-r", "task.recover").is_err());
        drop(store);
        let reopened = Store::open(&root.join("state.db")).unwrap();
        assert_eq!(reopened.task(&task).unwrap()["task"]["status"], "completed");
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_invalid_saved_results_and_preserves_failed_check_charges() {
        for variant in 0..6 {
            let (root, store, task) = fixture(
                if variant == 4 {
                    "Explain sorting"
                } else {
                    r#"sort:{"values":[2,1,2]}"#
                },
                if variant == 0 { "[1,2]" } else { "[1,2,2]" },
                if variant == 3 { 0 } else { 4 },
            );
            if variant == 1 {
                store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.node.content_hash','forged') WHERE kind='reasoning'",[]).unwrap();
            }
            if variant == 2 {
                store
                    .lock()
                    .unwrap()
                    .execute("DELETE FROM events WHERE kind='reasoning'", [])
                    .unwrap();
            }
            if variant == 5 {
                store
                    .finish_task(&task, "cancelled", "Cancellation remains terminal")
                    .unwrap();
            }
            let result = store.recover_verified_result(&task);
            if variant == 0 {
                assert_eq!(result.unwrap()["verified"], false);
                assert_eq!(
                    store.inference_audit(&task).unwrap()["remaining_inference_admission"]
                        ["tool_calls"],
                    3
                );
            } else {
                assert!(result.is_err(), "variant {variant}");
            }
            assert_eq!(
                store.task(&task).unwrap()["task"]["status"],
                if variant == 5 {
                    "cancelled"
                } else {
                    "interrupted"
                }
            );
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn failed_audit_write_rolls_back_result_and_tool_charge() {
        let (root, store, task) = fixture(
            r#"order:{"nodes":["a","b"],"before":[["a","b"]]}"#,
            r#"["a","b"]"#,
            4,
        );
        store.lock().unwrap().execute_batch("CREATE TRIGGER reject_recovered_proof BEFORE INSERT ON events WHEN NEW.kind='verification' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
        assert!(store.recover_verified_result(&task).is_err());
        assert_eq!(store.task(&task).unwrap()["task"]["status"], "interrupted");
        assert_eq!(
            store.inference_audit(&task).unwrap()["remaining_inference_admission"]["tool_calls"],
            4
        );
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_recovered_proof")
            .unwrap();
        assert_eq!(
            store.recover_verified_result(&task).unwrap()["verified"],
            true
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
