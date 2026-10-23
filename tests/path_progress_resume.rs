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

const QUERY: &str = r#"path:{"nodes":["s","a","b","c","g"],"edges":[["s","g"],["s","a"],["a","g"],["s","b"],["b","g"],["s","c"],["c","g"]],"start":"s","goal":"g"}"#;
const A: &str = r#"["s","a","g"]"#;
const B: &str = r#"["s","b","g"]"#;
const C: &str = r#"["s","c","g"]"#;
const GOOD: &str = r#"["s","g"]"#;

struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);

impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, answer) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected original path-progress inference");
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text: answer.into(),
                model: "pinned-path-fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}

fn config(endpoint: String, enabled: bool) -> Config {
    Config {
        home: std::env::temp_dir().join(format!(
            "fabric-path-progress-resume-{}",
            uuid::Uuid::new_v4()
        )),
        backend: "openai-local".into(),
        model: "pinned-original-model".into(),
        api_base: endpoint,
        interpret_goals: false,
        max_calls: 12,
        max_cycles: 4,
        max_tokens: 5000,
        max_tool_calls: 10,
        timeout_seconds: 120,
        progress_stopping: enabled.then_some(Settings { plateau_cycles: 2 }),
        ..Config::default()
    }
}

fn events<'a>(trace: &'a Value, kind: &str) -> Vec<&'a Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == kind)
        .collect()
}

fn remove_home(cfg: &Config) {
    let _ = std::fs::remove_dir_all(&cfg.home);
}

fn database(cfg: &Config) -> rusqlite::Connection {
    rusqlite::Connection::open(cfg.db()).unwrap()
}

fn interrupt_after(db: &rusqlite::Connection, task: &str, cutoff: i64) {
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, cutoff],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [task],
    )
    .unwrap();
}

async fn initial_checkpoint(cfg: &Config) -> (Arc<Store>, String, Value) {
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid graph"),
            ("explorer", "Try a direct path"),
            ("solver", A),
            ("solver", B),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run(QUERY, "research")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, GOOD);
    assert!(script.0.lock().unwrap().is_empty());
    let task = outcome.task_id;
    let trace = store.task(&task).unwrap();
    let progress = events(&trace, "reasoning_progress");
    assert_eq!(progress.len(), 2);
    assert_eq!(progress[0]["payload"]["score"], 1);
    assert_eq!(progress[1]["payload"]["non_improving_cycles"], 1);

    let db = database(cfg);
    let boundary_id: i64 = db
        .query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.cycles_before')=2",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    let boundary: Value = db
        .query_row(
            "SELECT payload FROM events WHERE task_id=?1 AND id=?2",
            rusqlite::params![task, boundary_id],
            |row| row.get::<_, String>(0),
        )
        .map(|raw| serde_json::from_str(&raw).unwrap())
        .unwrap();
    assert_eq!(boundary["path_progress_state"]["observed_cycles"], 2);
    assert_eq!(boundary["path_progress_state"]["best_score"], 1);
    assert_eq!(boundary["path_progress_state"]["non_improving_cycles"], 1);
    interrupt_after(&db, &task, boundary_id);
    drop(db);
    (store, task, boundary)
}

async fn serve_one(
    listener: tokio::net::TcpListener,
    expected_model: &'static str,
    answer: &'static str,
) {
    serve_answers(listener, expected_model, &[answer]).await;
}

async fn serve_answers(
    listener: tokio::net::TcpListener,
    expected_model: &'static str,
    answers: &[&'static str],
) {
    for answer in answers {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .expect("timed out waiting for resumed solver")
            .unwrap();
        let mut bytes = Vec::new();
        let request = loop {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
                .await
                .expect("timed out reading resumed request")
                .unwrap();
            assert!(n > 0, "provider closed before a complete request");
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 64_000, "request exceeded fixture bound");
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
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                        .unwrap();
                }
            }
        };
        assert_eq!(request["model"], expected_model);
        assert_eq!(request["max_tokens"], 384);
        let body = json!({
            "model":expected_model,
            "choices":[{"message":{"content":answer}}],
            "usage":{"prompt_tokens":1,"completion_tokens":1}
        })
        .to_string();
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        drop(stream);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(250), listener.accept())
            .await
            .is_err(),
        "continuation dispatched more than one solver call"
    );
}

