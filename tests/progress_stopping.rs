use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    reasoning::progress::Settings,
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1,2,3]},{"id":"b","domain":[0,1,2,3]},{"id":"c","domain":[0,1,2,3]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"b","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"c","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const FIRST: &str = r#"{"a":1,"b":0,"c":1}"#;
const SECOND: &str = r#"{"a":1,"b":2,"c":1}"#;
const PLATEAU: &str = r#"{"a":1,"b":3,"c":1}"#;
const GOOD: &str = r#"{"a":1,"b":1,"c":1}"#;

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
            .expect("Unexpected inference during original run");
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
                provider_metrics: None,
            })
        })
    }
}

fn config(endpoint: String) -> Config {
    Config {
        home: std::env::temp_dir().join(format!(
            "fabric-progress-continuation-{}",
            uuid::Uuid::new_v4()
        )),
        backend: "openai-local".into(),
        api_base: endpoint,
        interpret_goals: false,
        max_calls: 16,
        max_cycles: 4,
        max_tokens: 5000,
        max_tool_calls: 8,
        progress_stopping: Some(Settings { plateau_cycles: 2 }),
        ..Config::default()
    }
}

async fn serve_one(listener: tokio::net::TcpListener, answer: &'static str) {
    let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("timed out waiting for the resumed inference")
        .unwrap();
    let mut bytes = Vec::new();
    let request = loop {
        let mut chunk = [0u8; 4096];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .expect("timed out reading the resumed request")
            .unwrap();
        assert!(n > 0, "provider closed before sending a complete request");
        bytes.extend_from_slice(&chunk[..n]);
        assert!(
            bytes.len() < 64_000,
            "provider request exceeded fixture bound"
        );
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .expect("missing content-length")
                .trim()
                .parse::<usize>()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length]).unwrap();
            }
        }
    };
    assert_eq!(request["messages"][0]["role"], "system");
    assert_eq!(request["messages"][1]["role"], "user");
    let body = json!({"choices":[{"message":{"content":answer}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}).to_string();
    stream
        .write_all(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        ).as_bytes())
        .await
        .unwrap();
    drop(stream);

    // The resumed task must stop after this one solver response.
    assert!(
        tokio::time::timeout(Duration::from_millis(250), listener.accept())
            .await
            .is_err()
    );
}

fn progress_events(trace: &Value) -> Vec<(i64, Value)> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "reasoning_progress")
        .map(|event| {
            (
                event["sequence"].as_i64().unwrap(),
                event["payload"].clone(),
            )
        })
        .collect()
}

