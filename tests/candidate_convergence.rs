use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    store::Store,
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const QUERY: &str = r#"sort:{"values":[3,2,1]}"#;
const A: &str = "[3,2,1]";
const B: &str = "[2,3,1]";
const GOOD: &str = "[1,2,3]";
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
            .expect("Unexpected inference");
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}
fn config() -> Config {
    Config {
        home: std::env::temp_dir()
            .join(format!("fabric-candidate-history-{}", uuid::Uuid::new_v4())),
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 16,
        max_cycles: 4,
        max_tokens: 5000,
        max_tool_calls: 8,
        ..Config::default()
    }
}
#[tokio::test]
async fn resumed_solver_remembers_nonadjacent_failed_candidates_and_preserves_rejected_status() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut cfg = config();
    cfg.api_base = format!("http://{}/v1", listener.local_addr().unwrap());
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid graph"),
            ("solver", A),
            ("solver", B),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let original = Engine::with_backend(cfg.clone(), store.clone(), backend)
        .run(QUERY, "deep")
        .await
        .unwrap();
    assert_eq!(original.status, "completed");
    let task = original.task_id;
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let boundary: u64 = db
        .query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary'",
            [&task],
            |r| r.get(0),
        )
        .unwrap();
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, boundary],
    )
    .unwrap();
    // Legacy records need no new stopping-state field; original nodes suffice.
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND kind='candidate_progress'",
        [&task],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [&task],
    )
    .unwrap();
    drop(db);
    assert!(cogfabric::recovery::continuation::preview(&store, &task).is_ok());
    let provider = tokio::spawn(async move {
        let (mut stream, _) =
            tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0u8; 2048];
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 64000);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let body=serde_json::json!({"choices":[{"message":{"content":A}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}).to_string();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
    });
    let result = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    provider.await.unwrap();
    assert_eq!(result["status"], "rejected");
    assert_eq!(result["stop_reason"], "no_progress");
    assert_eq!(result["verification_attempted"], true);
    assert_eq!(result["verified"], false);
    assert_eq!(result["new_calls"], 1);
    let trace = store.task(&task).unwrap();
    let progress = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "candidate_progress")
        .unwrap();
    assert_eq!(
        progress["payload"]["previous_cycles"],
        serde_json::json!([1])
    );
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
#[tokio::test]
async fn revision_reconciliation_cannot_restart_an_already_recurrent_prose_search() {
    let cfg = config();
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let answers = vec![
        ("planner", "invalid graph"),
        ("explorer", "Alternative"),
        ("solver", "A"),
        (
            "critic",
            r#"{"revise":true,"reason":"Try another approach"}"#,
        ),
        ("reflector", "B"),
        ("solver", "B"),
        ("critic", r#"{"revise":true,"reason":"Try again"}"#),
        ("reflector", "A"),
        ("solver", "A"),
    ];
    let original = Engine::with_backend(
        cfg.clone(),
        store.clone(),
        Arc::new(Script(Mutex::new(answers.into()))),
    )
    .run("Explain the original objective", "research")
    .await
    .unwrap();
    assert_eq!(original.status, "unverified");
    assert_eq!(original.stop_reason, "no_progress");
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted' WHERE id=?1",
        [&original.task_id],
    )
    .unwrap();
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND kind='candidate_progress'",
        [&original.task_id],
    )
    .unwrap();
    drop(db);
    let before = store.task(&original.task_id).unwrap();
    let error = store
        .recover_revision_result(&original.task_id, true)
        .unwrap_err();
    assert!(error.to_string().contains("already recur"), "{error:#}");
    assert_eq!(store.task(&original.task_id).unwrap(), before);
    let trace: Value = store.task(&original.task_id).unwrap();
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "revision_recovery"));
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
