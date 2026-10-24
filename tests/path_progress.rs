use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    store::Store,
    VerificationClaim, VerificationFuture, VerificationRequest, VerifierDescriptor,
    VerifierProvider,
};
use serde_json::Value;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

const QUERY: &str = r#"path:{"nodes":["s","a","b","c","g"],"edges":[["s","g"],["s","a"],["a","g"],["s","b"],["b","g"],["s","c"],["c","g"]],"start":"s","goal":"g"}"#;
const A: &str = r#"["s","a","g"]"#;
const B: &str = r#"["s","b","g"]"#;
const C: &str = r#"["s","c","g"]"#;
const ENDPOINT_MISMATCH: &str = r#"["a","g"]"#;
const ENDPOINT_MISMATCH_B: &str = r#"["b","g"]"#;
const GOOD: &str = r#"["s","g"]"#;

struct Script(Mutex<VecDeque<&'static str>>);

impl Script {
    fn new(answers: impl IntoIterator<Item = &'static str>) -> Self {
        Self(Mutex::new(answers.into_iter().collect()))
    }

    fn remaining(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        assert_eq!(role, "solver");
        let answer = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected path-progress inference");
        Box::pin(async move {
            Ok(Generation {
                text: answer.into(),
                model: "path-progress-fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
                provider_metrics: None,
            })
        })
    }
}

struct RejectFirstProvider(Mutex<usize>);

impl VerifierProvider for RejectFirstProvider {
    fn descriptor(&self) -> VerifierDescriptor {
        VerifierDescriptor {
            id: "fixture.path_verifier".into(),
            version: "1.0.0".into(),
            implementation_sha256: "a".repeat(64),
        }
    }

    fn verify(&self, request: VerificationRequest) -> VerificationFuture<'_> {
        assert_eq!(request.criterion, "shortest_path/v1");
        let mut calls = self.0.lock().unwrap();
        let first = *calls == 0;
        *calls += 1;
        drop(calls);
        if first {
            Box::pin(async { Ok(VerificationClaim::rejected("fixture_veto")) })
        } else {
            Box::pin(async { Ok(VerificationClaim::accepted()) })
        }
    }
}

fn config(progress: bool) -> Config {
    Config {
        home: std::env::temp_dir().join(format!("fabric-path-progress-{}", uuid::Uuid::new_v4())),
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 12,
        max_cycles: 4,
        max_tokens: 5000,
        max_tool_calls: 10,
        progress_stopping: progress
            .then_some(cogfabric::reasoning::progress::Settings { plateau_cycles: 2 }),
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

async fn run(cfg: &Config, script: Arc<Script>) -> (cogfabric::engine::Outcome, Arc<Store>) {
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script)
        .run_profile(QUERY, "research", ExecutionProfile::Recursive)
        .await
        .unwrap();
    (outcome, store)
}

#[tokio::test]
async fn distinct_candidates_plateau_and_a_strict_improvement_restarts_the_streak() {
    let cfg = config(true);
    let script = Arc::new(Script::new([ENDPOINT_MISMATCH, A, B, C]));
    let (outcome, store) = run(&cfg, script.clone()).await;
    let trace = store.task(&outcome.task_id).unwrap();
    let progress = events(&trace, "reasoning_progress");

    assert_eq!(outcome.status, "rejected");
    assert_eq!(outcome.stop_reason, "no_progress");
    assert_eq!(outcome.calls, 4);
    assert_eq!(outcome.cycles, 4);
    assert_eq!(script.remaining(), 0);
    assert_eq!(progress.len(), 4);
    assert_eq!(
        progress
            .iter()
            .map(|e| e["payload"]["score"].as_u64())
            .collect::<Vec<_>>(),
        vec![Some(65), Some(1), Some(1), Some(1)]
    );
    assert_eq!(progress[1]["payload"]["best_score"], 1);
    assert_eq!(progress[1]["payload"]["non_improving_cycles"], 0);
    assert_eq!(progress[2]["payload"]["non_improving_cycles"], 1);
    assert_eq!(progress[3]["payload"]["non_improving_cycles"], 2);
    assert_eq!(progress[3]["payload"]["stop"], true);
    assert_eq!(progress[3]["payload"]["verified"], false);
    assert_eq!(progress[3]["payload"]["permissions_granted"], false);

    let settings = events(&trace, "execution_settings");
    assert_eq!(
        settings[0]["payload"]["progress_policy"],
        "shortest-path-progress/v1"
    );
    let contexts = events(&trace, "reasoning_context");
    assert_eq!(
        contexts[0]["payload"]["progress_policy"],
        "shortest-path-progress/v1"
    );
    remove_home(&cfg);
}

#[tokio::test]
async fn malformed_candidate_resets_only_the_consecutive_streak() {
    let cfg = config(true);
    let script = Arc::new(Script::new([A, "not-json", B, C]));
    let (outcome, store) = run(&cfg, script.clone()).await;
    let trace = store.task(&outcome.task_id).unwrap();
    let progress = events(&trace, "reasoning_progress");

    assert_eq!(outcome.status, "rejected");
    assert_eq!(outcome.stop_reason, "no_progress");
    assert_eq!(script.remaining(), 0);
    assert_eq!(progress.len(), 4);
    assert_eq!(progress[0]["payload"]["score"], 1);
    assert_eq!(progress[1]["payload"]["score"], Value::Null);
    assert_eq!(progress[1]["payload"]["scored"], false);
    assert_eq!(progress[1]["payload"]["best_score"], 1);
    assert_eq!(progress[1]["payload"]["non_improving_cycles"], 0);
    assert_eq!(progress[2]["payload"]["non_improving_cycles"], 1);
    assert_eq!(progress[3]["payload"]["stop"], true);
    remove_home(&cfg);
}

#[tokio::test]
async fn compiled_witness_is_accepted_before_any_plateau_assessment() {
    let cfg = config(true);
    let script = Arc::new(Script::new([ENDPOINT_MISMATCH, ENDPOINT_MISMATCH_B, GOOD]));
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run_profile(QUERY, "deep", ExecutionProfile::Recursive)
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let progress = events(&trace, "reasoning_progress");

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, GOOD);
    assert_eq!(outcome.calls, 3);
    assert_eq!(outcome.cycles, 3);
    assert_eq!(script.remaining(), 0);
    assert_eq!(progress.len(), 2);
    assert_eq!(progress[1]["payload"]["non_improving_cycles"], 1);
    assert_eq!(progress[1]["payload"]["stop"], false);
    let successful = events(&trace, "verification")
        .into_iter()
        .find(|event| event["payload"]["verified"] == true)
        .unwrap();
    assert_eq!(successful["payload"]["criterion"], "shortest_path/v1");
    assert_eq!(successful["payload"]["cycle"], 3);
    remove_home(&cfg);
}

