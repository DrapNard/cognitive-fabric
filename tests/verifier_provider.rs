use anyhow::{anyhow, Result};
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    escalation::{Decision, Descriptor as EscalationDescriptor, EscalationPolicy, Request},
    store::{sha256, Store},
    VerificationClaim, VerificationFuture, VerificationRequest, VerifierDescriptor,
    VerifierProvider,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::Notify;

const SORT: &str = r#"sort:{"values":[3,1,3]}"#;
const VALID: &str = "[1,3,3]";
const INVALID: &str = "[1,3]";

struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);

impl Script {
    fn new(responses: impl IntoIterator<Item = (&'static str, &'static str)>) -> Self {
        Self(Mutex::new(responses.into_iter().collect()))
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
        let (expected, text) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected {role} inference"));
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "verifier-provider-fixture".into(),
                input_tokens: Some(2),
                output_tokens: Some(2),
                elapsed_ms: 1.0,
                queue_wait_ms: Some(0.0),
                provider_metrics: None,
            })
        })
    }
}

#[derive(Clone)]
enum Behavior {
    Accept,
    Reject(&'static str),
    RejectFirst,
    Error,
    InvalidClaim,
    Delay(Duration),
    Wait(Arc<Notify>),
}

struct Provider {
    descriptor: Mutex<VerifierDescriptor>,
    behavior: Behavior,
    requests: Mutex<Vec<VerificationRequest>>,
    calls: std::sync::atomic::AtomicUsize,
    dropped: Arc<AtomicBool>,
}

impl Provider {
    fn new(behavior: Behavior) -> Arc<Self> {
        Self::with_descriptor(behavior, descriptor("1.0.0", 'a'))
    }

    fn with_descriptor(behavior: Behavior, descriptor: VerifierDescriptor) -> Arc<Self> {
        Arc::new(Self {
            descriptor: Mutex::new(descriptor),
            behavior,
            requests: Mutex::new(Vec::new()),
            calls: std::sync::atomic::AtomicUsize::new(0),
            dropped: Arc::new(AtomicBool::new(false)),
        })
    }

    fn requests(&self) -> Vec<VerificationRequest> {
        self.requests.lock().unwrap().clone()
    }

    fn set_descriptor(&self, value: VerifierDescriptor) {
        *self.descriptor.lock().unwrap() = value;
    }
}

impl VerifierProvider for Provider {
    fn descriptor(&self) -> VerifierDescriptor {
        self.descriptor.lock().unwrap().clone()
    }

    fn verify(&self, request: VerificationRequest) -> VerificationFuture<'_> {
        self.requests.lock().unwrap().push(request);
        let dropped = self.dropped.clone();
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        match self.behavior.clone() {
            Behavior::Accept => Box::pin(async { Ok(VerificationClaim::accepted()) }),
            Behavior::RejectFirst if call == 0 => {
                Box::pin(async { Ok(VerificationClaim::rejected("provider_veto")) })
            }
            Behavior::RejectFirst => Box::pin(async { Ok(VerificationClaim::accepted()) }),
            Behavior::Reject(code) => {
                Box::pin(async move { Ok(VerificationClaim::rejected(code)) })
            }
            Behavior::Error => Box::pin(async { Err(anyhow!("secret provider diagnostic")) }),
            Behavior::InvalidClaim => Box::pin(async {
                Ok(VerificationClaim {
                    accepted: true,
                    reason_code: Some("invalid_acceptance_shape".into()),
                })
            }),
            Behavior::Delay(duration) => Box::pin(async move {
                let _drop = DropMark(dropped);
                tokio::time::sleep(duration).await;
                Ok(VerificationClaim::accepted())
            }),
            Behavior::Wait(started) => Box::pin(async move {
                let _drop = DropMark(dropped);
                started.notify_one();
                std::future::pending::<Result<VerificationClaim>>().await
            }),
        }
    }
}

