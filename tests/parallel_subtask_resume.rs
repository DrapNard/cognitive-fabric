//! HTTP-backed recovery of durable parallel advisory waves.
use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    recovery::continuation,
    store::Store,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    future::Future,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1}]}"#;
// C is between A and B in declaration order but depends only on A. The ready
// wave partition is therefore [A,B], [C,D], whose first wave is not a prefix
// of the scheduler order [A,C,B,D].
const PLAN: &str = r#"{"steps":[{"id":"a","objective":"Subgoal A","depends_on":[]},{"id":"c","objective":"Subgoal C","depends_on":["a"]},{"id":"b","objective":"Subgoal B","depends_on":[]},{"id":"d","objective":"Subgoal D","depends_on":[]}] }"#;

#[derive(Clone, Debug)]
struct RecordedCall {
    role: String,
    prompt: String,
}

struct OriginalBackend(Mutex<Vec<RecordedCall>>);

impl InferenceBackend for OriginalBackend {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.0.lock().unwrap().push(RecordedCall {
            role: role.into(),
            prompt: prompt.into(),
        });
        let text = if role == "planner" {
            PLAN
        } else if prompt.contains("Subgoal A") {
            "result-a"
        } else if prompt.contains("Subgoal B") {
            "result-b"
        } else if prompt.contains("Subgoal C") {
            "result-c"
        } else if prompt.contains("Subgoal D") {
            "result-d"
        } else {
            r#"{"a":1}"#
        }
        .to_owned();
        Box::pin(async move {
            Ok(Generation {
                text,
                model: "parallel-resume-fixture/v1".into(),
                input_tokens: Some(2),
                output_tokens: Some(2),
                elapsed_ms: 0.1,
                queue_wait_ms: Some(0.0),
                provider_metrics: None,
            })
        })
    }
}

fn config(endpoint: String, home: std::path::PathBuf) -> Config {
    Config {
        home,
        backend: "openai-local".into(),
        model: "parallel-resume-fixture/v1".into(),
        api_base: endpoint,
        interpret_goals: false,
        max_calls: 10,
        max_cycles: 1,
        max_tokens: 2400,
        max_tool_calls: 8,
        timeout_seconds: 120,
        subtask_parallelism: Some(2),
        ..Config::default()
    }
}

struct Checkpoint {
    cfg: Config,
    store: Arc<Store>,
    task: String,
    original_settings: Value,
    marker: Value,
    listener: Option<TcpListener>,
}

async fn completed_checkpoint() -> Checkpoint {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let cfg = config(
        endpoint,
        std::env::temp_dir().join(format!("fabric-parallel-resume-{}", uuid::Uuid::new_v4())),
    );
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend = Arc::new(OriginalBackend(Mutex::new(Vec::new())));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), backend.clone())
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, r#"{"a":1}"#);
    let original_calls = backend.0.lock().unwrap();
    assert_eq!(original_calls.len(), 6);
    assert_eq!(original_calls[0].role, "planner");
    let final_prompt = &original_calls.last().unwrap().prompt;
    for expected in ["result-a", "result-b", "result-c", "result-d"] {
        assert!(final_prompt.contains(expected));
    }
    drop(original_calls);
    let trace = store.task(&outcome.task_id).unwrap();
    let settings = event(&trace, "execution_settings").clone();
    let marker = event(&trace, "subtask_parallel_schedule").clone();
    assert_eq!(marker["payload"]["order"], json!([0, 1, 2, 3]));
    assert_eq!(marker["payload"]["waves"], json!([[0, 2], [1, 3]]));
    assert_eq!(events(&trace, "subtask_resources").len(), 4);
    Checkpoint {
        cfg,
        store,
        task: outcome.task_id,
        original_settings: settings,
        marker,
        listener: Some(listener),
    }
}

fn event<'a>(trace: &'a Value, kind: &str) -> &'a Value {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == kind)
        .unwrap_or_else(|| panic!("missing {kind} event"))
}

fn events<'a>(trace: &'a Value, kind: &str) -> Vec<&'a Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == kind)
        .collect()
}

fn lock_path(cfg: &Config) -> std::path::PathBuf {
    let mut path = cfg.db().into_os_string();
    path.push(".runtime.lock");
    path.into()
}