async fn resume_after_two_failed_candidates(answer: &'static str) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = config(format!("http://{}/v1", listener.local_addr().unwrap()));
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid graph"),
            ("explorer", "Try setting each variable to one"),
            ("solver", FIRST),
            ("solver", SECOND),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let original = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run(QUERY, "research")
        .await
        .unwrap();
    assert_eq!(original.status, "completed");
    assert!(script.0.lock().unwrap().is_empty());
    let task = original.task_id;

    // Keep the durable boundary written after cycle two, immediately before cycle three.
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let cutoff: i64 = db
        .query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.cycles_before')=2",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    let boundary: Value = db
        .query_row(
            "SELECT payload FROM events WHERE task_id=?1 AND id=?2",
            rusqlite::params![task, cutoff],
            |row| row.get::<_, String>(0),
        )
        .map(|raw| serde_json::from_str(&raw).unwrap())
        .unwrap();
    assert_eq!(boundary["progress_state"]["observed_cycles"], 2);
    assert_eq!(boundary["progress_state"]["best_score"], 1);
    let prior_calls = boundary["calls_before"].as_u64().unwrap();
    let prior_cycles = boundary["cycles_before"].as_u64().unwrap();
    let prior_tool_calls = boundary["tool_calls_before"].as_u64().unwrap();
    let prior_tokens = boundary["reserved_output_tokens"].as_u64().unwrap();
    let prior_prompt_bytes = boundary["charged_prompt_bytes"].as_u64().unwrap();

    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, cutoff],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [&task],
    )
    .unwrap();
    drop(db);

    cogfabric::recovery::continuation::preview(&store, &task).unwrap();
    let before = store.task(&task).unwrap();
    let prior_progress = progress_events(&before);
    assert_eq!(prior_progress.len(), 2);
    assert_eq!(prior_progress[0].1["score"], 1);
    assert_eq!(prior_progress[1].1["score"], 1);
    let prior_solvers = before["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| {
            event["kind"] == "reasoning" && event["payload"]["node"]["role"] == "solver"
        })
        .map(|event| event["payload"]["text"].clone())
        .collect::<Vec<_>>();
    assert_eq!(prior_solvers, vec![FIRST, SECOND]);

    let server = tokio::spawn(serve_one(listener, answer));
    let resumed = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    server.await.unwrap();

    assert_eq!(resumed["new_calls"], 1);
    assert_eq!(resumed["charged_resources"]["calls"], prior_calls + 1);
    assert_eq!(resumed["charged_resources"]["cycles"], prior_cycles + 1);
    assert_eq!(
        resumed["charged_resources"]["tool_calls"],
        prior_tool_calls + 1
    );
    assert_eq!(
        resumed["charged_resources"]["reserved_output_tokens"],
        prior_tokens + 384
    );
    assert!(
        resumed["charged_resources"]["prompt_bytes"]
            .as_u64()
            .unwrap()
            > prior_prompt_bytes
    );
    assert_eq!(resumed["verification_attempted"], true);

    let stopped = answer == PLATEAU;
    assert_eq!(
        resumed["status"],
        if stopped { "rejected" } else { "completed" }
    );
    assert_eq!(
        resumed["stop_reason"],
        if stopped { "no_progress" } else { "verified" }
    );
    assert_eq!(resumed["verified"], !stopped);

    let after = store.task(&task).unwrap();
    let progress = progress_events(&after);
    if stopped {
        assert_eq!(progress.len(), 3);
        assert_eq!(progress[..2], prior_progress);
        assert_eq!(progress[2].1["cycle"], 3);
        assert_eq!(progress[2].1["score"], 1);
        assert_eq!(progress[2].1["best_score"], 1);
        assert_eq!(progress[2].1["non_improving_cycles"], 2);
        assert_eq!(progress[2].1["stop"], true);
    } else {
        assert_eq!(progress, prior_progress);
        assert_eq!(progress.len(), 2);
    }
    let events = after["events"].as_array().unwrap();
    let solvers = events
        .iter()
        .filter(|event| {
            event["kind"] == "reasoning" && event["payload"]["node"]["role"] == "solver"
        })
        .map(|event| event["payload"]["text"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(solvers, vec![FIRST, SECOND, answer]);
    assert_eq!(
        events
            .iter()
            .filter(|event| event["kind"] == "reasoning"
                && event["payload"]["node"]["role"] == "explorer")
            .count(),
        1
    );
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}

async fn recover_after_interrupted_event(stage: &'static str) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = config(format!("http://{}/v1", listener.local_addr().unwrap()));
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid graph"),
            ("explorer", "Try setting each variable to one"),
            ("solver", FIRST),
            ("solver", SECOND),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let original = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run(QUERY, "research")
        .await
        .unwrap();
    assert_eq!(original.status, "completed");
    assert!(script.0.lock().unwrap().is_empty());
    let task = original.task_id;

    // Exercise crash windows on either side of the durable progress assessment.
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let verification_id: i64 = db
        .query_row(
            "SELECT id FROM events WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=2 ORDER BY id LIMIT 1",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    let runtime_verification: Value = db
        .query_row(
            "SELECT payload FROM events WHERE task_id=?1 AND id=?2",
            rusqlite::params![task, verification_id],
            |row| row.get::<_, String>(0),
        )
        .map(|raw| serde_json::from_str(&raw).unwrap())
        .unwrap();
    assert_eq!(runtime_verification["verified"], false);
    assert!(runtime_verification.get("origin").is_none());
    let cutoff: i64 = db
        .query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind=?2 AND json_extract(payload,'$.cycle')=2",
            rusqlite::params![task, stage],
            |row| row.get(0),
        )
        .unwrap();
    assert!(cutoff >= verification_id);
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, cutoff],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [&task],
    )
    .unwrap();
    drop(db);

    let first_recovery = store.recover_verified_result(&task).unwrap();
    assert_eq!(first_recovery["verified"], false);
    assert_eq!(first_recovery["revision_ready"], true);
    let second_recovery = store.recover_verified_result(&task).unwrap();
    assert_eq!(second_recovery["verified"], false);
    assert_eq!(second_recovery["revision_ready"], true);

    // The initial retrieval plus two original checks and two explicit rechecks are charged.
    let after_recovery = store.task(&task).unwrap();
    assert_eq!(progress_events(&after_recovery).len(), 2);
    let events = after_recovery["events"].as_array().unwrap();
    let cycle_two_verifications = events
        .iter()
        .filter(|event| {
            event["kind"] == "verification"
                && event["payload"]["cycle"] == 2
                && event["payload"]["criterion"] == "finite_domain/v1"
        })
        .collect::<Vec<_>>();
    assert_eq!(cycle_two_verifications.len(), 2);
    assert_eq!(cycle_two_verifications[0]["payload"], runtime_verification);
    assert!(cycle_two_verifications[0]["payload"]
        .get("origin")
        .is_none());
    assert_eq!(
        cycle_two_verifications[1]["payload"]["origin"],
        "saved_solver_recovery"
    );
    assert_eq!(cycle_two_verifications[1]["payload"]["verified"], false);
    assert_eq!(
        events
            .iter()
            .filter(|event| {
                event["kind"] == "reasoning_progress" && event["payload"]["cycle"] == 2
            })
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event["kind"] == "tool_reservation")
            .count(),
        5
    );
    cogfabric::recovery::continuation::preview(&store, &task).unwrap();
    let recovery_boundary: Value = store.task(&task).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|event| event["kind"] == "reasoning_boundary")
        .unwrap()["payload"]
        .clone();
    assert_eq!(recovery_boundary["progress_state"]["observed_cycles"], 2);

    let server = tokio::spawn(serve_one(listener, GOOD));
    let resumed = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(resumed["new_calls"], 1);
    assert_eq!(resumed["status"], "completed");
    assert_eq!(resumed["stop_reason"], "verified");
    assert_eq!(resumed["verified"], true);
    assert_eq!(resumed["charged_resources"]["calls"], 5);
    assert_eq!(resumed["charged_resources"]["cycles"], 3);
    assert_eq!(resumed["charged_resources"]["tool_calls"], 6);
    assert_eq!(
        resumed["charged_resources"]["reserved_output_tokens"],
        recovery_boundary["reserved_output_tokens"]
            .as_u64()
            .unwrap()
            + 384
    );
    let final_trace = store.task(&task).unwrap();
    let final_events = final_trace["events"].as_array().unwrap();
    let solvers = final_events
        .iter()
        .filter(|event| {
            event["kind"] == "reasoning" && event["payload"]["node"]["role"] == "solver"
        })
        .map(|event| event["payload"]["text"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(solvers, vec![FIRST, SECOND, GOOD]);
    assert_eq!(
        final_events
            .iter()
            .filter(|event| event["kind"] == "reasoning"
                && event["payload"]["node"]["role"] == "explorer")
            .count(),
        1
    );
    assert_eq!(progress_events(&final_trace).len(), 2);
    assert_eq!(
        final_events
            .iter()
            .filter(|event| {
                event["kind"] == "verification"
                    && event["payload"]["cycle"] == 2
                    && event["payload"]["origin"] == "saved_solver_recovery"
            })
            .count(),
        1
    );
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}

#[tokio::test]
async fn repeated_saved_result_recovery_reconciles_progress_once_before_resume() {
    recover_after_interrupted_event("verification").await;
    recover_after_interrupted_event("reasoning_progress").await;
}

#[tokio::test]
async fn continuation_restores_progress_history_and_stops_a_plateau_but_accepts_a_witness() {
    resume_after_two_failed_candidates(PLATEAU).await;
    resume_after_two_failed_candidates(GOOD).await;
}