struct DropMark(Arc<AtomicBool>);
impl Drop for DropMark {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn descriptor(version: &str, hash_byte: char) -> VerifierDescriptor {
    VerifierDescriptor {
        id: "fixture.independent_sort".into(),
        version: version.into(),
        implementation_sha256: hash_byte.to_string().repeat(64),
    }
}

fn config(timeout_seconds: u64) -> Config {
    Config {
        home: std::env::temp_dir()
            .join(format!("fabric-verifier-provider-{}", uuid::Uuid::new_v4())),
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 4,
        max_cycles: 2,
        max_tokens: 1600,
        max_tool_calls: 4,
        timeout_seconds,
        max_wall_seconds: timeout_seconds,
        ..Config::default()
    }
}

fn new_engine(cfg: Config, script: Arc<Script>) -> (Engine, Arc<Store>) {
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    (Engine::with_backend(cfg, store.clone(), script), store)
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

fn install(engine: Engine, provider: Arc<Provider>) -> Engine {
    engine.with_verifier_provider(provider).unwrap()
}

#[test]
fn invalid_provider_descriptors_are_rejected_before_engine_installation() {
    for invalid in [
        VerifierDescriptor {
            version: "not-semver".into(),
            ..descriptor("1.0.0", 'a')
        },
        VerifierDescriptor {
            implementation_sha256: "A".repeat(64),
            ..descriptor("1.0.0", 'a')
        },
        VerifierDescriptor {
            implementation_sha256: "short".into(),
            ..descriptor("1.0.0", 'a')
        },
    ] {
        let cfg = config(10);
        let (engine, _) = new_engine(cfg.clone(), Arc::new(Script::new([])));
        let provider = Provider::with_descriptor(Behavior::Accept, invalid);
        assert!(engine.with_verifier_provider(provider).is_err());
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn accepted_provider_and_compiled_checker_complete_with_original_budget_and_audits() {
    let cfg = config(10);
    let script = Arc::new(Script::new([("solver", VALID)]));
    let (engine, store) = new_engine(cfg.clone(), script.clone());
    let provider = Provider::new(Behavior::Accept);
    let engine = install(engine, provider.clone());

    let outcome = engine.run(SORT, "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let provider_request = &provider.requests()[0];
    let snapshot = events(&trace, "verification_provider_snapshot");
    let intents = events(&trace, "verification_provider_intent");
    let results = events(&trace, "verification_provider_result");
    let proofs = events(&trace, "verification");

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, VALID);
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.cycles, 1);
    assert_eq!(outcome.resources.tool_calls, 2); // knowledge.search + sorting.verify
    assert_eq!(outcome.resources.reserved_output_tokens, 384);
    assert_eq!(script.remaining(), 0);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(intents.len(), 1);
    assert_eq!(results.len(), 1);
    assert_eq!(proofs.len(), 1);
    assert_eq!(
        snapshot[0]["payload"]["provider"]["id"],
        "fixture.independent_sort"
    );
    assert_eq!(snapshot[0]["payload"]["compiled_checker_required"], true);
    assert_eq!(snapshot[0]["payload"]["implementation_attested"], false);
    assert_eq!(
        intents[0]["payload"]["answer_sha256"],
        sha256(VALID.as_bytes())
    );
    assert_eq!(intents[0]["payload"]["permissions_granted"], false);
    assert_eq!(results[0]["payload"]["provider_accepted"], true);
    assert_eq!(results[0]["payload"]["compiled_accepted"], true);
    assert_eq!(results[0]["payload"]["verified"], true);
    assert_eq!(
        proofs[0]["payload"]["provider"],
        snapshot[0]["payload"]["provider"]
    );
    assert_eq!(provider_request.criterion, "integer_sort/v1");
    assert_eq!(provider_request.objective, SORT);
    assert_eq!(provider_request.objective_sha256, sha256(SORT.as_bytes()));
    assert_eq!(provider_request.answer, VALID);
    assert_eq!(provider_request.answer_sha256, sha256(VALID.as_bytes()));
    assert!(provider_request.deadline > std::time::Instant::now());
    remove_home(&cfg);
}

#[tokio::test]
async fn direct_finite_search_witness_is_checked_by_provider_without_inference_replay() {
    let query = r#"csp:{"variables":[{"id":"a","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1}]}"#;
    let cfg = Config {
        finite_search: Some(cogfabric::reasoning::finite_search::Settings { max_nodes: 32 }),
        ..config(10)
    };
    let (engine, store) = new_engine(cfg.clone(), Arc::new(Script::new([])));
    let provider = Provider::new(Behavior::Accept);
    let outcome = install(engine, provider.clone())
        .run_profile(query, "fast", ExecutionProfile::Recursive)
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, r#"{"a":1}"#);
    assert_eq!(outcome.calls, 0);
    assert_eq!(outcome.resources.tool_calls, 2); // constraints.solve + constraints.verify
    assert_eq!(events(&trace, "finite_search").len(), 1);
    assert_eq!(events(&trace, "verification_provider_intent").len(), 1);
    assert_eq!(
        events(&trace, "verification_provider_result")[0]["payload"]["verified"],
        true
    );
    assert_eq!(provider.requests().len(), 1);
    remove_home(&cfg);

    let cfg = Config {
        finite_search: Some(cogfabric::reasoning::finite_search::Settings { max_nodes: 32 }),
        ..config(10)
    };
    let fallback = Arc::new(Script::new([("solver", r#"{"a":0}"#)]));
    let (engine, store) = new_engine(cfg.clone(), fallback.clone());
    let provider = Provider::new(Behavior::Reject("provider_veto"));
    let outcome = install(engine, provider.clone())
        .run_profile(query, "fast", ExecutionProfile::Recursive)
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    assert_ne!(outcome.status, "completed");
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 3); // search, checker, fallback checker
    assert_eq!(events(&trace, "finite_search").len(), 1);
    assert_eq!(events(&trace, "verification_provider_intent").len(), 2);
    assert_eq!(
        events(&trace, "verification_provider_result")[0]["payload"]["verified"],
        false
    );
    assert_eq!(events(&trace, "verification_provider_result").len(), 2);
    assert_eq!(
        events(&trace, "verification").last().unwrap()["payload"]["verified"],
        false
    );
    assert_eq!(fallback.remaining(), 0);
    let requests = provider.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].answer, r#"{"a":1}"#);
    assert_eq!(requests[1].answer, r#"{"a":0}"#);
    remove_home(&cfg);
}

#[tokio::test]
async fn provider_acceptance_cannot_override_compiled_rejection_and_rejection_is_audited() {
    for (answer, behavior, expected_code) in [
        (INVALID, Behavior::Accept, "compiled_check_rejected"),
        (
            VALID,
            Behavior::Reject("provider_disagrees"),
            "provider_disagrees",
        ),
    ] {
        let cfg = config(10);
        let script = Arc::new(Script::new([("solver", answer)]));
        let (engine, store) = new_engine(cfg.clone(), script);
        let provider = Provider::new(behavior);
        let outcome = install(engine, provider.clone())
            .run(SORT, "fast")
            .await
            .unwrap();
        let trace = store.task(&outcome.task_id).unwrap();
        let result = events(&trace, "verification_provider_result")[0];
        let proof = events(&trace, "verification")[0];

        assert_eq!(outcome.status, "rejected");
        assert_eq!(result["payload"]["verified"], false);
        assert_eq!(result["payload"]["reason_code"], expected_code);
        assert_eq!(proof["payload"]["verified"], false);
        assert_eq!(outcome.resources.tool_calls, 2);
        assert_eq!(provider.requests().len(), 1);
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn provider_errors_and_malformed_claims_fail_closed_without_leaking_provider_text() {
    for behavior in [Behavior::Error, Behavior::InvalidClaim] {
        let cfg = config(10);
        let script = Arc::new(Script::new([("solver", VALID)]));
        let (engine, store) = new_engine(cfg.clone(), script);
        let provider = Provider::new(behavior);
        let outcome = install(engine, provider).run(SORT, "fast").await.unwrap();
        let trace = store.task(&outcome.task_id).unwrap();
        let result = events(&trace, "verification_provider_result")[0];

        assert_eq!(outcome.status, "rejected");
        assert_eq!(result["payload"]["verified"], false);
        assert_eq!(result["payload"]["reason_code"], "provider_unavailable");
        let serialized = serde_json::to_string(&trace).unwrap();
        assert!(!serialized.contains("secret provider diagnostic"));
        assert!(!outcome.answer.contains("secret provider diagnostic"));
        assert_eq!(outcome.resources.tool_calls, 2);
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn provider_timeout_exhausts_the_original_task_without_acceptance_or_replay() {
    let cfg = config(1);
    let script = Arc::new(Script::new([("solver", VALID)]));
    let (engine, store) = new_engine(cfg.clone(), script);
    let provider = Provider::new(Behavior::Delay(Duration::from_secs(2)));
    let outcome = tokio::time::timeout(
        Duration::from_secs(4),
        install(engine, provider.clone()).run(SORT, "fast"),
    )
    .await
    .unwrap()
    .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    assert_eq!(outcome.status, "exhausted");
    assert_eq!(events(&trace, "verification_provider_intent").len(), 1);
    assert!(events(&trace, "verification_provider_result").is_empty());
    assert!(events(&trace, "verification").is_empty());
    assert!(provider.dropped.load(Ordering::SeqCst));
    assert_eq!(outcome.resources.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 2);
    remove_home(&cfg);
}

#[tokio::test]
async fn cancellation_drops_provider_future_and_never_writes_acceptance() {
    let cfg = config(10);
    let script = Arc::new(Script::new([("solver", VALID)]));
    let (engine, store) = new_engine(cfg.clone(), script);
    let started = Arc::new(Notify::new());
    let provider = Provider::new(Behavior::Wait(started.clone()));
    let engine = install(engine, provider.clone());
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        engine
            .run_until(SORT, "fast", async move {
                let _ = cancel_rx.await;
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), started.notified())
        .await
        .unwrap();
    cancel_tx.send(()).unwrap();
    let outcome = task.await.unwrap().unwrap();
    let trace = store.task(&outcome.task_id).unwrap();

    assert_eq!(outcome.status, "cancelled");
    assert_eq!(events(&trace, "verification_provider_snapshot").len(), 1);
    assert_eq!(events(&trace, "verification_provider_intent").len(), 1);
    assert!(events(&trace, "verification_provider_result").is_empty());
    assert!(events(&trace, "verification").is_empty());
    assert!(provider.dropped.load(Ordering::SeqCst));
    assert_eq!(outcome.resources.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 2);
    remove_home(&cfg);
}

#[tokio::test]
async fn required_intent_audit_failure_prevents_callback_and_later_inference() {
    let cfg = config(10);
    let script = Arc::new(Script::new([
        ("planner", "invalid"),
        ("solver", INVALID),
        ("solver", VALID),
    ]));
    let (engine, store) = new_engine(cfg.clone(), script.clone());
    let provider = Provider::new(Behavior::Accept);
    let engine = install(engine, provider.clone());
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_verifier_intent BEFORE INSERT ON events
         WHEN NEW.kind='verification_provider_intent'
         BEGIN SELECT RAISE(ABORT,'fixture audit unavailable'); END;",
    )
    .unwrap();

    let result = engine.run(SORT, "deep").await.unwrap();
    assert_eq!(result.status, "failed");
    assert!(provider.requests().is_empty());
    assert_eq!(script.remaining(), 1); // no next solver dispatch after the failed required intent
    let task_id: String = db
        .query_row(
            "SELECT id FROM tasks ORDER BY created DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let trace = store.task(&task_id).unwrap();
    assert_eq!(events(&trace, "verification_provider_snapshot").len(), 1);
    assert!(events(&trace, "verification_provider_intent").is_empty());
    assert!(events(&trace, "verification_provider_result").is_empty());
    assert!(events(&trace, "verification").is_empty());
    let reasoning = events(&trace, "reasoning");
    assert_eq!(reasoning.len(), 2);
    assert_eq!(reasoning[0]["payload"]["node"]["role"], "planner");
    assert_eq!(reasoning[1]["payload"]["node"]["role"], "solver");
    drop(db);
    remove_home(&cfg);
}

#[tokio::test]
async fn every_required_provider_audit_failure_fails_closed() {
    for (failure_kind, expected_callbacks, expected_solver_calls) in [
        ("verification_provider_snapshot", 0, 0),
        ("verification_provider_result", 1, 1),
        ("verification", 1, 1),
    ] {
        let cfg = config(10);
        let script = Arc::new(Script::new([("solver", VALID), ("solver", VALID)]));
        let (engine, store) = new_engine(cfg.clone(), script.clone());
        let provider = Provider::new(Behavior::Accept);
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        db.execute_batch(&format!(
            "CREATE TRIGGER reject_required_event BEFORE INSERT ON events
             WHEN NEW.kind='{failure_kind}'
             BEGIN SELECT RAISE(ABORT,'fixture audit unavailable'); END;"
        ))
        .unwrap();

        let outcome = install(engine, provider.clone())
            .run(SORT, "fast")
            .await
            .unwrap();
        let task_id: String = db
            .query_row(
                "SELECT id FROM tasks ORDER BY created DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let trace = store.task(&task_id).unwrap();
        assert_eq!(outcome.status, "failed", "{failure_kind}");
        assert_eq!(
            provider.requests().len(),
            expected_callbacks,
            "{failure_kind}"
        );
        assert_eq!(
            script.remaining(),
            2 - expected_solver_calls,
            "{failure_kind}"
        );
        assert_ne!(outcome.status, "completed", "{failure_kind}");
        if failure_kind == "verification_provider_result" {
            assert_eq!(events(&trace, "verification_provider_intent").len(), 1);
            assert!(events(&trace, "verification_provider_result").is_empty());
            assert!(events(&trace, "verification").is_empty());
        } else if failure_kind == "verification" {
            assert_eq!(events(&trace, "verification_provider_result").len(), 1);
            assert!(events(&trace, "verification").is_empty());
        } else {
            assert!(events(&trace, "verification_provider_snapshot").is_empty());
            assert!(events(&trace, "verification_provider_intent").is_empty());
        }
        drop(db);
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn default_unknown_and_alone_paths_do_not_invoke_or_snapshot_optional_provider() {
    let cfg = config(10);
    let script = Arc::new(Script::new([("solver", VALID)]));
    let (default_engine, store) = new_engine(cfg.clone(), script.clone());
    let default = default_engine.run(SORT, "fast").await.unwrap();
    let default_trace = store.task(&default.task_id).unwrap();
    assert_eq!(default.status, "completed");
    assert!(events(&default_trace, "verification_provider_snapshot").is_empty());
    assert!(events(&default_trace, "verification_provider_intent").is_empty());
    assert!(events(&default_trace, "verification_provider_result").is_empty());
    assert_eq!(
        events(&default_trace, "verification")[0]["payload"]["verified"],
        true
    );

    let unknown_script = Arc::new(Script::new([("solver", "plain answer")]));
    let (unknown_engine, unknown_store) = new_engine(cfg.clone(), unknown_script);
    let unknown_provider = Provider::new(Behavior::Accept);
    let unknown = install(unknown_engine, unknown_provider.clone())
        .run("Explain an ordinary goal", "fast")
        .await
        .unwrap();
    let unknown_trace = unknown_store.task(&unknown.task_id).unwrap();
    assert_eq!(unknown.status, "unverified");
    assert!(unknown_provider.requests().is_empty());
    assert!(events(&unknown_trace, "verification_provider_snapshot").is_empty());

    let alone_script = Arc::new(Script::new([("solver", VALID)]));
    let (alone_engine, alone_store) = new_engine(cfg.clone(), alone_script);
    let alone_provider = Provider::new(Behavior::Accept);
    let alone = install(alone_engine, alone_provider.clone())
        .run_profile(SORT, "fast", ExecutionProfile::Alone)
        .await
        .unwrap();
    let alone_trace = alone_store.task(&alone.task_id).unwrap();
    assert_eq!(alone.status, "unverified");
    assert!(alone_provider.requests().is_empty());
    assert!(events(&alone_trace, "verification_provider_snapshot").is_empty());

    let malformed_script = Arc::new(Script::new([]));
    let (malformed_engine, malformed_store) = new_engine(cfg.clone(), malformed_script);
    let malformed_provider = Provider::new(Behavior::Accept);
    let malformed = install(malformed_engine, malformed_provider.clone())
        .run("sort:{\"values\":[]}", "fast")
        .await
        .unwrap();
    let malformed_trace = malformed_store.task(&malformed.task_id).unwrap();
    assert_eq!(malformed.status, "failed");
    assert!(malformed_provider.requests().is_empty());
    assert!(events(&malformed_trace, "verification_provider_snapshot").is_empty());
    assert!(events(&malformed_trace, "verification_provider_intent").is_empty());
    remove_home(&cfg);
}

struct CapturePolicy(Arc<Mutex<Vec<String>>>);
impl EscalationPolicy for CapturePolicy {
    fn descriptor(&self) -> EscalationDescriptor {
        EscalationDescriptor {
            id: "fixture.verifier_fingerprint".into(),
            version: "1.0.0".into(),
        }
    }
    fn decide(&self, request: &Request) -> Result<Decision> {
        self.0
            .lock()
            .unwrap()
            .push(request.configuration_sha256.clone());
        Ok(Decision {
            allow: true,
            reason_code: "fixture_continue".into(),
        })
    }
}

fn review_engine(
    cfg: Config,
    provider: Option<Arc<Provider>>,
    fingerprints: Arc<Mutex<Vec<String>>>,
) -> (Engine, Arc<Store>) {
    let script = Arc::new(Script::new([
        ("planner", "invalid"),
        ("solver", INVALID),
        ("solver", VALID),
    ]));
    let (mut engine, store) = new_engine(cfg, script);
    engine = engine
        .with_escalation_policy(Arc::new(CapturePolicy(fingerprints)))
        .unwrap();
    if let Some(provider) = provider {
        engine = install(engine, provider);
    }
    (engine, store)
}

#[tokio::test]
async fn provider_identity_changes_policy_fingerprints_and_engine_clones_keep_the_snapshot() {
    let cfg = config(10);
    let baseline_fingerprints = Arc::new(Mutex::new(Vec::new()));
    let (baseline_engine, baseline_store) =
        review_engine(cfg.clone(), None, baseline_fingerprints.clone());
    let baseline = baseline_engine.run(SORT, "deep").await.unwrap();
    assert_eq!(baseline.status, "completed");

    let provider = Provider::new(Behavior::Accept);
    let (mut engine, _) = review_engine(
        cfg.clone(),
        Some(provider.clone()),
        Arc::new(Mutex::new(Vec::new())),
    );
    let clone = engine.clone();
    let pinned = engine.verifier_provider_descriptor().unwrap().clone();
    provider.set_descriptor(descriptor("2.0.0", 'b'));
    assert_eq!(engine.verifier_provider_descriptor(), Some(&pinned));
    assert_eq!(clone.verifier_provider_descriptor(), Some(&pinned));

    let provider_one_fingerprints = Arc::new(Mutex::new(Vec::new()));
    engine = engine
        .with_escalation_policy(Arc::new(CapturePolicy(provider_one_fingerprints.clone())))
        .unwrap();
    let outcome = engine.run(SORT, "deep").await.unwrap();
    let trace = baseline_store.task(&outcome.task_id).unwrap();
    assert_eq!(outcome.status, "completed");
    let baseline_values = baseline_fingerprints.lock().unwrap().clone();
    let provider_values = provider_one_fingerprints.lock().unwrap().clone();
    assert!(!baseline_values.is_empty());
    assert!(!provider_values.is_empty());
    assert!(baseline_values
        .iter()
        .all(|value| value != &provider_values[0]));
    assert_eq!(
        events(&trace, "verification_provider_snapshot")[0]["payload"]["provider"]["version"],
        "1.0.0"
    );

    let second_provider = Provider::with_descriptor(Behavior::Accept, descriptor("2.0.0", 'b'));
    let version_two_fingerprints = Arc::new(Mutex::new(Vec::new()));
    let (version_two_engine, version_two_store) = review_engine(
        cfg.clone(),
        Some(second_provider),
        version_two_fingerprints.clone(),
    );
    let version_two = version_two_engine.run(SORT, "deep").await.unwrap();
    assert_eq!(version_two.status, "completed");
    let version_two_trace = version_two_store.task(&version_two.task_id).unwrap();
    assert_eq!(
        events(&version_two_trace, "verification_provider_snapshot")[0]["payload"]["provider"]
            ["version"],
        "2.0.0"
    );
    assert_ne!(
        provider_values[0],
        version_two_fingerprints.lock().unwrap()[0]
    );
    remove_home(&cfg);
}

#[tokio::test]
async fn published_memory_reuse_runs_fresh_provider_and_compiled_verification() -> Result<()> {
    let cfg = config(10);
    let (engine, store) = new_engine(cfg.clone(), Arc::new(Script::new([])));
    let prior = store.create_task(SORT, "fast").unwrap();
    store.finish_task(&prior, "unverified", VALID).unwrap();
    store
        .publish_memory(&prior, "episode", "Fixture memory requires fresh checking")
        .unwrap();
    let provider = Provider::new(Behavior::Accept);
    let outcome = install(engine, provider.clone())
        .run(SORT, "fast")
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let proof = events(&trace, "verification")[0];

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.stop_reason, "verified_memory");
    assert_eq!(outcome.answer, VALID);
    assert_eq!(outcome.calls, 0);
    assert_eq!(outcome.resources.tool_calls, 2);
    assert_eq!(provider.requests().len(), 1);
    assert_eq!(proof["payload"]["origin"], "published_memory");
    assert_eq!(
        proof["payload"]["memory_id"],
        store
            .search_memories("sort values")?
            .first()
            .map(|m| m.id.clone())
            .unwrap_or_default()
    );
    assert_eq!(proof["payload"]["verified"], true);
    assert_eq!(
        events(&trace, "verification_provider_result")[0]["payload"]["verified"],
        true
    );

    let rejected_script = Arc::new(Script::new([("solver", INVALID)]));
    let (rejected_engine, rejected_store) = new_engine(cfg.clone(), rejected_script.clone());
    let veto = Provider::new(Behavior::Reject("memory_recheck_veto"));
    let rejected = install(rejected_engine, veto.clone())
        .run(SORT, "fast")
        .await
        .unwrap();
    let rejected_trace = rejected_store.task(&rejected.task_id).unwrap();
    assert_ne!(rejected.status, "completed");
    assert_ne!(rejected.stop_reason, "verified_memory");
    assert_eq!(rejected.calls, 1);
    assert_eq!(veto.requests().len(), 2); // stored answer and fallback solver answer
    assert_eq!(
        events(&rejected_trace, "verification")[0]["payload"]["origin"],
        "published_memory"
    );
    assert_eq!(
        events(&rejected_trace, "verification")[0]["payload"]["verified"],
        false
    );
    assert_eq!(rejected_script.remaining(), 0);
    remove_home(&cfg);
    Ok(())
}

#[tokio::test]
async fn custom_provider_task_recovery_guards_fail_before_any_mutation() {
    for (kind, payload) in [
        (
            "verification_provider_snapshot",
            json!({"provider":descriptor("1.0.0",'a')}),
        ),
        (
            "verification_provider_intent",
            json!({"provider":descriptor("1.0.0",'a')}),
        ),
        (
            "verification_provider_result",
            json!({"provider":descriptor("1.0.0",'a')}),
        ),
        (
            "verification",
            json!({"provider":descriptor("1.0.0",'a'),"verified":true}),
        ),
    ] {
        let cfg = config(10);
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        let task = store.create_task(SORT, "deep").unwrap();
        store.finish_task(&task, "interrupted", "").unwrap();
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        db.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,?2,'security-kernel',?3,unixepoch())",
            rusqlite::params![task, kind, payload.to_string()],
        )
        .unwrap();
        drop(db);
        let before = store.task(&task).unwrap();
        for rejected in [
            store.recover_verified_result(&task).is_err(),
            cogfabric::recovery::continuation::preview(&store, &task).is_err(),
            store.recover_advisory_result(&task, true).is_err(),
            store.recover_revision_result(&task, true).is_err(),
        ] {
            assert!(rejected, "{kind}");
            assert_eq!(store.task(&task).unwrap(), before, "{kind}");
        }
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn recovery_guard_fails_closed_for_malformed_and_oversized_verification_payloads() {
    for payload in [
        "{\"provider\":".to_owned(),
        format!(
            "{{\"ordinary\":true,\"padding\":\"{}\"}}",
            "x".repeat(300_000)
        ),
    ] {
        let cfg = config(10);
        let store = Store::open(&cfg.db()).unwrap();
        let task = store.create_task(SORT, "deep").unwrap();
        store.finish_task(&task, "interrupted", "").unwrap();
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        db.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'verification','verifier-r',?2,unixepoch())",
            rusqlite::params![task, payload],
        )
        .unwrap();
        drop(db);
        let before = recovery_state(&cfg, &task);
        assert!(store.recover_verified_result(&task).is_err());
        assert_eq!(recovery_state(&cfg, &task), before);
        remove_home(&cfg);
    }
}

fn recovery_state(cfg: &Config, task: &str) -> (String, i64) {
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.query_row(
        "SELECT status,(SELECT count(*) FROM events WHERE task_id=?1) FROM tasks WHERE id=?1",
        [task],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

#[tokio::test]
async fn advice_observations_are_scoped_by_provider_identity() {
    let mut cfg = config(10);
    cfg.advice_guard = Some(cogfabric::reasoning::advice_guard::Settings {
        min_observations: 1,
        sample_size: 1,
        window_seconds: 60,
        trip_failure_basis_points: 10000,
    });
    let captures = [
        None,
        Some(Provider::with_descriptor(
            Behavior::Accept,
            descriptor("2.0.0", 'b'),
        )),
        Some(Provider::new(Behavior::Accept)),
    ];
    let mut scopes = Vec::new();
    let mut stores = Vec::new();
    for provider in captures {
        let (engine, store) =
            review_engine(cfg.clone(), provider, Arc::new(Mutex::new(Vec::new())));
        let outcome = engine.run(SORT, "deep").await.unwrap();
        assert_eq!(outcome.status, "completed");
        let trace = store.task(&outcome.task_id).unwrap();
        let decision = events(&trace, "advice_guard_decision")[0];
        scopes.push(
            decision["payload"]["scope_sha256"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        stores.push(store);
    }
    assert_ne!(scopes[0], scopes[1]);
    assert_ne!(scopes[1], scopes[2]);
    assert_ne!(scopes[0], scopes[2]);
    remove_home(&cfg);
}

#[tokio::test]
async fn provider_veto_of_valid_finite_domain_answer_is_unscored_and_can_continue() {
    let mut cfg = config(10);
    cfg.progress_stopping = Some(cogfabric::reasoning::progress::Settings { plateau_cycles: 2 });
    let query = r#"csp:{"variables":[{"id":"a","domain":[0,1]},{"id":"b","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1},{"variable":"b","coefficient":1}],"relation":"eq","rhs":1}]}"#;
    let first = r#"{"a":0,"b":1}"#;
    let second = r#"{"a":1,"b":0}"#;
    let script = Arc::new(Script::new([("solver", first), ("solver", second)]));
    let (engine, store) = new_engine(cfg.clone(), script.clone());
    let provider = Provider::new(Behavior::RejectFirst);
    let outcome = install(engine, provider.clone())
        .run_profile(query, "deep", ExecutionProfile::Recursive)
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let veto = events(&trace, "reasoning_progress_provider_veto");
    let results = events(&trace, "verification_provider_result");

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, second);
    assert_eq!(outcome.cycles, 2);
    assert_eq!(outcome.calls, 2);
    assert_eq!(outcome.resources.tool_calls, 2);
    assert_eq!(script.remaining(), 0);
    assert_eq!(veto.len(), 1);
    assert_eq!(veto[0]["payload"]["scored"], false);
    assert_eq!(veto[0]["payload"]["provider_veto"], true);
    assert_eq!(veto[0]["payload"]["stop"], false);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["payload"]["compiled_accepted"], true);
    assert_eq!(results[0]["payload"]["provider_accepted"], false);
    assert_eq!(results[0]["payload"]["verified"], false);
    assert_eq!(results[1]["payload"]["verified"], true);
    remove_home(&cfg);
}
