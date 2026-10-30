use super::*;
use crate::{engine::Engine, registry};
use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};
const QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[1,2,3]},{"id":"b","domain":[1,2,3]}],"constraints":[{"kind":"all_different","variables":["a","b"]},{"kind":"linear","terms":[{"variable":"a","coefficient":1},{"variable":"b","coefficient":1}],"relation":"eq","rhs":4}]}"#;
const GOOD: &str = r#"{"a":1,"b":3}"#;
const BAD: &str = r#"{"a":2,"b":2}"#;
struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        assert!(prompt.contains("Satisfy every linear relation"));
        let (expected, text) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("Unexpected extra inference");
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
fn config() -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        synthesize_results: true,
        ..Config::default()
    }
}
#[tokio::test]
async fn independently_revises_constraint_failure_preserves_witness_and_audits_all_charges() {
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest:registry::Manifest=serde_json::from_value(json!({"id":"data.constraints.s1","version":"1.0.0","kind":"checker","checker":"finite_domain/v1","description":"Independent finite-domain checker","tags":["csp"],"source":"Synthetic regression fixture","license":"CC0-1.0"})).unwrap();
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let script = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid graph"),
            ("solver", BAD),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let engine = Engine::with_backend(config(), store.clone(), script.clone());
    let outcome = engine.run(QUERY, "deep").await.unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, GOOD);
    assert_eq!(outcome.calls, 3);
    assert_eq!(outcome.cycles, 2);
    assert_eq!(outcome.resources.tool_calls, 3); // Retrieval plus both independent checks.
    assert!(outcome
        .evidence
        .contains(&"verified:finite_domain/v1".into()));
    assert!(script.0.lock().unwrap().is_empty());
    assert!(store.inference_audit(&outcome.task_id).is_ok());
    let trace = store.task(&outcome.task_id).unwrap();
    let checks = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "verification")
        .collect::<Vec<_>>();
    assert_eq!(checks.len(), 2);
    assert_eq!(checks[0]["payload"]["verified"], false);
    assert_eq!(checks[1]["payload"]["verified"], true);
    assert_eq!(checks[1]["payload"]["expert"]["id"], manifest.id);
    assert_eq!(
        checks[1]["payload"]["answer_hash"],
        crate::store::sha256(GOOD.as_bytes())
    );
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "synthesis"));
}
#[tokio::test]
async fn invalid_criteria_forged_success_budget_denial_and_stagnation_never_verify() {
    for variant in 0..5 {
        let store = Arc::new(Store::in_memory().unwrap());
        let mut cfg = config();
        if variant == 2 {
            cfg.max_tool_calls = 1;
        }
        let answers = match variant {
            0 => vec![],
            1 => vec![("solver", r#"{"verified":true}"#)],
            2 => vec![("solver", GOOD)],
            3 => vec![
                ("planner", "invalid graph"),
                ("solver", BAD),
                ("solver", BAD),
            ],
            _ => vec![("solver", "unsatisfiable")],
        };
        let script = Arc::new(Script(Mutex::new(answers.into())));
        let engine = Engine::with_backend(cfg, store.clone(), script.clone());
        let query = if variant == 0 {
            "csp:{\"variables\":[],\"constraints\":[]}"
        } else {
            QUERY
        };
        let outcome = engine
            .run(query, if variant == 3 { "deep" } else { "fast" })
            .await
            .unwrap();
        assert_eq!(
            outcome.status,
            if matches!(variant, 0 | 2) {
                "failed"
            } else {
                "rejected"
            },
            "variant {variant}"
        );
        assert!(outcome.evidence.is_empty());
        assert!(script.0.lock().unwrap().is_empty());
        if variant == 3 {
            assert_eq!(outcome.stop_reason, "no_progress");
            assert_eq!(outcome.calls, 3);
        }
    }
}
#[tokio::test]
async fn policy_and_verification_audit_failures_cannot_create_verified_evidence() {
    for kind in ["tool_reservation", "verification"] {
        let store = Arc::new(Store::in_memory().unwrap());
        let condition = if kind == "tool_reservation" {
            "AND json_extract(NEW.payload,'$.capability')='constraints.verify'"
        } else {
            ""
        };
        store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER refuse_check BEFORE INSERT ON events WHEN NEW.kind='{kind}' {condition} BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let engine = Engine::with_backend(
            config(),
            store.clone(),
            Arc::new(Script(Mutex::new(vec![("solver", GOOD)].into()))),
        );
        let outcome = engine.run(QUERY, "fast").await.unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.calls, 1);
        assert!(outcome.evidence.is_empty());
        assert!(!store.task(&outcome.task_id).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "verification"));
    }
}

#[tokio::test]
async fn prose_episode_assignments_are_not_reusable_memory() {
    let store = Arc::new(Store::in_memory().unwrap());
    let first = Engine::with_backend(
        config(),
        store.clone(),
        Arc::new(Script(Mutex::new(vec![("solver", GOOD)].into()))),
    )
    .run(QUERY, "fast")
    .await
    .unwrap();
    assert_eq!(first.status, "completed");
    assert!(store
        .publish_memory(
            &first.task_id,
            "episode",
            "Synthetic operator-reviewed assignment; requires rechecking",
        )
        .is_err());
    let engine = Engine::with_backend(
        config(),
        store.clone(),
        Arc::new(Script(Mutex::new(vec![("solver", GOOD)].into()))),
    );
    let fresh = engine.run(QUERY, "fast").await.unwrap();
    assert_eq!(fresh.status, "completed");
    assert_eq!(fresh.calls, 1);
    assert_eq!(fresh.answer, GOOD);
    let trace = store.task(&fresh.task_id).unwrap();
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "verification" && e["payload"]["origin"] == "published_memory"));
}