/// Rewind only while holding the same exclusive flock required by public recovery.
fn crash_at(cfg: &Config, store: &Store, task: &str, cutoff: i64) {
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path(cfg))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    {
        let db = Connection::open(cfg.db()).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            rusqlite::params![task, cutoff],
        )
        .unwrap();
        db.execute(
            "UPDATE tasks SET status='running',answer='' WHERE id=?1",
            [task],
        )
        .unwrap();
    }
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN) }, 0);
    drop(lock);
    assert_eq!(store.recover_tasks().unwrap(), vec![task.to_owned()]);
}

fn cutoff_after_wave(cfg: &Config, task: &str, ids: &[&str]) -> i64 {
    let db = Connection::open(cfg.db()).unwrap();
    db.query_row(
        "SELECT max(id) FROM events WHERE task_id=?1 AND kind='subtask_resources' AND json_extract(payload,'$.state')='unverified' AND json_extract(payload,'$.node_id') IN (?2,?3,?4,?5)",
        rusqlite::params![task, ids.first().copied().unwrap_or(""), ids.get(1).copied().unwrap_or(""), ids.get(2).copied().unwrap_or(""), ids.get(3).copied().unwrap_or("")],
        |row| row.get(0),
    ).unwrap()
}

fn truncate_after_completed_ids(checkpoint: &Checkpoint, ids: &[&str]) {
    let cutoff = cutoff_after_wave(&checkpoint.cfg, &checkpoint.task, ids);
    assert!(cutoff > 0);
    crash_at(&checkpoint.cfg, &checkpoint.store, &checkpoint.task, cutoff);
}

fn truncate_before_marker(checkpoint: &Checkpoint) {
    let db = Connection::open(checkpoint.cfg.db()).unwrap();
    let cutoff: i64 = db.query_row(
        "SELECT max(id) FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.state')='pending'",
        [&checkpoint.task],
        |row| row.get(0),
    ).unwrap();
    crash_at(&checkpoint.cfg, &checkpoint.store, &checkpoint.task, cutoff);
}

fn truncate_at_ready_solver(checkpoint: &Checkpoint) {
    let db = Connection::open(checkpoint.cfg.db()).unwrap();
    let cutoff: i64 = db
        .query_row(
            "SELECT id FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.phase')='ready_solver' ORDER BY id LIMIT 1",
            [&checkpoint.task],
            |row| row.get(0),
        )
        .unwrap();
    crash_at(&checkpoint.cfg, &checkpoint.store, &checkpoint.task, cutoff);
}

async fn read_request(stream: &mut TcpStream) -> Value {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let count = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .expect("timed out reading provider request")
            .unwrap();
        assert!(count > 0, "provider closed before the request completed");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(
            bytes.len() <= 64_000,
            "provider request exceeded fixture bound"
        );
        let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
        let length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .expect("request omitted content length")
            .trim()
            .parse::<usize>()
            .unwrap();
        if bytes.len() >= end + 4 + length {
            return serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
        }
    }
}

async fn accept_request(listener: &TcpListener) -> (TcpStream, Value) {
    let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("timed out waiting for model dispatch")
        .unwrap();
    let request = read_request(&mut stream).await;
    assert_eq!(request["model"], "parallel-resume-fixture/v1");
    (stream, request)
}

async fn respond(stream: &mut TcpStream, answer: &str) {
    let body = json!({
        "model":"parallel-resume-fixture/v1",
        "choices":[{"message":{"content":answer}}],
        "usage":{"prompt_tokens":2,"completion_tokens":2}
    })
    .to_string();
    stream.write_all(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    ).as_bytes()).await.unwrap();
}

fn prompt(request: &Value) -> &str {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|message| message["content"].as_str())
        .next_back()
        .unwrap()
}