#[tokio::test]
async fn provider_veto_is_unscored_and_does_not_prevent_a_later_verified_path() {
    let cfg = config(true);
    let script = Arc::new(Script::new([GOOD, GOOD]));
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let engine = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .with_verifier_provider(Arc::new(RejectFirstProvider(Mutex::new(0))))
        .unwrap();
    let outcome = engine
        .run_profile(QUERY, "research", ExecutionProfile::Recursive)
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, GOOD);
    assert_eq!(outcome.calls, 2);
    assert_eq!(outcome.cycles, 2);
    assert_eq!(script.remaining(), 0);
    let veto = events(&trace, "reasoning_progress_provider_veto");
    assert_eq!(veto.len(), 1);
    assert_eq!(veto[0]["payload"]["score"], Value::Null);
    assert_eq!(veto[0]["payload"]["scored"], false);
    assert_eq!(veto[0]["payload"]["provider_veto"], true);
    assert_eq!(veto[0]["payload"]["stop"], false);
    assert!(events(&trace, "reasoning_progress").is_empty());
    assert_eq!(events(&trace, "verification_provider_result").len(), 2);
    remove_home(&cfg);
}

#[tokio::test]
async fn failed_progress_audit_prevents_the_next_solver_dispatch() {
    let cfg = config(true);
    let script = Arc::new(Script::new([A, B]));
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    rusqlite::Connection::open(cfg.db())
        .unwrap()
        .execute_batch("CREATE TRIGGER refuse_path_progress BEFORE INSERT ON events WHEN NEW.kind='reasoning_progress' AND json_extract(NEW.payload,'$.criterion')='shortest_path/v1' BEGIN SELECT RAISE(ABORT,'path progress audit unavailable'); END;")
        .unwrap();

    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run_profile(QUERY, "research", ExecutionProfile::Recursive)
        .await;

    let outcome = outcome.unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.cycles, 1);
    assert_eq!(script.remaining(), 1);
    let trace = store.task(&outcome.task_id).unwrap();
    assert!(events(&trace, "reasoning_progress").is_empty());
    assert_eq!(
        events(&trace, "reasoning")
            .iter()
            .filter(|event| event["payload"]["node"]["role"] == "solver")
            .count(),
        1
    );
    remove_home(&cfg);
}

#[tokio::test]
async fn default_off_path_runs_keep_legacy_markers_and_no_progress_events() {
    let cfg = config(false);
    let script = Arc::new(Script::new([A, B, C, ENDPOINT_MISMATCH]));
    let (outcome, store) = run(&cfg, script.clone()).await;
    let trace = store.task(&outcome.task_id).unwrap();

    assert_eq!(outcome.status, "rejected");
    assert_eq!(outcome.calls, 4);
    assert_eq!(outcome.cycles, 4);
    assert_eq!(script.remaining(), 0);
    assert!(events(&trace, "reasoning_progress").is_empty());
    assert!(events(&trace, "reasoning_progress_provider_veto").is_empty());
    assert!(events(&trace, "execution_settings")[0]["payload"]
        .get("progress_policy")
        .is_none());
    assert!(events(&trace, "reasoning_context")[0]["payload"]
        .get("progress_policy")
        .is_none());
    remove_home(&cfg);
}

#[tokio::test]
async fn alone_profile_does_not_enable_path_progress_even_when_configured() {
    let cfg = config(true);
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script::new([A]));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run_profile(QUERY, "fast", ExecutionProfile::Alone)
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();

    assert_eq!(script.remaining(), 0);
    assert!(events(&trace, "reasoning_progress").is_empty());
    assert!(events(&trace, "reasoning_progress_provider_veto").is_empty());
    assert!(events(&trace, "execution_settings")[0]["payload"]
        .get("progress_policy")
        .is_none());
    assert!(events(&trace, "reasoning_context")[0]["payload"]
        .get("progress_policy")
        .is_none());
    assert!(events(&trace, "reasoning_boundary")[0]["payload"]
        .get("path_progress_state")
        .is_none());
    remove_home(&cfg);
}
