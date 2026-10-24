use super::*;
fn settings() -> Settings {
    Settings {
        min_observations: 2,
        sample_size: 4,
        window_seconds: 60,
        trip_failure_basis_points: 7500,
    }
}
fn observation(scope: &str, phase: Phase, boot: &str, time: u64, outcome: Outcome) -> Observation {
    Observation {
        format_version: 1,
        scope_sha256: scope.into(),
        phase,
        boot_id: boot.into(),
        ended_ns: time,
        budget_ms: 10000,
        elapsed_ms: 100,
        outcome,
        permissions_granted: false,
        provider_stop_proven: false,
    }
}
fn insert(store: &Store, task: &str, observation: Observation) {
    store
        .event(
            task,
            "advice_phase_observation",
            "jev-plus",
            serde_json::to_value(observation).unwrap(),
        )
        .unwrap();
}
#[test]
fn history_is_same_boot_window_scope_phase_budget_and_sample_bounded() {
    let store = Store::in_memory().unwrap();
    let task = store.create_task("History", "research").unwrap();
    let boot = uuid::Uuid::new_v4().to_string();
    let scope = "a".repeat(64);
    for (s, p, b, t, outcome) in [
        (
            scope.clone(),
            Phase::Planning,
            boot.clone(),
            100_000_000_000,
            Outcome::TimedOut,
        ),
        (
            scope.clone(),
            Phase::Planning,
            boot.clone(),
            101_000_000_000,
            Outcome::Completed,
        ),
        (
            "b".repeat(64),
            Phase::Planning,
            boot.clone(),
            101_000_000_000,
            Outcome::Failed,
        ),
        (
            scope.clone(),
            Phase::Exploration,
            boot.clone(),
            101_000_000_000,
            Outcome::Failed,
        ),
        (
            scope.clone(),
            Phase::Planning,
            uuid::Uuid::new_v4().to_string(),
            101_000_000_000,
            Outcome::Failed,
        ),
        (
            scope.clone(),
            Phase::Planning,
            boot.clone(),
            1,
            Outcome::Failed,
        ),
    ] {
        insert(&store, &task, observation(&s, p, &b, t, outcome));
    }
    let counts = history(
        &store.lock().unwrap(),
        &settings(),
        &scope,
        Phase::Planning,
        9999,
        &boot,
        102_000_000_000,
    )
    .unwrap();
    assert_eq!(counts.observations, 2);
    assert_eq!(counts.failures, 1);
    assert_eq!(
        history(
            &store.lock().unwrap(),
            &settings(),
            &scope,
            Phase::Planning,
            11000,
            &boot,
            102_000_000_000
        )
        .unwrap()
        .observations,
        0
    );
    let mut one = settings();
    one.sample_size = 2;
    insert(
        &store,
        &task,
        observation(
            &scope,
            Phase::Planning,
            &boot,
            102_000_000_000,
            Outcome::Failed,
        ),
    );
    let counts = history(
        &store.lock().unwrap(),
        &one,
        &scope,
        Phase::Planning,
        10000,
        &boot,
        102_000_000_000,
    )
    .unwrap();
    assert_eq!(counts.observations, 2);
    assert_eq!(counts.failures, 1);
    for _ in 0..512 {
        store
            .event(&task, "unrelated", "runtime", json!({}))
            .unwrap();
    }
    assert_eq!(
        history(
            &store.lock().unwrap(),
            &settings(),
            &scope,
            Phase::Planning,
            10000,
            &boot,
            102_000_000_000
        )
        .unwrap()
        .observations,
        0
    );
}
#[test]
fn settings_defaults_serialization_and_bad_history_fail_closed() {
    let cfg = Config::default();
    assert!(serde_json::to_value(&cfg)
        .unwrap()
        .get("advice_guard")
        .is_none());
    for bad in [
        Settings {
            min_observations: 0,
            ..settings()
        },
        Settings {
            sample_size: 1,
            ..settings()
        },
        Settings {
            sample_size: 17,
            ..settings()
        },
        Settings {
            window_seconds: 0,
            ..settings()
        },
        Settings {
            trip_failure_basis_points: 10001,
            ..settings()
        },
    ] {
        assert!(bad.validate().is_err());
    }
    let store = Store::in_memory().unwrap();
    let task = store.create_task("Bad history", "research").unwrap();
    let boot = uuid::Uuid::new_v4().to_string();
    let scope = "a".repeat(64);
    insert(
        &store,
        &task,
        observation(&scope, Phase::Planning, &boot, 200, Outcome::Failed),
    );
    assert_eq!(
        history(
            &store.lock().unwrap(),
            &settings(),
            &scope,
            Phase::Planning,
            10000,
            &boot,
            100
        )
        .unwrap()
        .observations,
        0
    );
    store
        .lock()
        .unwrap()
        .execute(
            "DELETE FROM events WHERE kind='advice_phase_observation'",
            [],
        )
        .unwrap();
    store
        .event(
            &task,
            "advice_phase_observation",
            "jev-plus",
            json!({"private":"x".repeat(5000)}),
        )
        .unwrap();
    assert!(history(
        &store.lock().unwrap(),
        &settings(),
        &scope,
        Phase::Planning,
        10000,
        &boot,
        100
    )
    .is_err());
}

