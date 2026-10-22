//! Reconcile the final recorded result of a parallel wave before its graph
//! transition was durably written.
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
};

const QUERY: &str = r#"csp:{"variables":[{"id":"x","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const PLAN: &str = r#"{"steps":[{"id":"a","objective":"Subgoal A","depends_on":[]},{"id":"b","objective":"Subgoal B","depends_on":[]}]}"#;

#[derive(Default)]
struct Script(Mutex<Vec<(String, String)>>);

impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.0
            .lock()
            .unwrap()
            .push((role.to_owned(), prompt.to_owned()));
        let text = if role == "planner" {
            PLAN.to_owned()
        } else if prompt.contains("Subgoal A") {
            "advice-a".to_owned()
        } else if prompt.contains("Subgoal B") {
            "advice-b".to_owned()
        } else {
            r#"{"x":1}"#.to_owned()
        };
        Box::pin(async move {
            Ok(Generation {
                text,
                model: "parallel-advisory-recovery-fixture/v1".into(),
                input_tokens: Some(2),
                output_tokens: Some(2),
                elapsed_ms: 0.1,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}

struct Fixture {
    cfg: Config,
    store: Arc<Store>,
    task: String,
}

async fn fixture() -> Fixture {
    let cfg = Config {
        home: std::env::temp_dir().join(format!(
            "fabric-parallel-advisory-recovery-{}",
            uuid::Uuid::new_v4()
        )),
        backend: "openai-local".into(),
        model: "parallel-advisory-recovery-fixture/v1".into(),
        api_base: "http://127.0.0.1:1/v1".into(),
        interpret_goals: false,
        max_calls: 8,
        max_cycles: 1,
        max_tokens: 2400,
        max_tool_calls: 8,
        timeout_seconds: 120,
        subtask_parallelism: Some(2),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script::default());
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, r#"{"x":1}"#);
    let calls = script.0.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[0].0, "planner");
    assert!(calls
        .iter()
        .any(|(role, prompt)| role == "solver" && prompt.contains("Subgoal A")));
    assert!(calls
        .iter()
        .any(|(role, prompt)| role == "solver" && prompt.contains("Subgoal B")));
    drop(calls);

    let trace = store.task(&outcome.task_id).unwrap();
    let marker = event(&trace, "subtask_parallel_schedule");
    assert_eq!(marker["payload"]["waves"], json!([[0, 1]]));
    assert_eq!(
        events(&trace, "subtask_resources")
            .iter()
            .filter(|event| event["payload"]["state"] == "unverified")
            .count(),
        2
    );
    let db = Connection::open(cfg.db()).unwrap();
    let cutoff: i64 = db
        .query_row(
            "SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.text')='advice-b' ORDER BY id DESC LIMIT 1",
            [&outcome.task_id],
            |row| row.get(0),
        )
        .unwrap();
    rewind_to_running_wave_tail(&cfg, &outcome.task_id, cutoff);
    Fixture {
        cfg,
        store,
        task: outcome.task_id,
    }
}

fn event<'a>(trace: &'a Value, kind: &str) -> &'a Value {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == kind)
        .unwrap()
}

fn events(trace: &Value, kind: &str) -> Vec<Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == kind)
        .cloned()
        .collect()
}

