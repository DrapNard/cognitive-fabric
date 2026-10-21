use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    reasoning::finite_search::Settings,
    store::Store,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

const SAT: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const UNSAT: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":2}]}"#;
const WITNESS: &str = r#"{"a":1}"#;

struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);

impl Script {
    fn new(responses: Vec<(&'static str, &'static str)>) -> Self {
        Self(Mutex::new(responses.into()))
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
        let (expected_role, text) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("finite search unexpectedly invoked inference");
        assert_eq!(role, expected_role);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "finite-search-fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}

fn config(search: Option<usize>) -> Config {
    Config {
        home: std::env::temp_dir().join(format!("fabric-finite-search-{}", uuid::Uuid::new_v4())),
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 8,
        max_cycles: 4,
        max_tokens: 2400,
        max_tool_calls: 8,
        finite_search: search.map(|max_nodes| Settings { max_nodes }),
        ..Config::default()
    }
}

fn engine(cfg: &Config, script: Arc<Script>) -> (Engine, Arc<Store>) {
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    (
        Engine::with_backend(cfg.clone(), store.clone(), script),
        store,
    )
}

fn event<'a>(trace: &'a Value, kind: &str) -> Vec<&'a Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == kind)
        .collect()
}

fn finite_search_event(trace: &Value) -> Value {
    let matches = event(trace, "finite_search");
    assert_eq!(matches.len(), 1);
    matches[0].clone()
}

fn remove_home(cfg: &Config) {
    let _ = std::fs::remove_dir_all(&cfg.home);
}