async fn serve_remainder(listener: TcpListener) {
    let (mut first, req_first) = accept_request(&listener).await;
    let (mut second, req_second) = accept_request(&listener).await;
    let prompts = [prompt(&req_first), prompt(&req_second)];
    assert!(prompts.iter().any(|value| value.contains("Subgoal C")));
    assert!(prompts.iter().any(|value| value.contains("Subgoal D")));
    // C depends on A and must receive its completed-wave result. B and D were
    // still pending at the crash and must not appear as saved results.
    assert!(prompts.iter().any(|value| value.contains("result-a")));
    assert!(prompts.iter().all(|value| !value.contains("result-b")));
    respond(&mut second, "result-d-resumed").await;
    respond(&mut first, "result-c-resumed").await;

    let (mut solver, request) = accept_request(&listener).await;
    let content = prompt(&request);
    for expected in [
        "result-a",
        "result-b",
        "result-c-resumed",
        "result-d-resumed",
    ] {
        assert!(
            content.contains(expected),
            "final solver omitted {expected}: {content}"
        );
    }
    respond(&mut solver, r#"{"a":1}"#).await;
}

async fn serve_all_after_initialization(listener: TcpListener) {
    let (mut first, req_first) = accept_request(&listener).await;
    let (mut second, req_second) = accept_request(&listener).await;
    let first_prompts = [prompt(&req_first), prompt(&req_second)];
    assert!(first_prompts
        .iter()
        .any(|value| value.contains("Subgoal A")));
    assert!(first_prompts
        .iter()
        .any(|value| value.contains("Subgoal B")));
    respond(&mut second, "result-b-resumed").await;
    respond(&mut first, "result-a-resumed").await;

    let (mut third, req_third) = accept_request(&listener).await;
    let (mut fourth, req_fourth) = accept_request(&listener).await;
    let second_prompts = [prompt(&req_third), prompt(&req_fourth)];
    assert!(second_prompts
        .iter()
        .any(|value| value.contains("Subgoal C")));
    assert!(second_prompts
        .iter()
        .any(|value| value.contains("Subgoal D")));
    assert!(second_prompts
        .iter()
        .any(|value| value.contains("result-a-resumed")));
    respond(&mut fourth, "result-d-resumed").await;
    respond(&mut third, "result-c-resumed").await;

    let (mut solver, request) = accept_request(&listener).await;
    let content = prompt(&request);
    for expected in [
        "result-a-resumed",
        "result-b-resumed",
        "result-c-resumed",
        "result-d-resumed",
    ] {
        assert!(
            content.contains(expected),
            "final solver omitted {expected}: {content}"
        );
    }
    respond(&mut solver, r#"{"a":1}"#).await;
}

#[tokio::test]
async fn resumes_only_pending_complete_wave_and_checks_original_criterion() {
    let mut checkpoint = completed_checkpoint().await;
    truncate_after_completed_ids(&checkpoint, &["a", "b"]);
    let preview = continuation::preview(&checkpoint.store, &checkpoint.task).unwrap();
    assert_eq!(preview["checkpoint_eligible"], true);
    assert_eq!(preview["parallel_history"]["completed_waves"], 1);
    assert_eq!(preview["parallel_history"]["total_waves"], 2);
    assert_eq!(preview["parallel_history"]["permissions_granted"], false);
    assert_eq!(preview["parallel_history"]["results_verified"], false);
    assert_eq!(preview["parallel_history"]["automatic_replay"], false);
    assert_eq!(preview["model"], "parallel-resume-fixture/v1");
    assert_eq!(
        preview["subtasks"],
        json!([
            {"id":"a","state":"unverified"},
            {"id":"c","state":"pending"},
            {"id":"b","state":"unverified"},
            {"id":"d","state":"pending"}
        ])
    );
    assert_eq!(checkpoint.marker["payload"]["order"], json!([0, 1, 2, 3]));
    assert_eq!(
        checkpoint.marker["payload"]["waves"],
        json!([[0, 2], [1, 3]])
    );
    let listener = checkpoint.listener.take().unwrap();
    let server = tokio::spawn(serve_remainder(listener));
    let report = continuation::resume(
        checkpoint.store.clone(),
        &checkpoint.task,
        true,
        std::future::pending::<()>(),
    )
    .await
    .unwrap();
    assert_eq!(
        report["status"],
        "completed",
        "resume report: {report}; trace: {}",
        checkpoint.store.task(&checkpoint.task).unwrap()
    );
    server.await.unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["verified"], true);
    assert_eq!(report["criterion"], "finite_domain/v1");
    assert_eq!(report["new_calls"], 3);
    assert_eq!(report["charged_resources"]["calls"], 6);
    assert_eq!(report["charged_resources"]["reserved_output_tokens"], 1408);
    assert_eq!(report["parallel_history"]["completed_waves"], 1);
    assert_eq!(report["parallel_history"]["total_waves"], 2);
    let after = checkpoint.store.task(&checkpoint.task).unwrap();
    assert_eq!(
        event(&after, "execution_settings"),
        &checkpoint.original_settings
    );
    assert_eq!(
        event(&after, "subtask_parallel_schedule"),
        &checkpoint.marker
    );
    assert_eq!(events(&after, "subtask_parallel_schedule").len(), 1);
    assert_eq!(events(&after, "model_dispatch").len(), 6);
    assert_eq!(
        event(&after, "reasoning_boundary")["payload"]["phase"],
        "ready_solver"
    );
    let continuation_events = events(&after, "continuation");
    assert_eq!(continuation_events.len(), 2);
    assert_eq!(
        continuation_events[0]["payload"]["parallel_history"]["completed_waves"],
        1
    );
    assert_eq!(
        continuation_events[1]["payload"]["parallel_history"]["automatic_replay"],
        false
    );
    let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
}

#[tokio::test]
async fn ready_final_solver_resumes_after_every_parallel_wave() {
    let mut checkpoint = completed_checkpoint().await;
    truncate_at_ready_solver(&checkpoint);
    let preview = continuation::preview(&checkpoint.store, &checkpoint.task).unwrap();
    assert_eq!(preview["phase"], "ready_solver");
    assert_eq!(preview["parallel_history"]["completed_waves"], 2);
    let listener = checkpoint.listener.take().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, request) = accept_request(&listener).await;
        let content = prompt(&request);
        for expected in ["result-a", "result-b", "result-c", "result-d"] {
            assert!(
                content.contains(expected),
                "final solver omitted {expected}: {content}"
            );
        }
        respond(&mut stream, r#"{"a":1}"#).await;
    });
    let report = continuation::resume(
        checkpoint.store.clone(),
        &checkpoint.task,
        true,
        std::future::pending::<()>(),
    )
    .await
    .unwrap();
    assert_eq!(
        report["status"],
        "completed",
        "resume report: {report}; trace: {}",
        checkpoint.store.task(&checkpoint.task).unwrap()
    );
    server.await.unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["verified"], true);
    assert_eq!(report["new_calls"], 1);
    let trace = checkpoint.store.task(&checkpoint.task).unwrap();
    assert_eq!(events(&trace, "subtask_parallel_schedule").len(), 1);
    assert_eq!(events(&trace, "model_dispatch").len(), 6);
    let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
}

