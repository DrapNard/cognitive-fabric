use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    reasoning::{
        finite_search::Settings as SearchSettings, progress::Settings as ProgressSettings,
    },
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

const QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const WITNESS: &str = r#"{"a":1}"#;
const BAD: &str = r#"{"a":0}"#;
const THREE_VARIABLE_QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1,2,3]},{"id":"b","domain":[0,1,2,3]},{"id":"c","domain":[0,1,2,3]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"b","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"c","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const REJECTED: &str = r#"{"a":1,"b":0,"c":1}"#;
const GOOD: &str = r#"{"a":1,"b":1,"c":1}"#;
const PLAN_TWO: &str = r#"{"steps":[{"id":"a","objective":"Inspect the first result","depends_on":[]},{"id":"b","objective":"Review the next result","depends_on":["a"]}]}"#;

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
            .unwrap_or_else(|| panic!("unexpected {role} inference in recovery fixture"));
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "fixture-model".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}

fn config(endpoint: String, progress: bool) -> Config {
    Config {
        home: std::env::temp_dir().join(format!(
            "fabric-finite-search-result-recovery-{}",
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
        finite_search: Some(SearchSettings { max_nodes: 1 }),
        progress_stopping: progress.then_some(ProgressSettings { plateau_cycles: 2 }),
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

fn db(cfg: &Config) -> rusqlite::Connection {
    rusqlite::Connection::open(cfg.db()).unwrap()
}

fn interrupt_at(db: &rusqlite::Connection, task: &str, cutoff: i64) {
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

fn solver_result_id(db: &rusqlite::Connection, task: &str, cycle: Option<u64>) -> i64 {
    let cycle_filter = cycle.map_or(String::new(), |cycle| {
        format!(" AND json_extract(payload,'$.node.cycle')={cycle}")
    });
    db.query_row(
        &format!("SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver'{cycle_filter}"),
        [task],
        |row| row.get(0),
    )
    .unwrap()
}

async fn run_fast(answer: &'static str) -> (Config, Arc<Store>, String) {
    let cfg = config("http://127.0.0.1:1/v1".into(), false);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(vec![("solver", answer)].into())));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run(QUERY, "fast")
        .await
        .unwrap();
    assert!(script.0.lock().unwrap().is_empty());
    let trace = store.task(&outcome.task_id).unwrap();
    assert_eq!(events(&trace, "finite_search").len(), 1);
    assert_eq!(
        events(&trace, "finite_search")[0]["payload"]["outcome"],
        "limit_reached"
    );
    assert_eq!(outcome.calls, 1);
    let task = outcome.task_id;
    let solver_id = solver_result_id(&db(&cfg), &task, Some(1));
    interrupt_at(&db(&cfg), &task, solver_id);
    (cfg, store, task)
}

async fn run_research(progress: bool, rewind_after_first: bool) -> (Config, Arc<Store>, String) {
    let cfg = config("http://127.0.0.1:1/v1".into(), progress);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid plan"),
            ("explorer", "Try changing b"),
            ("solver", REJECTED),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run_profile(
            THREE_VARIABLE_QUERY,
            "research",
            ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert!(script.0.lock().unwrap().is_empty());
    let task = outcome.task_id;
    let db = db(&cfg);
    assert_eq!(
        events(&store.task(&task).unwrap(), "finite_search").len(),
        1
    );
    if rewind_after_first {
        let first_solver = db.query_row(
            "SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver' AND json_extract(payload,'$.node.cycle')=1",
            [&task],
            |row| row.get::<_, i64>(0),
        ).unwrap();
        interrupt_at(&db, &task, first_solver);
    }
    drop(db);
    (cfg, store, task)
}

async fn serve_one(listener: tokio::net::TcpListener, answer: &'static str) {
    serve_answers(listener, vec![(384, answer)]).await;
}

async fn serve_answers(listener: tokio::net::TcpListener, answers: Vec<(u64, &'static str)>) {
    for (max_tokens, answer) in answers {
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
            assert!(
                bytes.len() < 64_000,
                "provider request exceeded fixture bound"
            );
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .expect("missing request content-length")
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                        .unwrap();
                }
            }
        };
        assert_eq!(request["model"], "pinned-original-model");
        assert_eq!(request["max_tokens"], max_tokens);
        let body = json!({
            "model":"pinned-original-model",
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
            .is_err()
    );
}

#[tokio::test]
async fn limit_receipt_recovery_rechecks_saved_witness_without_search_or_inference_replay() {
    let (cfg, store, task) = run_fast(WITNESS).await;
    let before = store.task(&task).unwrap();
    assert!(events(&before, "verification").is_empty());
    let search = events(&before, "finite_search");
    let search_charge = events(&before, "tool_reservation")
        .into_iter()
        .filter(|event| event["payload"]["capability"] == "constraints.solve")
        .count();
    assert_eq!(search_charge, 1);
    let initial_calls = store.inference_audit(&task).unwrap()["reserved_calls"]
        .as_u64()
        .unwrap();
    let initial_tools = events(&before, "tool_reservation").len();

    let report = store.recover_verified_result(&task).unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["verified"], true);
    assert_eq!(report["inference_repeated"], false);
    assert_eq!(report["finite_search_history"]["outcome"], "limit_reached");
    assert_eq!(report["finite_search_history"]["automatic_replay"], false);
    assert_eq!(report["finite_search_history"]["verified"], false);
    assert_eq!(
        report["finite_search_history"]["permissions_granted"],
        false
    );

    let after = store.task(&task).unwrap();
    assert_eq!(events(&after, "finite_search").len(), 1);
    assert_eq!(events(&after, "finite_search")[0], search[0]);
    assert_eq!(
        events(&after, "tool_reservation")
            .into_iter()
            .filter(|event| event["payload"]["capability"] == "constraints.solve")
            .count(),
        1
    );
    assert_eq!(events(&after, "tool_reservation").len(), initial_tools + 1);
    assert_eq!(
        store.inference_audit(&task).unwrap()["reserved_calls"],
        initial_calls
    );
    assert_eq!(events(&after, "verification").len(), 1);
    assert_eq!(
        events(&after, "verification")[0]["payload"]["origin"],
        "saved_solver_recovery"
    );
    assert_eq!(store.task(&task).unwrap()["task"]["answer"], WITNESS);
    remove_home(&cfg);
}

#[tokio::test]
async fn invalid_limit_receipts_are_rejected_before_recovery_writes_or_charges() {
    for tamper in ["objective", "outcome", "missing", "duplicate", "source"] {
        let (cfg, store, task) = run_fast(WITNESS).await;
        let db = db(&cfg);
        match tamper {
            "objective" => db.execute(
                "UPDATE events SET payload=json_set(payload,'$.objective_sha256','forged') WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "outcome" => db.execute(
                "UPDATE events SET payload=json_set(payload,'$.outcome','witness') WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "missing" => db.execute(
                "DELETE FROM events WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "duplicate" => db.execute(
                "INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "source" => db.execute(
                "UPDATE events SET source='attacker' WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            _ => unreachable!(),
        };
        drop(db);
        let before = store.task(&task).unwrap();
        assert!(
            store.recover_verified_result(&task).is_err(),
            "tamper={tamper}"
        );
        assert_eq!(store.task(&task).unwrap(), before, "tamper={tamper}");
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn failed_fresh_verification_audit_rolls_back_its_search_task_charge() {
    let (cfg, store, task) = run_fast(WITNESS).await;
    rusqlite::Connection::open(cfg.db())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_search_result_verification BEFORE INSERT ON events WHEN NEW.kind='verification' BEGIN SELECT RAISE(ABORT,'fixture verifier audit failure'); END;",
        )
        .unwrap();
    let before = store.task(&task).unwrap();
    let charges = events(&before, "tool_reservation").len();
    assert!(store.recover_verified_result(&task).is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    assert_eq!(
        events(&store.task(&task).unwrap(), "tool_reservation").len(),
        charges
    );
    rusqlite::Connection::open(cfg.db())
        .unwrap()
        .execute_batch("DROP TRIGGER reject_search_result_verification")
        .unwrap();
    let report = store.recover_verified_result(&task).unwrap();
    assert_eq!(report["verified"], true);
    remove_home(&cfg);
}

#[tokio::test]
async fn rejected_search_result_recovery_reconciles_progress_and_resumes_one_solver() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let cfg = config(endpoint, true);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid plan"),
            ("explorer", "Try changing b"),
            ("solver", REJECTED),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let original = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run_profile(
            THREE_VARIABLE_QUERY,
            "research",
            ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(original.status, "completed");
    assert!(script.0.lock().unwrap().is_empty());
    let task = original.task_id;
    let db = db(&cfg);
    let first_solver = db.query_row(
        "SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver' AND json_extract(payload,'$.node.cycle')=1",
        [&task],
        |row| row.get::<_, i64>(0),
    ).unwrap();
    interrupt_at(&db, &task, first_solver);
    drop(db);

    let initial = store.task(&task).unwrap();
    let original_calls = store.inference_audit(&task).unwrap()["reserved_calls"]
        .as_u64()
        .unwrap();
    let original_tool_count = events(&initial, "tool_reservation").len();
    assert_eq!(events(&initial, "verification").len(), 0);
    for _ in 0..2 {
        let report = store.recover_verified_result(&task).unwrap();
        assert_eq!(report["verified"], false);
        assert_eq!(report["revision_ready"], true);
        assert_eq!(report["finite_search_history"]["outcome"], "limit_reached");
    }
    let reconciled = store.task(&task).unwrap();
    assert_eq!(events(&reconciled, "reasoning_progress").len(), 1);
    assert_eq!(
        events(&reconciled, "reasoning_progress")[0]["payload"]["cycle"],
        1
    );
    assert_eq!(
        events(&reconciled, "reasoning_progress")[0]["payload"]["score"],
        1
    );
    assert_eq!(
        events(&reconciled, "verification")
            .iter()
            .filter(|event| event["payload"]["origin"] == "saved_solver_recovery")
            .count(),
        1
    );
    assert_eq!(
        events(&reconciled, "tool_reservation").len(),
        original_tool_count + 2
    );
    assert_eq!(
        store.inference_audit(&task).unwrap()["reserved_calls"],
        original_calls
    );
    let ready: Vec<_> = events(&reconciled, "reasoning_boundary")
        .into_iter()
        .filter(|event| event["payload"]["phase"] == "ready_solver")
        .collect();
    assert_eq!(ready.len(), 3); // Original boundary plus one fresh-charge boundary per check.
    assert_eq!(
        ready[1]["payload"]["tool_calls_before"],
        original_tool_count + 1
    );
    assert_eq!(
        ready[2]["payload"]["tool_calls_before"],
        original_tool_count + 2
    );
    let recovery_boundary = ready.last().unwrap()["payload"].clone();
    assert_eq!(recovery_boundary["cycles_before"], 1);
    assert_eq!(recovery_boundary["progress_state"]["observed_cycles"], 1);
    assert_eq!(
        recovery_boundary["tool_calls_before"],
        original_tool_count + 2
    );

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
    assert_eq!(resumed["charged_resources"]["calls"], original_calls + 1);
    assert_eq!(resumed["charged_resources"]["cycles"], 2);
    assert_eq!(
        events(&store.task(&task).unwrap(), "finite_search")
            .into_iter()
            .filter(|event| event["payload"]["outcome"] == "limit_reached")
            .count(),
        1
    );
    assert_eq!(
        events(&store.task(&task).unwrap(), "reasoning_progress").len(),
        1
    );
    remove_home(&cfg);
}

#[tokio::test]
async fn progress_assessment_and_saved_proof_recovery_crash_windows_are_deduplicated() {
    // A pre-existing live assessment with no operator proof is the crash window
    // after Jev-R recorded the plateau score but before result recovery committed.
    let (cfg, store, task) = run_research(true, false).await;
    let database = db(&cfg);
    let progress_id: i64 = database.query_row(
        "SELECT id FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=1 ORDER BY id LIMIT 1",
        [&task],
        |row| row.get(0),
    ).unwrap();
    // This is the original-run crash window after the checker and progress
    // assessment committed, before the next solver cycle was dispatched.
    interrupt_at(&database, &task, progress_id);
    drop(database);
    let report = store.recover_verified_result(&task).unwrap();
    assert_eq!(report["verified"], false);
    assert_eq!(report["revision_ready"], true);
    let after_existing_assessment = store.task(&task).unwrap();
    assert_eq!(
        events(&after_existing_assessment, "reasoning_progress").len(),
        1
    );
    assert_eq!(
        events(&after_existing_assessment, "verification")
            .iter()
            .filter(|event| event["payload"]["origin"] == "saved_solver_recovery")
            .count(),
        1
    );
    remove_home(&cfg);

    // A proof-present/assessment-missing partial state is also replayed through
    // the public recovery API. Both the proof and deterministic assessment must
    // converge to one durable record each after the retry.
    let (cfg, store, task) = run_research(true, true).await;
    let first = store.recover_verified_result(&task).unwrap();
    assert_eq!(first["revision_ready"], true);
    let db = db(&cfg);
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND kind='reasoning_progress'",
        [&task],
    )
    .unwrap();
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.phase')='ready_solver' AND json_extract(payload,'$.cycles_before')=1",
        [&task],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [&task],
    )
    .unwrap();
    drop(db);
    let before_retry = store.task(&task).unwrap();
    let second = store.recover_verified_result(&task).unwrap();
    assert_eq!(second["verified"], false);
    assert_eq!(second["revision_ready"], true);
    let after_retry = store.task(&task).unwrap();
    assert_eq!(
        events(&after_retry, "verification")
            .iter()
            .filter(|event| event["payload"]["origin"] == "saved_solver_recovery")
            .count(),
        1
    );
    assert_eq!(events(&after_retry, "reasoning_progress").len(), 1);
    assert!(events(&before_retry, "verification")
        .iter()
        .any(|event| event["payload"]["origin"] == "saved_solver_recovery"));
    assert_eq!(events(&after_retry, "reasoning_boundary").len(), 2);
    remove_home(&cfg);
}

#[tokio::test]
async fn full_objective_check_can_recover_a_subtask_witness_but_rejection_never_resumes_it() {
    for (subtask_answer, expected_verified) in [(WITNESS, true), (BAD, false)] {
        let cfg = config("http://127.0.0.1:1/v1".into(), false);
        cfg.save().unwrap();
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        let script = Arc::new(Script(Mutex::new(
            vec![
                ("planner", PLAN_TWO),
                ("solver", subtask_answer),
                ("solver", "Reviewed second result"),
                ("solver", WITNESS),
            ]
            .into(),
        )));
        let original = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
            .run_profile(QUERY, "deep", ExecutionProfile::Collective)
            .await
            .unwrap();
        assert!(script.0.lock().unwrap().is_empty());
        let task = original.task_id;
        let db = db(&cfg);
        let subtask_end: i64 = db.query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='a' AND json_extract(payload,'$.node.state')='unverified'",
            [&task],
            |row| row.get(0),
        ).unwrap();
        let subtask_solver: String = db.query_row(
            "SELECT json_extract(payload,'$.text') FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver' ORDER BY id LIMIT 1",
            [&task],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(subtask_solver, subtask_answer);
        interrupt_at(&db, &task, subtask_end);
        drop(db);

        let before = store.task(&task).unwrap();
        let report = store.recover_verified_result(&task).unwrap();
        assert_eq!(report["verified"], expected_verified);
        assert_eq!(
            report["revision_ready"], false,
            "subtask result must not fabricate a whole-goal solver checkpoint"
        );
        assert_eq!(report["finite_search_history"]["outcome"], "limit_reached");
        let after = store.task(&task).unwrap();
        assert_eq!(events(&after, "finite_search").len(), 1);
        assert_eq!(
            events(&after, "tool_reservation").len(),
            events(&before, "tool_reservation").len() + 1
        );
        assert_eq!(
            after["task"]["status"],
            if expected_verified {
                "completed"
            } else {
                "interrupted"
            }
        );
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn failed_advisory_saved_result_recovery_keeps_progress_enabled_graph_resumable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let cfg = config(endpoint, true);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", PLAN_TWO),
            ("solver", BAD),
            ("solver", "Reviewed second result"),
            ("solver", WITNESS),
        ]
        .into(),
    )));
    let original = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(original.status, "completed");
    assert!(script.0.lock().unwrap().is_empty());
    let task = original.task_id;
    let database = db(&cfg);
    let first_subtask: i64 = database
        .query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='a' AND json_extract(payload,'$.node.state')='unverified'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    interrupt_at(&database, &task, first_subtask);
    drop(database);

    let report = store.recover_verified_result(&task).unwrap();
    assert_eq!(report["verified"], false);
    assert_eq!(report["revision_ready"], false);
    assert_eq!(
        events(&store.task(&task).unwrap(), "reasoning_progress").len(),
        0,
        "cycle-zero subtask rejection must not enter whole-goal progress history"
    );
    let preview = cogfabric::recovery::continuation::preview(&store, &task).unwrap();
    assert_eq!(preview["phase"], "pre_solver_graph");
    assert_eq!(preview["subtasks"][0]["state"], "unverified");
    assert_eq!(preview["subtasks"][1]["state"], "pending");

    let server = tokio::spawn(serve_answers(
        listener,
        vec![(192, "Reviewed second result"), (384, WITNESS)],
    ));
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
    assert_eq!(resumed["status"], "completed");
    assert_eq!(resumed["verified"], true);
    assert_eq!(
        events(&store.task(&task).unwrap(), "reasoning_progress").len(),
        0
    );
    remove_home(&cfg);
}

#[tokio::test]
async fn rejected_plateau_result_recovery_does_not_create_a_resume_boundary() {
    let cfg = config("http://127.0.0.1:1/v1".into(), true);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let second = r#"{"a":1,"b":2,"c":1}"#;
    let plateau = r#"{"a":1,"b":3,"c":1}"#;
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid plan"),
            ("explorer", "Try changing b"),
            ("solver", REJECTED),
            ("solver", second),
            ("solver", plateau),
        ]
        .into(),
    )));
    let original = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run_profile(
            THREE_VARIABLE_QUERY,
            "research",
            ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(original.status, "rejected");
    assert_eq!(original.stop_reason, "no_progress");
    assert!(script.0.lock().unwrap().is_empty());
    let task = original.task_id;
    let db = db(&cfg);
    let plateau_solver = solver_result_id(&db, &task, Some(3));
    interrupt_at(&db, &task, plateau_solver);
    drop(db);

    let before = store.task(&task).unwrap();
    let report = store.recover_verified_result(&task).unwrap();
    assert_eq!(report["verified"], false);
    assert_eq!(report["revision_ready"], false);
    let after = store.task(&task).unwrap();
    assert_eq!(events(&after, "reasoning_progress").len(), 3);
    assert_eq!(
        events(&after, "reasoning_progress")[2]["payload"]["stop"],
        true
    );
    assert_eq!(
        events(&after, "reasoning_boundary").len(),
        events(&before, "reasoning_boundary").len(),
        "plateau recovery must not add a bypass boundary"
    );
    assert_eq!(after["task"]["status"], "interrupted");
    assert!(cogfabric::recovery::continuation::preview(&store, &task).is_err());
    remove_home(&cfg);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn expired_original_execution_clock_allows_only_proof_reconciliation() {
    let (cfg, store, task) = run_research(false, true).await;
    let database = db(&cfg);
    let settings: String = database
        .query_row(
            "SELECT payload FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 1",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    let settings: Value = serde_json::from_str(&settings).unwrap();
    let duration_ns = settings["configuration"]["max_wall_seconds"]
        .as_u64()
        .unwrap()
        * 1_000_000_000;
    let now_ns = std::fs::read_to_string("/proc/uptime")
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .split_once('.')
        .map(|(seconds, fraction)| {
            let fractional_ns = format!("{fraction:0<9}")[..9].parse::<u64>().unwrap();
            seconds.parse::<u64>().unwrap() * 1_000_000_000 + fractional_ns
        })
        .unwrap();
    assert!(now_ns > duration_ns + 1_000_000_000);
    let expired_deadline = now_ns - 1_000_000_000;
    let expired_start = expired_deadline - duration_ns;
    database
        .execute(
            "UPDATE events SET payload=json_set(payload,'$.execution_clock.started_ns',?2,'$.execution_clock.deadline_ns',?3) WHERE task_id=?1 AND kind='execution_settings'",
            rusqlite::params![task, expired_start, expired_deadline],
        )
        .unwrap();
    drop(database);

    let report = store.recover_verified_result(&task).unwrap();
    assert_eq!(report["verified"], false);
    assert_eq!(report["revision_ready"], false);
    assert_eq!(report["finite_search_history"]["outcome"], "limit_reached");
    let trace = store.task(&task).unwrap();
    assert_eq!(events(&trace, "verification").len(), 1);
    assert_eq!(events(&trace, "reasoning_boundary").len(), 1);
    assert_eq!(trace["task"]["status"], "interrupted");
    assert!(cogfabric::recovery::continuation::preview(&store, &task).is_err());
    remove_home(&cfg);
}