async fn continue_checkpoint(answer: &'static str) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = config(
        format!("http://{}/v1", listener.local_addr().unwrap()),
        true,
    );
    let (store, task, boundary) = initial_checkpoint(&cfg).await;
    let prior_calls = boundary["calls_before"].as_u64().unwrap();
    let prior_cycles = boundary["cycles_before"].as_u64().unwrap();
    let prior_tools = boundary["tool_calls_before"].as_u64().unwrap();
    let prior_tokens = boundary["reserved_output_tokens"].as_u64().unwrap();
    let prior_prompt_bytes = boundary["charged_prompt_bytes"].as_u64().unwrap();
    cogfabric::recovery::continuation::preview(&store, &task).unwrap();

    let server = tokio::spawn(serve_one(listener, "pinned-original-model", answer));
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
    assert_eq!(resumed["charged_resources"]["tool_calls"], prior_tools + 1);
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
    let stopped = answer == C;
    assert_eq!(
        resumed["status"],
        if stopped { "rejected" } else { "completed" }
    );
    assert_eq!(
        resumed["stop_reason"],
        if stopped { "no_progress" } else { "verified" }
    );
    assert_eq!(resumed["verified"], !stopped);
    let trace = store.task(&task).unwrap();
    let progress = events(&trace, "reasoning_progress");
    assert_eq!(progress.len(), if stopped { 3 } else { 2 });
    assert_eq!(progress[0]["payload"]["cycle"], 1);
    assert_eq!(progress[1]["payload"]["cycle"], 2);
    if stopped {
        assert_eq!(progress[2]["payload"]["cycle"], 3);
        assert_eq!(progress[2]["payload"]["score"], 1);
        assert_eq!(progress[2]["payload"]["non_improving_cycles"], 2);
        assert_eq!(progress[2]["payload"]["stop"], true);
    } else {
        let proof = events(&trace, "verification")
            .into_iter()
            .find(|event| event["payload"]["verified"] == true)
            .unwrap();
        assert_eq!(proof["payload"]["cycle"], 3);
        assert_eq!(proof["payload"]["criterion"], "shortest_path/v1");
    }
    remove_home(&cfg);
}

#[tokio::test]
async fn continuation_restores_path_history_exact_scope_and_original_budgets() {
    continue_checkpoint(C).await;
    continue_checkpoint(GOOD).await;
}

enum Tamper {
    Score,
    MissingAssessment,
    AnswerHash,
    Source,
    Criterion,
    Snapshot,
    SettingsMarker,
    ContextMarker,
    DuplicateAssessment,
}

fn apply_tamper(db: &rusqlite::Connection, task: &str, tamper: Tamper) {
    match tamper {
        Tamper::Score => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.score',9) WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2",
                [task],
            )
            .unwrap();
        }
        Tamper::MissingAssessment => {
            db.execute(
                "DELETE FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2",
                [task],
            )
            .unwrap();
        }
        Tamper::AnswerHash => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.answer_hash',?2) WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2",
                rusqlite::params![task, "0".repeat(64)],
            )
            .unwrap();
        }
        Tamper::Source => {
            db.execute(
                "UPDATE events SET source='untrusted' WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2",
                [task],
            )
            .unwrap();
        }
        Tamper::Criterion => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.criterion','finite_domain/v1') WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2",
                [task],
            )
            .unwrap();
        }
        Tamper::Snapshot => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.path_progress_state.non_improving_cycles',0) WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.cycles_before')=2",
                [task],
            )
            .unwrap();
        }
        Tamper::SettingsMarker => {
            db.execute(
                "UPDATE events SET payload=json_remove(payload,'$.progress_policy') WHERE task_id=?1 AND kind='execution_settings'",
                [task],
            )
            .unwrap();
        }
        Tamper::ContextMarker => {
            db.execute(
                "UPDATE events SET payload=json_remove(payload,'$.progress_policy') WHERE task_id=?1 AND kind='reasoning_context'",
                [task],
            )
            .unwrap();
        }
        Tamper::DuplicateAssessment => {
            db.execute(
                "INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2",
                [task],
            )
            .unwrap();
        }
    }
}

