//! End-to-end checks for opt-in bounded parallel advisory DAG execution.
use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::Semaphore;

const QUERY: &str = "Explain how the independent subgoals fit together";
const PLAN_FOUR: &str = r#"{"steps":[{"id":"a","objective":"Subgoal A","depends_on":[]},{"id":"b","objective":"Subgoal B","depends_on":[]},{"id":"c","objective":"Subgoal C","depends_on":["a","b"]},{"id":"d","objective":"Subgoal D","depends_on":[]}]}"#;
const PLAN_TWO: &str = r#"{"steps":[{"id":"a","objective":"Subgoal A","depends_on":[]},{"id":"b","objective":"Subgoal B","depends_on":[]}] }"#;

#[derive(Clone, Debug)]
struct Call {
    role: String,
    prompt: String,
}

#[derive(Clone, Copy)]
enum Behavior {
    Ordered,
    Stall,
    Immediate,
}

struct FixtureBackend {
    plan: &'static str,
    behavior: Behavior,
    calls: Mutex<Vec<Call>>,
    active: AtomicUsize,
    peak_active: AtomicUsize,
    a_complete: AtomicBool,
    b_complete: AtomicBool,
    c_started: AtomicBool,
    release_a: Semaphore,
    release_b: Semaphore,
}

impl FixtureBackend {
    fn new(plan: &'static str, behavior: Behavior) -> Self {
        Self {
            plan,
            behavior,
            calls: Mutex::new(Vec::new()),
            active: AtomicUsize::new(0),
            peak_active: AtomicUsize::new(0),
            a_complete: AtomicBool::new(false),
            b_complete: AtomicBool::new(false),
            c_started: AtomicBool::new(false),
            release_a: Semaphore::new(0),
            release_b: Semaphore::new(0),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

struct Active<'a>(&'a FixtureBackend);

impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

impl InferenceBackend for FixtureBackend {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.calls.lock().unwrap().push(Call {
            role: role.into(),
            prompt: prompt.into(),
        });
        Box::pin(async move {
            let text = if role == "planner" {
                self.plan.to_owned()
            } else if prompt.contains("Subgoal A") {
                let _active = self.active_subtask().await;
                match self.behavior {
                    Behavior::Ordered => {
                        self.release_a.acquire().await.unwrap().forget();
                        self.a_complete.store(true, Ordering::SeqCst);
                        "result-a".into()
                    }
                    Behavior::Stall => std::future::pending::<String>().await,
                    Behavior::Immediate => "result-a".into(),
                }
            } else if prompt.contains("Subgoal B") {
                let _active = self.active_subtask().await;
                match self.behavior {
                    Behavior::Ordered => {
                        self.release_b.acquire().await.unwrap().forget();
                        self.b_complete.store(true, Ordering::SeqCst);
                        "result-b".into()
                    }
                    Behavior::Stall => std::future::pending::<String>().await,
                    Behavior::Immediate => "result-b".into(),
                }
            } else if prompt.contains("Subgoal C") {
                self.c_started.store(true, Ordering::SeqCst);
                assert!(self.a_complete.load(Ordering::SeqCst));
                assert!(self.b_complete.load(Ordering::SeqCst));
                let _active = self.active_subtask().await;
                "result-c".into()
            } else if prompt.contains("Subgoal D") {
                let _active = self.active_subtask().await;
                "result-d".into()
            } else {
                "final-answer".into()
            };
            Ok(Generation {
                text,
                model: "parallel-subtask-fixture/v1".into(),
                input_tokens: Some(3),
                output_tokens: Some(2),
                elapsed_ms: 0.1,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}

impl FixtureBackend {
    async fn active_subtask(&self) -> Active<'_> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak_active.fetch_max(active, Ordering::SeqCst);
        Active(self)
    }
}

fn config(parallelism: Option<usize>, calls: usize, tokens: usize) -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: calls,
        max_tokens: tokens,
        max_cycles: 1,
        subtask_parallelism: parallelism,
        ..Config::default()
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

fn events(trace: &Value, kind: &str) -> Vec<Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == kind)
        .cloned()
        .collect()
}

#[tokio::test]
async fn independent_subtasks_overlap_and_dependency_and_final_order_are_preserved() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(FixtureBackend::new(PLAN_FOUR, Behavior::Ordered));
    let engine = Engine::with_backend(config(Some(2), 6, 1408), store.clone(), backend.clone());

    let run = tokio::spawn(async move {
        engine
            .run_profile(QUERY, "deep", ExecutionProfile::Collective)
            .await
    });
    let overlap = tokio::time::timeout(Duration::from_secs(3), async {
        while backend.peak_active.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        overlap.is_ok(),
        "the two independent first-wave subtasks must overlap; observed calls: {:?}",
        backend.calls()
    );

    // Complete B first. C depends on both A and B and must remain undispatched
    // until A is released as well.
    backend.release_b.add_permits(1);
    tokio::time::timeout(Duration::from_secs(3), async {
        while !backend.b_complete.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("subtask B should complete after its release");
    assert!(!backend.c_started.load(Ordering::SeqCst));
    backend.release_a.add_permits(1);

    let outcome = tokio::time::timeout(Duration::from_secs(3), run)
        .await
        .expect("parallel advisory graph stalled")
        .unwrap()
        .unwrap();
    assert_eq!(outcome.answer, "final-answer");
    assert_eq!(outcome.status, "unverified");
    assert_eq!(outcome.calls, 6);
    assert_eq!(outcome.resources.reserved_output_tokens, 1408);
    assert!(backend.peak_active.load(Ordering::SeqCst) >= 2);

    let trace = store.task(&outcome.task_id).unwrap();
    let schedule = &event(&trace, "subtask_parallel_schedule")["payload"];
    assert_eq!(schedule["max_parallelism"], 2);
    assert_eq!(schedule["order"], json!([0, 1, 2, 3]));
    assert_eq!(schedule["waves"], json!([[0, 1], [2, 3]]));
    assert_eq!(schedule["permissions_granted"], false);
    assert_eq!(schedule["results_verified"], false);
    assert!(event(&trace, "reasoning_boundary")["payload"]["strategy"]
        .as_str()
        .unwrap()
        .contains(r#"{"id":"a","result":"result-a"},{"id":"b","result":"result-b"},{"id":"c","result":"result-c"},{"id":"d","result":"result-d"}"#));

    let dispatches = events(&trace, "model_dispatch");
    assert_eq!(dispatches.len(), 6);
    assert_eq!(dispatches[0]["payload"]["role"], "planner");
    assert_eq!(dispatches[0]["payload"]["allocation"], 256);
    assert_eq!(dispatches.last().unwrap()["payload"]["role"], "solver");
    assert_eq!(dispatches.last().unwrap()["payload"]["allocation"], 384);
    assert_eq!(
        dispatches
            .iter()
            .map(|item| item["payload"]["prompt_bytes"].as_u64().unwrap())
            .sum::<u64>(),
        outcome.resources.prompt_bytes as u64,
        "all original prompt reservations, including the final solver, remain charged"
    );
    assert_eq!(
        events(&trace, "subtask_resources")
            .iter()
            .filter(|item| item["payload"]["state"] == "unverified")
            .count(),
        4
    );

    let calls = backend.calls();
    let final_prompt = &calls.last().unwrap().prompt;
    let positions = ["result-a", "result-b", "result-c", "result-d"].map(|result| {
        final_prompt
            .find(result)
            .expect("result should reach final solver")
    });
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(calls.iter().any(|call| call.prompt.contains("Subgoal C")));
}

#[tokio::test]
async fn cancelling_parallel_wave_drops_every_active_backend_future() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(FixtureBackend::new(PLAN_TWO, Behavior::Stall));
    let engine = Engine::with_backend(config(Some(2), 4, 1024), store.clone(), backend.clone());
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
    let run = tokio::spawn(async move {
        engine
            .run_until(QUERY, "deep", async move {
                let _ = cancelled.await;
            })
            .await
    });
    let overlap = tokio::time::timeout(Duration::from_secs(3), async {
        while backend.peak_active.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        overlap.is_ok(),
        "both active advisory futures should be observable before cancellation; observed calls: {:?}",
        backend.calls()
    );
    cancel.send(()).unwrap();

    let outcome = tokio::time::timeout(Duration::from_secs(3), run)
        .await
        .expect("cancellation did not finish the task")
        .unwrap()
        .unwrap();
    assert_eq!(outcome.status, "cancelled");
    assert_eq!(
        outcome.calls, 3,
        "planner plus both already admitted subtasks"
    );
    assert_eq!(outcome.resources.reserved_output_tokens, 640);
    tokio::time::timeout(Duration::from_secs(1), async {
        while backend.active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancellation must drop both backend futures without detached tasks");

    let trace = store.task(&outcome.task_id).unwrap();
    let cancelled_nodes = events(&trace, "subtask")
        .iter()
        .filter(|item| item["payload"]["node"]["state"] == "cancelled")
        .count();
    assert_eq!(cancelled_nodes, 2);
    let costs = events(&trace, "subtask_resources");
    assert_eq!(costs.len(), 2);
    assert!(costs.iter().all(|item| {
        item["payload"]["state"] == "cancelled"
            && item["payload"]["calls_reserved"] == 1
            && item["payload"]["reserved_output_tokens"] == 192
            && item["payload"]["inference_completed"] == false
            && item["payload"]["input_tokens"].is_null()
            && item["payload"]["output_tokens"].is_null()
    }));
    assert!(!events(&trace, "reasoning_boundary").iter().any(|item| {
        item["payload"]["strategy"]
            .as_str()
            .is_some_and(|strategy| strategy.contains("Subtask results"))
    }));
}

#[tokio::test]
async fn absent_and_single_parallelism_keep_sequential_dispatch_compatibility() {
    for parallelism in [None, Some(1)] {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = Arc::new(FixtureBackend::new(PLAN_TWO, Behavior::Immediate));
        let engine =
            Engine::with_backend(config(parallelism, 4, 1024), store.clone(), backend.clone());
        let outcome = engine
            .run_profile(QUERY, "deep", ExecutionProfile::Collective)
            .await
            .unwrap();
        assert_eq!(outcome.calls, 4);
        assert_eq!(outcome.resources.reserved_output_tokens, 1024);
        assert_eq!(backend.peak_active.load(Ordering::SeqCst), 1);
        let trace = store.task(&outcome.task_id).unwrap();
        assert!(events(&trace, "subtask_parallel_schedule").is_empty());
        let running = events(&trace, "subtask")
            .into_iter()
            .filter(|item| item["payload"]["node"]["state"] == "running")
            .map(|item| {
                item["payload"]["node"]["step"]["id"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(running, ["a", "b"]);
    }
}

#[tokio::test]
async fn insufficient_graph_call_budget_preserves_the_whole_goal_solver_reserve() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(FixtureBackend::new(PLAN_TWO, Behavior::Immediate));
    // Planner plus final solver fit, but the graph has no two calls available
    // while retaining a final whole-goal solver call.
    let engine = Engine::with_backend(config(Some(2), 3, 640), store.clone(), backend.clone());
    let outcome = engine
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();

    assert_eq!(outcome.calls, 2);
    assert_eq!(outcome.resources.reserved_output_tokens, 640);
    let trace = store.task(&outcome.task_id).unwrap();
    assert!(events(&trace, "subtask_parallel_schedule").is_empty());
    let skipped_costs = events(&trace, "subtask_resources");
    assert_eq!(skipped_costs.len(), 2);
    assert!(skipped_costs.iter().all(|item| {
        item["payload"]["state"] == "skipped"
            && item["payload"]["calls_reserved"] == 0
            && item["payload"]["reserved_output_tokens"] == 0
    }));
    assert!(events(&trace, "schedule")
        .iter()
        .any(|item| item["payload"]["decision"] == "skip_decomposition"));
    let dispatches = events(&trace, "model_dispatch");
    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0]["payload"]["role"], "planner");
    assert_eq!(dispatches[1]["payload"]["role"], "solver");
    assert_eq!(dispatches[1]["payload"]["allocation"], 384);
}

#[tokio::test]
async fn failed_second_wave_reservation_does_not_dispatch_any_second_wave_backend() {
    let directory = std::env::temp_dir().join(format!(
        "fabric-parallel-wave-audit-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let db_path = directory.join("runtime.sqlite3");
    let store = Arc::new(Store::open(&db_path).unwrap());
    rusqlite::Connection::open(&db_path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse_second_wave BEFORE INSERT ON events
             WHEN NEW.kind='model_dispatch' AND json_extract(NEW.payload,'$.call')=4
             BEGIN SELECT RAISE(ABORT,'second-wave reservation unavailable'); END;",
        )
        .unwrap();
    let backend = Arc::new(FixtureBackend::new(PLAN_FOUR, Behavior::Immediate));
    let engine = Engine::with_backend(config(Some(2), 6, 1408), store.clone(), backend.clone());
    let outcome = engine
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();

    assert_eq!(outcome.status, "failed");
    assert_eq!(
        outcome.calls, 3,
        "planner and both first-wave reservations remain charged"
    );
    assert_eq!(outcome.resources.reserved_output_tokens, 640);
    let dispatched = backend.calls();
    assert_eq!(dispatched.len(), 3);
    assert_eq!(dispatched[0].role, "planner");
    assert!(dispatched
        .iter()
        .any(|call| call.prompt.contains("Subgoal A")));
    assert!(dispatched
        .iter()
        .any(|call| call.prompt.contains("Subgoal B")));
    assert!(!dispatched
        .iter()
        .any(|call| { call.prompt.contains("Subgoal C") || call.prompt.contains("Subgoal D") }));

    let trace = store.task(&outcome.task_id).unwrap();
    assert_eq!(events(&trace, "model_dispatch").len(), 3);
    let schedule = &event(&trace, "subtask_parallel_schedule")["payload"];
    assert_eq!(schedule["waves"], json!([[0, 1], [2, 3]]));
    assert_eq!(
        events(&trace, "subtask_resources")
            .iter()
            .filter(|item| item["payload"]["state"] == "unverified")
            .count(),
        2
    );

    drop(engine);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn required_parallel_schedule_audit_failure_prevents_any_subtask_dispatch() {
    let directory =
        std::env::temp_dir().join(format!("fabric-parallel-audit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let db_path = directory.join("runtime.sqlite3");
    let store = Arc::new(Store::open(&db_path).unwrap());
    rusqlite::Connection::open(&db_path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse_parallel_schedule BEFORE INSERT ON events
             WHEN NEW.kind='subtask_parallel_schedule'
             BEGIN SELECT RAISE(ABORT,'parallel schedule audit unavailable'); END;",
        )
        .unwrap();
    let backend = Arc::new(FixtureBackend::new(PLAN_TWO, Behavior::Immediate));
    let engine = Engine::with_backend(config(Some(2), 4, 1024), store.clone(), backend.clone());
    let outcome = engine
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();

    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.calls, 1);
    assert_eq!(
        backend
            .calls()
            .iter()
            .map(|call| call.role.as_str())
            .collect::<Vec<_>>(),
        ["planner"]
    );
    let trace = store.task(&outcome.task_id).unwrap();
    assert!(events(&trace, "subtask_parallel_schedule").is_empty());
    assert!(events(&trace, "subtask")
        .iter()
        .all(|item| { item["payload"]["node"]["state"] != "running" }));
    assert_eq!(events(&trace, "model_dispatch").len(), 1);

    drop(engine);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