#[tokio::test]
async fn deterministic_search_returns_checked_witness_without_model_calls() {
    let cfg = config(Some(4096));
    let script = Arc::new(Script::new(vec![]));
    let (engine, store) = engine(&cfg, script.clone());

    let outcome = engine.run(SAT, "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let search = finite_search_event(&trace);
    let payload = &search["payload"];
    let verification = event(&trace, "verification");

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 0);
    assert_eq!(outcome.cycles, 0);
    assert_eq!(outcome.resources.tool_calls, 3); // knowledge.search, solve, verify
    assert_eq!(outcome.stop_reason, "verified");
    assert_eq!(outcome.answer, WITNESS);
    assert_eq!(payload["criterion"], "finite_domain/v1");
    assert_eq!(
        payload["objective_sha256"],
        cogfabric::store::sha256(SAT.as_bytes())
    );
    assert_eq!(payload["outcome"], "witness");
    assert_eq!(payload["max_nodes"], 4096);
    assert!(payload["explored_nodes"].as_u64().unwrap() > 0);
    assert_eq!(payload["verified"], false);
    assert_eq!(payload["permissions_granted"], false);
    assert_eq!(search["source"], "jev-r");
    assert_eq!(verification.len(), 1);
    assert_eq!(verification[0]["payload"]["verified"], true);
    assert_eq!(verification[0]["payload"]["origin"], "finite_search");
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn exhaustive_search_reports_unsatisfiable_as_rejected_without_witness() {
    let cfg = config(Some(4096));
    let script = Arc::new(Script::new(vec![]));
    let (engine, store) = engine(&cfg, script.clone());

    let outcome = engine.run(UNSAT, "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let search = finite_search_event(&trace);

    assert_eq!(outcome.status, "rejected");
    assert_eq!(outcome.stop_reason, "proven_unsatisfiable");
    assert_eq!(outcome.calls, 0);
    assert_eq!(outcome.cycles, 0);
    assert_eq!(outcome.resources.tool_calls, 2); // knowledge.search and solve
    assert!(!outcome.answer.contains("\"a\""));
    assert!(outcome.answer.contains("No satisfying assignment exists"));
    assert_eq!(search["payload"]["outcome"], "unsatisfiable");
    assert_eq!(search["payload"]["verified"], false);
    assert!(event(&trace, "verification").is_empty());
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn node_limit_falls_back_to_existing_solver_and_keeps_search_charge() {
    let cfg = config(Some(1));
    let script = Arc::new(Script::new(vec![("solver", WITNESS)]));
    let (engine, store) = engine(&cfg, script.clone());

    let outcome = engine.run(SAT, "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let search = finite_search_event(&trace);
    let reservations = event(&trace, "tool_reservation");

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, WITNESS);
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 3);
    assert_eq!(search["payload"]["outcome"], "limit_reached");
    assert_eq!(search["payload"]["max_nodes"], 1);
    assert!(reservations
        .iter()
        .any(|reservation| { reservation["payload"]["capability"] == "constraints.solve" }));
    assert_eq!(
        reservations
            .iter()
            .filter(|reservation| reservation["payload"]["capability"] == "constraints.verify")
            .count(),
        1
    );
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn insufficient_remaining_tool_budget_skips_search_and_preserves_normal_check() {
    let mut cfg = config(Some(4096));
    cfg.max_tool_calls = 2; // Initial knowledge.search leaves only one slot.
    let script = Arc::new(Script::new(vec![("solver", WITNESS)]));
    let (engine, store) = engine(&cfg, script.clone());

    let outcome = engine.run(SAT, "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let reservations = event(&trace, "tool_reservation");

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 2);
    assert!(event(&trace, "finite_search").is_empty());
    assert!(!reservations
        .iter()
        .any(|reservation| { reservation["payload"]["capability"] == "constraints.solve" }));
    assert_eq!(
        reservations
            .iter()
            .filter(|reservation| reservation["payload"]["capability"] == "constraints.verify")
            .count(),
        1
    );
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn finite_search_audit_failure_fails_closed_before_inference_or_verification() {
    let cfg = config(Some(4096));
    let script = Arc::new(Script::new(vec![]));
    let (engine, store) = engine(&cfg, script.clone());
    rusqlite::Connection::open(cfg.db())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse_finite_search BEFORE INSERT ON events WHEN NEW.kind='finite_search' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;",
        )
        .unwrap();

    let outcome = engine.run(SAT, "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let reservations = event(&trace, "tool_reservation");

    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.stop_reason, "failure");
    assert_eq!(outcome.calls, 0);
    assert_eq!(outcome.resources.tool_calls, 2); // knowledge.search + solve reservation
    assert!(event(&trace, "finite_search").is_empty());
    assert!(event(&trace, "verification").is_empty());
    assert!(event(&trace, "error")[0]["payload"]["message"]
        .as_str()
        .unwrap()
        .contains("audit unavailable"));
    assert!(!reservations
        .iter()
        .any(|reservation| { reservation["payload"]["capability"] == "constraints.verify" }));
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn ready_cancellation_prevents_search_and_tool_reservations() {
    let cfg = config(Some(4096));
    let script = Arc::new(Script::new(vec![]));
    let (engine, store) = engine(&cfg, script.clone());

    let outcome = engine
        .run_until(SAT, "fast", std::future::ready(()))
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();

    assert_eq!(outcome.status, "cancelled");
    assert_eq!(outcome.calls, 0);
    assert_eq!(outcome.resources.tool_calls, 0);
    assert!(event(&trace, "finite_search").is_empty());
    assert!(event(&trace, "tool_reservation").is_empty());
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn absent_search_setting_preserves_model_solver_path() {
    let cfg = config(None);
    let script = Arc::new(Script::new(vec![("solver", WITNESS)]));
    let (engine, store) = engine(&cfg, script.clone());

    let outcome = engine.run(SAT, "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 2); // knowledge.search and verification
    assert!(event(&trace, "finite_search").is_empty());
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn alone_profile_does_not_use_or_charge_opt_in_search() {
    let cfg = config(Some(4096));
    let script = Arc::new(Script::new(vec![("solver", "model-only answer")]));
    let (engine, store) = engine(&cfg, script.clone());

    let outcome = engine
        .run_profile(SAT, "fast", ExecutionProfile::Alone)
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();

    assert_eq!(outcome.status, "unverified");
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 0);
    assert!(event(&trace, "finite_search").is_empty());
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn enabled_finite_search_does_not_intercept_other_verification_criteria() {
    let cfg = config(Some(4096));
    let script = Arc::new(Script::new(vec![("solver", "[1,2]")]));
    let (engine, store) = engine(&cfg, script.clone());

    let outcome = engine
        .run(r#"sort:{"values":[2,1]}"#, "fast")
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();

    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 2); // knowledge.search and sorting verification
    assert!(event(&trace, "finite_search").is_empty());
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}

#[derive(Deserialize)]
struct CorpusCase {
    input: String,
    verification: String,
}

#[derive(Deserialize)]
struct CorpusManifest {
    cases: Vec<CorpusLabel>,
}

#[derive(Deserialize)]
struct CorpusLabel {
    satisfiable: bool,
}

#[tokio::test]
async fn runtime_search_solves_the_public_corpus_without_inference() {
    let corpus = include_str!("../evaluation/finite_domain_cases.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<CorpusCase>(line).unwrap())
        .collect::<Vec<_>>();
    let manifest: CorpusManifest =
        serde_json::from_str(include_str!("../evaluation/finite_domain_manifest.json")).unwrap();
    assert_eq!(corpus.len(), 17);
    assert_eq!(manifest.cases.len(), corpus.len());

    let cfg = config(Some(4096));
    let script = Arc::new(Script::new(vec![]));
    let (engine, store) = engine(&cfg, script.clone());
    let mut satisfiable = 0;
    let mut unsatisfiable = 0;

    for (case, label) in corpus.iter().zip(&manifest.cases) {
        assert_eq!(case.verification, "finite_domain_v1");
        let outcome = engine.run(&case.input, "fast").await.unwrap();
        let trace = store.task(&outcome.task_id).unwrap();
        let search = finite_search_event(&trace);
        assert_eq!(search["payload"]["max_nodes"], 4096);
        assert_eq!(outcome.calls, 0);
        if label.satisfiable {
            satisfiable += 1;
            assert_eq!(outcome.status, "completed", "{}", case.input);
            assert_eq!(search["payload"]["outcome"], "witness");
            assert_eq!(outcome.resources.tool_calls, 3); // knowledge.search, solve, verify
            assert_eq!(event(&trace, "verification").len(), 1);
            assert_eq!(
                event(&trace, "verification")[0]["payload"]["verified"],
                true
            );
        } else {
            unsatisfiable += 1;
            assert_eq!(outcome.status, "rejected", "{}", case.input);
            assert_eq!(outcome.stop_reason, "proven_unsatisfiable");
            assert_eq!(search["payload"]["outcome"], "unsatisfiable");
            assert_eq!(outcome.resources.tool_calls, 2); // knowledge.search and solve
            assert!(event(&trace, "verification").is_empty());
        }
    }

    assert_eq!(satisfiable, 14);
    assert_eq!(unsatisfiable, 3);
    assert_eq!(script.remaining(), 0);

    remove_home(&cfg);
}