#[tokio::test]
async fn restores_the_safe_pre_marker_initialization_window_without_replaying_planner() {
    let mut checkpoint = completed_checkpoint().await;
    truncate_before_marker(&checkpoint);
    let before = checkpoint.store.task(&checkpoint.task).unwrap();
    assert!(events(&before, "subtask_parallel_schedule").is_empty());
    assert_eq!(events(&before, "model_dispatch").len(), 1);
    let preview = continuation::preview(&checkpoint.store, &checkpoint.task).unwrap();
    assert_eq!(preview["checkpoint_eligible"], true);
    assert!(preview["parallel_history"].is_null());
    assert_eq!(
        preview["subtasks"],
        json!([
            {"id":"a","state":"pending"},
            {"id":"c","state":"pending"},
            {"id":"b","state":"pending"},
            {"id":"d","state":"pending"}
        ])
    );
    let listener = checkpoint.listener.take().unwrap();
    let server = tokio::spawn(serve_all_after_initialization(listener));
    let report = continuation::resume(
        checkpoint.store.clone(),
        &checkpoint.task,
        true,
        std::future::pending::<()>(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["verified"], true);
    assert_eq!(report["new_calls"], 5);
    let after = checkpoint.store.task(&checkpoint.task).unwrap();
    assert_eq!(events(&after, "model_dispatch").len(), 6);
    assert_eq!(events(&after, "subtask_parallel_schedule").len(), 1);
    assert_eq!(
        events(&after, "continuation")[0]["payload"]["parallel_history"],
        Value::Null
    );
    let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
}

fn tamper(checkpoint: &Checkpoint, kind: &str) {
    let db = Connection::open(checkpoint.cfg.db()).unwrap();
    match kind {
        "wave" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.waves',json('[ [0,1], [2,3] ]')) WHERE task_id=?1 AND kind='subtask_parallel_schedule'", [&checkpoint.task]).unwrap();
        }
        "result" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.node.result','altered-result') WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='a' AND json_extract(payload,'$.node.state')='unverified'", [&checkpoint.task]).unwrap();
        }
        "call" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.call',999) WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.text')='result-a'", [&checkpoint.task]).unwrap();
        }
        "reservation" => {
            db.execute("DELETE FROM events WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='model_dispatch' AND json_extract(payload,'$.role')='solver' ORDER BY id LIMIT 1 OFFSET 1)", [&checkpoint.task]).unwrap();
        }
        "marker" => {
            db.execute(
                "DELETE FROM events WHERE task_id=?1 AND kind='subtask_parallel_schedule'",
                [&checkpoint.task],
            )
            .unwrap();
        }
        "running" => {
            let node: String = db.query_row("SELECT payload FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='c' AND json_extract(payload,'$.node.state')='pending' ORDER BY id DESC LIMIT 1", [&checkpoint.task], |row| row.get(0)).unwrap();
            let mut payload: Value = serde_json::from_str(&node).unwrap();
            payload["node"]["state"] = json!("running");
            db.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'subtask','jev-plus',?2,unixepoch())", rusqlite::params![checkpoint.task, payload.to_string()]).unwrap();
        }
        "incomplete" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.node.state','pending','$.node.result',NULL) WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='b' AND json_extract(payload,'$.node.state')='unverified'", [&checkpoint.task]).unwrap();
        }
        "deadline" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.execution_clock.deadline_ns',1) WHERE task_id=?1 AND kind='execution_settings'", [&checkpoint.task]).unwrap();
        }
        "budget" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.configuration.max_calls',4,'$.call_limit',4) WHERE task_id=?1 AND kind='execution_settings'", [&checkpoint.task]).unwrap();
        }
        _ => panic!("unknown tampering fixture"),
    }
}