use crate::backend::Generation;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
struct SlowPlanner {
    planners: AtomicUsize,
    solvers: AtomicUsize,
}
impl InferenceBackend for SlowPlanner {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            if role == "planner" {
                self.planners.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<()>().await;
            }
            assert_eq!(role, "solver");
            self.solvers.fetch_add(1, Ordering::SeqCst);
            Ok(Generation {
                text: "[1,2,3]".into(),
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
fn cfg() -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_tokens: 2000,
        timeout_seconds: 1,
        advice_guard: Some(settings()),
        ..Config::default()
    }
}
#[cfg(target_os = "linux")]
#[tokio::test(start_paused = true)]
async fn repeated_timeouts_skip_planning_without_spending_solver_resources() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(SlowPlanner {
        planners: AtomicUsize::new(0),
        solvers: AtomicUsize::new(0),
    });
    let engine = crate::engine::Engine::with_backend(cfg(), store.clone(), backend.clone());
    for attempt in 0..3 {
        let outcome = engine
            .run(r#"sort:{"values":[3,2,1]}"#, "deep")
            .await
            .unwrap();
        assert_eq!(outcome.status, "completed");
        assert_eq!(outcome.calls, if attempt < 2 { 2 } else { 1 });
        let trace = store.task(&outcome.task_id).unwrap();
        let events = trace["events"].as_array().unwrap();
        let decision = events
            .iter()
            .find(|e| e["kind"] == "advice_guard_decision")
            .unwrap();
        assert_eq!(decision["payload"]["allow"], attempt < 2);
        if attempt == 2 {
            assert!(!events.iter().any(|e| e["kind"] == "planning_phase_budget"));
            assert_eq!(outcome.resources.reserved_output_tokens, 384);
        }
    }
    let separate = engine
        .run_profile(
            r#"sort:{"values":[3,2,1]}"#,
            "deep",
            crate::engine::ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(separate.calls, 2);
    assert_eq!(backend.planners.load(Ordering::SeqCst), 3);
    assert_eq!(backend.solvers.load(Ordering::SeqCst), 4);
}
#[cfg(target_os = "linux")]
#[tokio::test(start_paused = true)]
async fn observations_survive_reopen_and_model_scope_change_restarts_warmup() {
    let path = std::env::temp_dir().join(format!("fabric-advice-{}", uuid::Uuid::new_v4()));
    let mut config = cfg();
    config.home = path.clone();
    let backend = Arc::new(SlowPlanner {
        planners: AtomicUsize::new(0),
        solvers: AtomicUsize::new(0),
    });
    for attempt in 0..4 {
        if attempt == 3 {
            config.model = "different-model".into();
        }
        let store = Arc::new(Store::open(&config.db()).unwrap());
        let engine = crate::engine::Engine::with_backend(config.clone(), store, backend.clone());
        let outcome = engine
            .run(r#"sort:{"values":[3,2,1]}"#, "deep")
            .await
            .unwrap();
        assert_eq!(outcome.calls, if attempt == 2 { 1 } else { 2 });
        assert_eq!(outcome.status, "completed");
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[cfg(target_os = "linux")]
#[tokio::test]
async fn decision_and_observation_audit_failures_prevent_followup_inference() {
    struct Fast(Mutex<Vec<&'static str>>);
    impl InferenceBackend for Fast {
        fn generate<'a>(
            &'a self,
            role: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            assert_eq!(role, "planner");
            self.0.lock().unwrap().push("planner");
            Box::pin(async {
                Ok(Generation {
                    text: "invalid".into(),
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
    for kind in ["advice_guard_decision", "advice_phase_observation"] {
        let store = Arc::new(Store::in_memory().unwrap());
        store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER refuse_guard BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let backend = Arc::new(Fast(Mutex::new(Vec::new())));
        let outcome = crate::engine::Engine::with_backend(cfg(), store, backend)
            .run(r#"sort:{"values":[3,2,1]}"#, "deep")
            .await
            .unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(
            outcome.calls,
            if kind == "advice_guard_decision" {
                0
            } else {
                1
            }
        );
    }
}

#[test]
fn exact_thresholds_and_completed_work_exceeding_current_budget_count_as_cost_failure() {
    assert!(!tripped(
        &settings(),
        &History {
            observations: 1,
            failures: 1,
            total_elapsed_ms: 1
        }
    ));
    assert!(!tripped(
        &settings(),
        &History {
            observations: 4,
            failures: 2,
            total_elapsed_ms: 1
        }
    ));
    assert!(tripped(
        &settings(),
        &History {
            observations: 4,
            failures: 3,
            total_elapsed_ms: 1
        }
    ));
    let store = Store::in_memory().unwrap();
    let task = store.create_task("Over budget history", "deep").unwrap();
    let boot = uuid::Uuid::new_v4().to_string();
    let scope = "a".repeat(64);
    let mut slow = observation(&scope, Phase::Planning, &boot, 100, Outcome::Completed);
    slow.elapsed_ms = 15000;
    slow.budget_ms = 20000;
    insert(&store, &task, slow);
    let counts = history(
        &store.lock().unwrap(),
        &settings(),
        &scope,
        Phase::Planning,
        10000,
        &boot,
        200,
    )
    .unwrap();
    assert_eq!(counts.observations, 1);
    assert_eq!(counts.failures, 1);
}
#[cfg(target_os = "linux")]
#[tokio::test(start_paused = true)]
async fn exploration_history_does_not_suppress_successful_planning() {
    struct SlowExplorer;
    impl InferenceBackend for SlowExplorer {
        fn generate<'a>(
            &'a self,
            role: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async move {
                if role == "explorer" {
                    std::future::pending::<()>().await;
                }
                let text = match role {
                    "planner" => "invalid",
                    "solver" => "[1,2,3]",
                    _ => panic!("Unexpected role"),
                };
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
    let store = Arc::new(Store::in_memory().unwrap());
    let engine = crate::engine::Engine::with_backend(cfg(), store.clone(), Arc::new(SlowExplorer));
    for attempt in 0..3 {
        let outcome = engine
            .run(r#"sort:{"values":[3,2,1]}"#, "research")
            .await
            .unwrap();
        assert_eq!(outcome.status, "completed");
        assert_eq!(outcome.calls, if attempt < 2 { 3 } else { 2 });
        let trace = store.task(&outcome.task_id).unwrap();
        let decisions = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "advice_guard_decision")
            .collect::<Vec<_>>();
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0]["payload"]["phase"], "planning");
        assert_eq!(decisions[0]["payload"]["allow"], true);
        assert_eq!(decisions[1]["payload"]["allow"], attempt < 2);
    }
}

#[tokio::test]
async fn expired_phase_ticket_dispatches_no_inference_and_creates_no_success_observation() {
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store.create_task("Expired optional phase", "deep").unwrap();
    let backend = Arc::new(SlowPlanner {
        planners: AtomicUsize::new(0),
        solvers: AtomicUsize::new(0),
    });
    let mut session =
        Session::new(cfg(), store.clone(), backend.clone(), task.clone(), "deep").unwrap();
    for phase in [Phase::Planning, Phase::Exploration] {
        let ticket = Ticket {
            phase,
            scope: Some("a".repeat(64)),
            budget: Duration::ZERO,
            started: Instant::now(),
            calls_before: 0,
        };
        if phase == Phase::Planning {
            assert!(session
                .planning_advice("Objective", ticket)
                .await
                .unwrap()
                .is_empty());
        } else {
            assert!(session
                .exploration_advice("Objective", ticket)
                .await
                .unwrap()
                .is_none());
        }
    }
    assert_eq!(backend.planners.load(Ordering::SeqCst), 0);
    assert_eq!(session.resources.calls, 0);
    assert!(!store.task(&task).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "advice_phase_observation"));
}