fn rewind_to_running_wave_tail(cfg: &Config, task: &str, cutoff: i64) {
    let mut lock_path = cfg.db().into_os_string();
    lock_path.push(".runtime.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path)
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
}

fn remove_fixture(fixture: Fixture) {
    let home = fixture.cfg.home.clone();
    drop(fixture.store);
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn links_last_parallel_result_and_admits_the_valid_checkpoint_without_replay_or_refund() {
    let fixture = fixture().await;
    assert_eq!(
        fixture.store.recover_tasks().unwrap(),
        vec![fixture.task.clone()]
    );
    let before = fixture.store.task(&fixture.task).unwrap();
    let before_audit = fixture.store.inference_audit(&fixture.task).unwrap();
    assert_eq!(
        events(&before, "reasoning")
            .iter()
            .filter(|event| {
                event["payload"]["node"]["role"] == "solver"
                    && event["payload"]["node"]["cycle"] == 0
            })
            .count(),
        2
    );
    assert_eq!(
        events(&before, "subtask")
            .iter()
            .filter(|event| event["payload"]["node"]["state"] == "running")
            .count(),
        2
    );
    assert_eq!(
        events(&before, "subtask")
            .iter()
            .filter(|event| event["payload"]["node"]["state"] == "unverified")
            .count(),
        1,
        "the last reasoning result must precede the final wave graph transition"
    );

    assert!(fixture
        .store
        .recover_advisory_result(&fixture.task, false)
        .is_err());
    let mut lock_path = fixture.cfg.db().into_os_string();
    lock_path.push(".runtime.lock");
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path)
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    assert!(fixture
        .store
        .recover_advisory_result(&fixture.task, true)
        .is_err());
    assert_eq!(unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_UN) }, 0);
    drop(lease);
    let report = fixture
        .store
        .recover_advisory_result(&fixture.task, true)
        .unwrap();
    assert_eq!(report["subtask_id"], "b");
    assert_eq!(report["result_verified"], false);
    assert_eq!(report["inference_replayed"], false);
    assert_eq!(report["resume_authorized"], false);
    assert_eq!(report["original_budget_preserved"], true);
    assert_eq!(report["call"], before_audit["reserved_calls"]);

    let after = fixture.store.task(&fixture.task).unwrap();
    let after_audit = fixture.store.inference_audit(&fixture.task).unwrap();
    assert_eq!(
        after_audit["reserved_calls"],
        before_audit["reserved_calls"]
    );
    assert_eq!(
        after_audit["reserved_output_tokens"],
        before_audit["reserved_output_tokens"]
    );
    assert_eq!(
        events(&after, "model_dispatch"),
        events(&before, "model_dispatch")
    );
    assert_eq!(
        events(&after, "reasoning")
            .iter()
            .filter(|event| {
                event["payload"]["node"]["role"] == "solver"
                    && event["payload"]["node"]["cycle"] == 0
            })
            .count(),
        2
    );
    assert_eq!(
        events(&after, "subtask")
            .iter()
            .filter(|event| event["payload"]["node"]["state"] == "unverified")
            .count(),
        2
    );
    assert_eq!(
        event(&after, "advisory_recovery")["payload"]["result_sha256"],
        cogfabric::store::sha256(b"advice-b")
    );
    assert_eq!(
        continuation::preview(&fixture.store, &fixture.task).unwrap()["checkpoint_eligible"],
        true,
        "both graph nodes should form a valid next checkpoint"
    );
    remove_fixture(fixture);
}

#[tokio::test]
async fn malformed_call_wave_or_unknown_parallel_outcome_refuses_reconciliation() {
    for alteration in ["call", "wave", "unknown"] {
        let fixture = fixture().await;
        assert_eq!(
            fixture.store.recover_tasks().unwrap(),
            vec![fixture.task.clone()]
        );
        let db = Connection::open(fixture.cfg.db()).unwrap();
        match alteration {
            "call" => {
                db.execute(
                    "UPDATE events SET payload=json_set(payload,'$.call',99) WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.text')='advice-b'",
                    [&fixture.task],
                )
                .unwrap();
            }
            "wave" => {
                db.execute(
                    "UPDATE events SET payload=json_set(payload,'$.waves',json('[ [0], [1] ]')) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",
                    [&fixture.task],
                )
                .unwrap();
            }
            "unknown" => {
                db.execute(
                    "DELETE FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.text')='advice-b'",
                    [&fixture.task],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        drop(db);
        let before = fixture.store.task(&fixture.task).unwrap();
        assert!(
            fixture
                .store
                .recover_advisory_result(&fixture.task, true)
                .is_err(),
            "{alteration} must fail closed"
        );
        assert_eq!(fixture.store.task(&fixture.task).unwrap(), before);
        assert!(events(&before, "advisory_recovery").is_empty());
        remove_fixture(fixture);
    }
}

#[tokio::test]
async fn failed_advisory_audit_rolls_back_the_parallel_graph_transition() {
    let fixture = fixture().await;
    assert_eq!(
        fixture.store.recover_tasks().unwrap(),
        vec![fixture.task.clone()]
    );
    let db = Connection::open(fixture.cfg.db()).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_parallel_advisory_recovery BEFORE INSERT ON events
         WHEN NEW.kind='advisory_recovery'
         BEGIN SELECT RAISE(ABORT,'parallel advisory audit unavailable'); END;",
    )
    .unwrap();
    let before = fixture.store.task(&fixture.task).unwrap();
    assert!(fixture
        .store
        .recover_advisory_result(&fixture.task, true)
        .is_err());
    assert_eq!(fixture.store.task(&fixture.task).unwrap(), before);
    assert_eq!(
        fixture.store.inference_audit(&fixture.task).unwrap()["reserved_calls"],
        3
    );
    drop(db);
    remove_fixture(fixture);
}
