use super::*;
use crate::{backend::Generation, registry, store::Document};
use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};
const QUERY: &str = "Describe guarded borrowing";
struct Script {
    answers: Mutex<VecDeque<(&'static str, String)>>,
    mutation: Option<Arc<Store>>,
    expect_reference: bool,
}
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("Unapproved inference");
        assert_eq!(role, expected);
        assert!(!prompt.contains("post_snapshot_sentinel"));
        if role == "interface" {
            assert!(!prompt.contains("pinned_reference_sentinel"));
            if let Some(store) = &self.mutation {
                registry::disable(store, "reference.atomic").unwrap();
                store
                    .add_document(&doc("late", "post_snapshot_sentinel"))
                    .unwrap();
            }
        } else {
            assert_eq!(
                prompt.contains("pinned_reference_sentinel"),
                self.expect_reference
            );
        }
        Box::pin(async move {
            Ok(Generation {
                text,
                model: "fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}
fn doc(id: &str, sentinel: &str) -> Document {
    Document {
        id: id.into(),
        skill_id: "reference.atomic".into(),
        title: "Atomic references".into(),
        body: format!("atomic {sentinel}"),
        source: "Synthetic fixture".into(),
        license: "CC0-1.0".into(),
    }
}
fn fixture() -> Arc<Store> {
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest:registry::Manifest=serde_json::from_value(json!({"id":"reference.atomic","version":"1.0.0","kind":"reference","description":"Synthetic reference provider","tags":["fixture"],"capabilities":[{"id":"domain.atomic","version":"1.0.0"}],"source":"Synthetic fixture","license":"CC0-1.0"})).unwrap();
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    store
        .add_document(&doc("original", "pinned_reference_sentinel"))
        .unwrap();
    store
}
fn cfg() -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: true,
        max_calls: 3,
        ..Config::default()
    }
}
fn script(query: &str, answer: &str, expect: bool, mutation: Option<Arc<Store>>) -> Arc<Script> {
    Arc::new(Script {answers:Mutex::new(vec![("interface",json!({"objective":query,"constraints":[],"required_capabilities":["domain.atomic"],"clarification":null}).to_string()),("planner","invalid graph".into()),("solver",answer.into())].into()),mutation,expect_reference:expect})
}
#[tokio::test]
async fn capability_documents_use_the_original_snapshot_despite_live_disable_and_insert() {
    for profile in [ExecutionProfile::Full, ExecutionProfile::Experts] {
        let store = fixture();
        assert!(store.search(QUERY, None, 3).unwrap().is_empty());
        let backend = script(QUERY, "Final unverified answer", true, Some(store.clone()));
        let outcome = Engine::with_backend(cfg(), store.clone(), backend.clone())
            .run_profile(QUERY, "standard", profile)
            .await
            .unwrap();
        assert_eq!(outcome.status, "unverified");
        assert_eq!(outcome.calls, 3);
        assert_eq!(outcome.resources.tool_calls, 2);
        assert!(outcome.evidence.contains(&"document:original".into()));
        assert!(!outcome.evidence.contains(&"document:late".into()));
        assert!(backend.answers.lock().unwrap().is_empty());
        let trace = store.task(&outcome.task_id).unwrap();
        assert!(trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "capability_retrieval"
                && e["payload"]["admitted"] == true
                && e["payload"]["documents"][0]["id"] == "original"));
    }
}
#[tokio::test]
async fn preserves_the_verifier_tool_reserve_and_skips_oversized_snapshot() {
    for large in [false, true] {
        let store = fixture();
        if large {
            store.lock().unwrap().execute_batch("CREATE TABLE oversized_reference_fixture(body BLOB); INSERT INTO oversized_reference_fixture VALUES(zeroblob(64000000));").unwrap();
        }
        let query = r#"sort:{"values":[2,1]}"#;
        let mut cfg = cfg();
        cfg.max_tool_calls = 2;
        let backend = script(query, "[1,2]", false, None);
        let outcome = Engine::with_backend(cfg, store.clone(), backend.clone())
            .run(query, "standard")
            .await
            .unwrap();
        assert_eq!(outcome.status, "completed");
        assert_eq!(outcome.resources.tool_calls, 2);
        assert!(!outcome.evidence.contains(&"document:original".into()));
        assert!(backend.answers.lock().unwrap().is_empty());
        let trace = store.task(&outcome.task_id).unwrap();
        let event = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "capability_retrieval")
            .unwrap();
        assert_eq!(event["payload"]["admitted"], false);
        assert_eq!(
            event["payload"]["reason"],
            if large {
                "snapshot_unavailable"
            } else {
                "preserve_main_solver_and_verifier"
            }
        );
    }
}
#[tokio::test]
async fn retrieval_audit_failure_stops_before_solver_and_retains_search_charge() {
    for kind in ["capability_retrieval_intent", "capability_retrieval"] {
        let store = fixture();
        store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER deny_reference BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let backend = script(QUERY, "Must not be generated", true, None);
        let outcome = Engine::with_backend(cfg(), store.clone(), backend.clone())
            .run(QUERY, "standard")
            .await
            .unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.calls, 1);
        assert_eq!(outcome.resources.tool_calls, 2);
        assert!(!outcome.evidence.contains(&"document:original".into()));
        assert_eq!(backend.answers.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn corrupt_provider_document_fails_integrity_before_entering_solver_context() {
    let store = fixture();
    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE documents SET sha256=?1 WHERE id='original'",
            ["0".repeat(64)],
        )
        .unwrap();
    let backend = script(QUERY, "Must not be generated", true, None);
    let outcome = Engine::with_backend(cfg(), store.clone(), backend.clone())
        .run(QUERY, "standard")
        .await
        .unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.resources.tool_calls, 2);
    assert!(!outcome.evidence.contains(&"document:original".into()));
    assert_eq!(backend.answers.lock().unwrap().len(), 2);
}