#[tokio::test]
async fn altered_wave_result_call_reservation_state_deadline_and_budget_are_refused_read_only() {
    for kind in [
        "wave",
        "result",
        "call",
        "reservation",
        "marker",
        "running",
        "incomplete",
        "deadline",
        "budget",
    ] {
        let checkpoint = completed_checkpoint().await;
        truncate_after_completed_ids(&checkpoint, &["a", "b"]);
        tamper(&checkpoint, kind);
        let before = checkpoint.store.task(&checkpoint.task).unwrap();
        assert!(
            continuation::preview(&checkpoint.store, &checkpoint.task).is_err(),
            "{kind} preview must fail closed"
        );
        assert!(
            continuation::resume(checkpoint.store.clone(), &checkpoint.task, true, async {})
                .await
                .is_err(),
            "{kind} resume must fail closed"
        );
        let after = checkpoint.store.task(&checkpoint.task).unwrap();
        assert_eq!(
            after, before,
            "{kind} refusal must not change task state or events"
        );
        assert!(
            tokio::time::timeout(
                Duration::from_millis(100),
                checkpoint.listener.as_ref().unwrap().accept()
            )
            .await
            .is_err(),
            "{kind} refusal must not reach the HTTP backend"
        );
        let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
    }
}

#[tokio::test]
async fn continuation_audit_rollback_happens_before_any_http_dispatch() {
    let checkpoint = completed_checkpoint().await;
    truncate_after_completed_ids(&checkpoint, &["a", "b"]);
    let db = Connection::open(checkpoint.cfg.db()).unwrap();
    db.execute_batch(
        "CREATE TRIGGER refuse_parallel_continuation BEFORE INSERT ON events
         WHEN NEW.kind='continuation' AND json_extract(NEW.payload,'$.state')='started'
         BEGIN SELECT RAISE(ABORT,'reject continuation audit'); END;",
    )
    .unwrap();
    let before = checkpoint.store.task(&checkpoint.task).unwrap();
    let error = continuation::resume(checkpoint.store.clone(), &checkpoint.task, true, async {})
        .await
        .unwrap_err();
    assert!(error.to_string().contains("reject continuation audit"));
    assert_eq!(checkpoint.store.task(&checkpoint.task).unwrap(), before);
    assert!(tokio::time::timeout(
        Duration::from_millis(100),
        checkpoint.listener.as_ref().unwrap().accept()
    )
    .await
    .is_err());
    drop(db);
    let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
}
