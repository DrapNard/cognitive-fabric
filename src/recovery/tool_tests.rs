use super::*;
use std::sync::Arc;
struct Idle;
impl crate::fabric::tools::ArithmeticBackend for Idle {
    fn evaluate<'a>(
        &'a self,
        _: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async { bail!("Idle recovery fixture cannot execute commands") })
    }
}
impl crate::fabric::tools::sql::SqlBackend for Idle {
    fn query<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async { bail!("Idle recovery fixture cannot execute commands") })
    }
}
fn worker_start(store: &Store, task: &str, session: &str) {
    store.worker_audit(task, serde_json::json!({"format_version":1,"session_id":session,"module_id":"inference.local","module_version":"1.0.0","state":"session_started"})).unwrap();
}
fn tool_start(store: &Store, task: &str, capability: &str) -> String {
    let session = uuid::Uuid::new_v4().to_string();
    store.event(task, "tool_worker", "security-kernel", serde_json::json!({"session_id":session,"state":"started","charged_calls":0,"module_id":"tool.local","module_version":"1.0.0","capability":capability})).unwrap();
    session
}
#[test]
fn mixed_tool_recovery_preserves_results_charges_and_closed_sessions() {
    let root = std::env::temp_dir().join(format!("fabric-mixed-recovery-{}", uuid::Uuid::new_v4()));
    let store = Store::open(&root.join("state.db")).unwrap();
    let task = store.create_task("mixed worker history", "fast").unwrap();
    store.finish_task(&task, "completed", "42").unwrap();
    let inference = uuid::Uuid::new_v4().to_string();
    worker_start(&store, &task, &inference);
    let math = tool_start(&store, &task, "math.evaluate");
    let sql = tool_start(&store, &task, "sql.query");
    store
        .event(
            &task,
            "tool_worker",
            "security-kernel",
            serde_json::json!({"session_id":sql,"state":"authorized","charged_calls":1}),
        )
        .unwrap();
    for terminal in ["closed", "failed", "interrupted"] {
        let session = tool_start(&store, &task, "sql.query");
        store
            .event(
                &task,
                "tool_worker",
                "security-kernel",
                serde_json::json!({"session_id":session,"state":terminal}),
            )
            .unwrap();
    }
    let recovered = store.recover_workers().unwrap();
    assert_eq!(
        recovered
            .iter()
            .map(|r| r.session_id.as_str())
            .collect::<Vec<_>>(),
        [inference, math, sql.clone()]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert!(store.recover_workers().unwrap().is_empty());
    let trace = store.task(&task).unwrap();
    assert_eq!(trace["task"]["answer"], "42");
    let events = trace["events"].as_array().unwrap();
    assert!(events
        .iter()
        .any(|e| e["payload"]["session_id"] == sql && e["payload"]["charged_calls"] == 1));
    let record = events
        .iter()
        .find(|e| e["payload"]["session_id"] == sql && e["payload"]["state"] == "interrupted")
        .unwrap();
    assert_eq!(record["payload"]["capability"], "sql.query");
    assert_eq!(record["payload"]["automatic_replay"], false);
    assert_eq!(record["payload"]["resource_usage_complete"], false);
    assert_eq!(
        record["payload"]["external_effects_stopped_verified"],
        false
    );
    assert!(record["payload"].get("charged_calls").is_none());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn malformed_tool_metadata_or_audit_failure_rolls_back_mixed_recovery() {
    for case in 0..6 {
        let root = std::env::temp_dir().join(format!(
            "fabric-tool-recovery-denial-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Store::open(&root.join("state.db")).unwrap();
        let task = store
            .create_task("rollback worker recovery", "fast")
            .unwrap();
        worker_start(&store, &task, &uuid::Uuid::new_v4().to_string());
        let mut payload = serde_json::json!({"session_id":uuid::Uuid::new_v4().to_string(),"state":"started","charged_calls":0,"module_id":"sql.local","module_version":"1.0.0","capability":"sql.query","format_version":1});
        match case {
            0 => payload["capability"] = serde_json::json!("shell.execute"),
            1 => payload["charged_calls"] = serde_json::json!(1),
            2 => payload["format_version"] = serde_json::json!(2),
            3 => payload["padding"] = serde_json::json!("x".repeat(4096)),
            5 => payload["format_version"] = serde_json::Value::Null,
            _ => {
                store.lock().unwrap().execute_batch("CREATE TRIGGER fail_tool_recovery BEFORE INSERT ON events WHEN NEW.kind='tool_worker' AND json_extract(NEW.payload,'$.state')='interrupted' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
            }
        }
        store
            .event(&task, "tool_worker", "security-kernel", payload)
            .unwrap();
        assert!(store.recover_workers().is_err());
        let count: u32 = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE source='human-cli'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[tokio::test]
async fn live_arithmetic_and_sql_sessions_block_recovery_until_dropped() {
    use crate::fabric::tools::{self, Descriptor};
    use tokio::net::UnixStream;
    for sql in [false, true] {
        let root = std::env::temp_dir().join(format!(
            "fabric-live-tool-recovery-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(Store::open(&root.join("state.db")).unwrap());
        let task = store.create_task("live tool lease", "fast").unwrap();
        let descriptor = Descriptor {
            id: "tool.local".into(),
            version: "1.0.0".into(),
            capability: if sql { "sql.query" } else { "math.evaluate" }.into(),
        };
        let (_client, server) = UnixStream::pair().unwrap();
        let (_stop, receive) = tokio::sync::watch::channel(false);
        let worker_store = store.clone();
        let worker_task = task.clone();
        let uid = unsafe { libc::geteuid() };
        let worker = tokio::spawn(async move {
            if sql {
                tools::sql::unix::serve(
                    server,
                    tools::sql::unix::Grant::new(&worker_task, descriptor, 1, 30)?,
                    uid,
                    std::process::id(),
                    Arc::new(Idle),
                    worker_store,
                    receive,
                )
                .await
            } else {
                tools::unix::serve(
                    server,
                    tools::unix::Grant::new(&worker_task, descriptor, 1, 30)?,
                    uid,
                    std::process::id(),
                    Arc::new(Idle),
                    worker_store,
                    receive,
                )
                .await
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if !store.task(&task).unwrap()["events"]
                    .as_array()
                    .unwrap()
                    .is_empty()
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(store.recover_workers().is_err());
        worker.abort();
        assert!(worker.await.unwrap_err().is_cancelled());
        // Physical lease release can lag task cancellation under concurrent load.
        // Recovery must remain denied until every physical owner releases it.
        let recovered = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                match store.recover_workers() {
                    Ok(recovered) => break recovered,
                    Err(error) => {
                        assert!(
                            error.chain().any(|cause| cause
                                .downcast_ref::<std::io::Error>()
                                .is_some_and(|io| io.kind() == std::io::ErrorKind::WouldBlock)),
                            "Unexpected recovery failure: {error}"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                }
            }
        })
        .await
        .expect("Dropped worker lease must become recoverable");
        assert_eq!(recovered.len(), 1);
        assert!(store.recover_workers().unwrap().is_empty());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