#[tokio::test]
async fn tampered_path_history_is_rejected_without_writes_or_provider_dispatch() {
    for tamper in [
        Tamper::Score,
        Tamper::MissingAssessment,
        Tamper::AnswerHash,
        Tamper::Source,
        Tamper::Criterion,
        Tamper::Snapshot,
        Tamper::SettingsMarker,
        Tamper::ContextMarker,
        Tamper::DuplicateAssessment,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cfg = config(
            format!("http://{}/v1", listener.local_addr().unwrap()),
            true,
        );
        let (store, task, _) = initial_checkpoint(&cfg).await;
        apply_tamper(&database(&cfg), &task, tamper);
        let before = store.task(&task).unwrap();
        assert!(cogfabric::recovery::continuation::preview(&store, &task).is_err());
        assert_eq!(store.task(&task).unwrap(), before);
        assert!(cogfabric::recovery::continuation::resume(
            store.clone(),
            &task,
            true,
            std::future::pending(),
        )
        .await
        .is_err());
        assert_eq!(store.task(&task).unwrap(), before);
        assert!(
            tokio::time::timeout(Duration::from_millis(250), listener.accept())
                .await
                .is_err(),
            "tampered recovery attempted HTTP dispatch"
        );
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn repeated_saved_result_recovery_deduplicates_path_progress_but_keeps_fresh_charges() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = config(
        format!("http://{}/v1", listener.local_addr().unwrap()),
        true,
    );
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid graph"),
            ("explorer", "Try a direct path"),
            ("solver", A),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run(QUERY, "research")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert!(script.0.lock().unwrap().is_empty());
    let task = outcome.task_id;
    let db = database(&cfg);
    let progress_id: i64 = db
        .query_row(
            "SELECT id FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=1",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    interrupt_after(&db, &task, progress_id);
    drop(db);

    let original = store.task(&task).unwrap();
    assert_eq!(events(&original, "reasoning_progress").len(), 1);
    assert_eq!(events(&original, "verification").len(), 1);
    let first = store.recover_verified_result(&task).unwrap();
    assert_eq!(first["verified"], false);
    assert_eq!(first["revision_ready"], true);
    let after_first = store.task(&task).unwrap();
    assert_eq!(events(&after_first, "reasoning_progress").len(), 1);
    assert_eq!(
        events(&after_first, "verification")
            .iter()
            .filter(|event| event["payload"]["origin"] == "saved_solver_recovery")
            .count(),
        1
    );
    let reservations_after_first = events(&after_first, "tool_reservation").len();
    let second = store.recover_verified_result(&task).unwrap();
    assert_eq!(second["verified"], false);
    assert_eq!(second["revision_ready"], true);
    let after_second = store.task(&task).unwrap();
    assert_eq!(events(&after_second, "reasoning_progress").len(), 1);
    assert_eq!(
        events(&after_second, "verification")
            .iter()
            .filter(|event| event["payload"]["origin"] == "saved_solver_recovery")
            .count(),
        1
    );
    assert_eq!(
        events(&after_second, "tool_reservation").len(),
        reservations_after_first + 1,
        "each explicit recovery recheck keeps its fresh verification charge"
    );
    let ready = events(&after_second, "reasoning_boundary")
        .into_iter()
        .rfind(|event| event["payload"]["phase"] == "ready_solver")
        .unwrap();
    assert_eq!(
        ready["payload"]["path_progress_state"]["observed_cycles"],
        1
    );
    assert_eq!(ready["payload"]["path_progress_state"]["best_score"], 1);
    assert!(cogfabric::recovery::continuation::preview(&store, &task).is_ok());

    let resumed = tokio::spawn(serve_one(listener, "pinned-original-model", GOOD));
    let result = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    resumed.await.unwrap();
    assert_eq!(result["new_calls"], 1);
    assert_eq!(result["status"], "completed");
    assert_eq!(result["stop_reason"], "verified");
    let final_trace = store.task(&task).unwrap();
    assert_eq!(events(&final_trace, "reasoning_progress").len(), 1);
    assert_eq!(
        events(&final_trace, "verification")
            .iter()
            .filter(|event| event["payload"]["origin"] == "saved_solver_recovery")
            .count(),
        1
    );
    remove_home(&cfg);
}

#[tokio::test]
async fn unmarked_legacy_path_checkpoint_keeps_its_previous_continuation_rules() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = config(
        format!("http://{}/v1", listener.local_addr().unwrap()),
        false,
    );
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid graph"),
            ("explorer", "Try a direct path"),
            ("solver", A),
            ("solver", B),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run(QUERY, "research")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert!(script.0.lock().unwrap().is_empty());
    let task = outcome.task_id;
    let db = database(&cfg);
    let boundary_id: i64 = db
        .query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.cycles_before')=2",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    interrupt_after(&db, &task, boundary_id);
    drop(db);
    let before = store.task(&task).unwrap();
    assert!(events(&before, "reasoning_progress").is_empty());
    assert!(events(&before, "execution_settings")[0]["payload"]
        .get("progress_policy")
        .is_none());
    assert!(cogfabric::recovery::continuation::preview(&store, &task).is_ok());
    let server = tokio::spawn(serve_answers(listener, "pinned-original-model", &[C, C]));
    let resumed = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(resumed["new_calls"], 2);
    assert_eq!(resumed["status"], "rejected");
    assert!(events(&store.task(&task).unwrap(), "reasoning_progress").is_empty());
    remove_home(&cfg);
}
