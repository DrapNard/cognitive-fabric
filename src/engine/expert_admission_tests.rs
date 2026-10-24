//! Ranked expert admission must distinguish a budget refusal from an attempt.
use super::*;
use crate::{backend::Generation, registry, routing};
use std::{future::Future, pin::Pin, sync::Mutex};

struct Ranked;
impl routing::ExpertRouter for Ranked {
    fn descriptor(&self) -> routing::Descriptor {
        routing::Descriptor {
            id: "fixture.rank".into(),
            version: "1.0.0".into(),
        }
    }
    fn select(&self, _: routing::Request<'_>) -> Result<Vec<String>> {
        Ok(vec!["rust.first.s1".into(), "rust.second.s1".into()])
    }
}
struct AdviceBackend {
    prompts: Mutex<Vec<String>>,
    failure: u8,
}
impl InferenceBackend for AdviceBackend {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.prompts.lock().unwrap().push(prompt.into());
        Box::pin(async move {
            if prompt.contains("FIRST_ADVICE") {
                match self.failure {
                    1 => std::future::pending::<()>().await,
                    2 => bail!("Provider integrity failure"),
                    _ => {}
                }
            }
            Ok(Generation {
                text: if prompt.contains("SECOND_ADVICE") {
                    "SECOND_RESULT".into()
                } else if role == "critic" {
                    r#"{"revise":false,"reason":"No revision"}"#.into()
                } else {
                    "Unverified answer".into()
                },
                model: "fixture/v1".into(),
                input_tokens: Some(5),
                output_tokens: Some(5),
                elapsed_ms: 1.0,
                queue_wait_ms: None,
                provider_metrics: None,
            })
        })
    }
}
fn engine(expensive: bool, failure: u8) -> (Engine, Arc<Store>, Arc<AdviceBackend>) {
    let store = Arc::new(Store::in_memory().unwrap());
    for (id, marker) in [
        ("rust.first.s1", "FIRST_ADVICE"),
        ("rust.second.s1", "SECOND_ADVICE"),
    ] {
        let mut raw = json!({"id":id,"version":"1.0.0","kind":"workflow","description":"Admission fixture","tags":["ownership"],"source":"fixture","license":"CC0","workflow":marker});
        if id == "rust.first.s1" && expensive {
            raw.as_object_mut().unwrap().remove("workflow");
            raw["workflow_steps"] = json!((0..4)
                .map(|i| json!({"id":format!("step{i}"),"advice":marker,"depends_on":[]}))
                .collect::<Vec<_>>());
        }
        let manifest: registry::Manifest = serde_json::from_value(raw).unwrap();
        registry::register(&store, &manifest).unwrap();
        registry::activate(&store, id, "1.0.0").unwrap();
    }
    let backend = Arc::new(AdviceBackend {
        prompts: Mutex::new(Vec::new()),
        failure,
    });
    let cfg = Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 4,
        max_cycles: 1,
        max_wall_seconds: 30,
        ..Default::default()
    };
    let engine = Engine::with_backend(cfg, store.clone(), backend.clone())
        .with_expert_router(Arc::new(Ranked))
        .unwrap();
    (engine, store, backend)
}
fn dispatches(trace: &serde_json::Value) -> Vec<(String, bool)> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "expert_dispatch")
        .map(|e| {
            (
                e["payload"]["id"].as_str().unwrap().into(),
                e["payload"]["admitted"].as_bool().unwrap(),
            )
        })
        .collect()
}
#[tokio::test]
async fn unaffordable_first_workflow_does_not_hide_affordable_ranked_candidate() {
    let (engine, store, backend) = engine(true, 0);
    let result = engine.run("Explain Rust ownership", "deep").await.unwrap();
    assert_eq!(
        dispatches(&store.task(&result.task_id).unwrap()),
        vec![
            ("rust.first.s1".into(), false),
            ("rust.second.s1".into(), true)
        ]
    );
    let prompts = backend.prompts.lock().unwrap();
    assert!(!prompts.iter().any(|p| p.contains("FIRST_ADVICE")));
    assert_eq!(
        prompts
            .iter()
            .filter(|p| p.contains("SECOND_ADVICE"))
            .count(),
        1
    );
    assert!(prompts.iter().any(|p| p.contains("SECOND_RESULT")));
    assert_eq!(result.calls, 3);
    assert_eq!(result.resources.reserved_output_tokens, 832);
    assert_eq!(result.status, "unverified");
}
#[tokio::test(start_paused = true)]
async fn admitted_timeout_does_not_buy_another_expert_attempt() {
    let (engine, store, backend) = engine(false, 1);
    let result = engine.run("Explain Rust ownership", "deep").await.unwrap();
    let trace = store.task(&result.task_id).unwrap();
    assert_eq!(dispatches(&trace), vec![("rust.first.s1".into(), true)]);
    assert!(!backend
        .prompts
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("SECOND_ADVICE")));
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "expert_failure")
            .count(),
        1
    );
    assert_eq!(result.calls, 3);
    assert!(result.resources.output_tokens.is_none());
}
#[tokio::test]
async fn admitted_provider_failure_is_fatal_without_another_expert() {
    let (engine, store, backend) = engine(false, 2);
    assert_eq!(
        engine
            .run("Explain Rust ownership", "deep")
            .await
            .unwrap()
            .status,
        "failed"
    );
    let prompts = backend.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    let task: String = store
        .lock()
        .unwrap()
        .query_row("SELECT id FROM tasks", [], |r| r.get(0))
        .unwrap();
    let trace = store.task(&task).unwrap();
    assert_eq!(dispatches(&trace), vec![("rust.first.s1".into(), true)]);
    assert_eq!(trace["task"]["status"], "failed");
}
#[tokio::test]
async fn refusal_audit_failure_never_selects_another_candidate() {
    let (engine, store, backend) = engine(true, 0);
    store.lock().unwrap().execute_batch("CREATE TRIGGER reject_refusal BEFORE INSERT ON events WHEN NEW.kind='expert_dispatch' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    assert_eq!(
        engine
            .run("Explain Rust ownership", "deep")
            .await
            .unwrap()
            .status,
        "failed"
    );
    assert!(backend.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn prompt_budget_refusal_of_legacy_workflow_can_select_shorter_advice() {
    let (mut engine, store, backend) = engine(false, 0);
    engine.cfg.max_input_bytes = 6000;
    let manifest:registry::Manifest=serde_json::from_value(json!({"id":"rust.first.s1","version":"2.0.0","kind":"workflow","description":"Long advice fixture","tags":["ownership"],"source":"fixture","license":"CC0","workflow":format!("FIRST_ADVICE {}","x".repeat(1900))})).unwrap();
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, "rust.first.s1", "2.0.0").unwrap();
    let outcome = engine.run("Explain Rust ownership", "deep").await.unwrap();
    assert_eq!(
        dispatches(&store.task(&outcome.task_id).unwrap()),
        vec![
            ("rust.first.s1".into(), false),
            ("rust.second.s1".into(), true)
        ]
    );
    let prompts = backend.prompts.lock().unwrap();
    assert!(!prompts.iter().any(|p| p.contains("FIRST_ADVICE")));
    assert!(prompts.iter().any(|p| p.contains("SECOND_ADVICE")));
    assert_eq!(outcome.calls, 3);
    assert!(outcome.resources.prompt_bytes <= 6000);
}

#[tokio::test]
async fn refused_bound_model_has_no_backend_snapshot_or_phase_allocation() {
    let (mut engine, store, _) = engine(true, 0);
    // Refusal must precede even a managed backend's construction. No server or
    // worker is required for a candidate which cannot fit the call allowance.
    engine.cfg.inference_process = true;
    let selection = super::expert_tests::register_model(&store, "http://127.0.0.1:1/v1");
    let manifest:registry::Manifest=serde_json::from_value(json!({"id":"rust.first.s1","version":"2.0.0","kind":"workflow","description":"Bound model refused by call ceiling","tags":["ownership"],"source":"fixture","license":"CC0","model":selection,"workflow_steps":(0..4).map(|i|json!({"id":format!("step{i}"),"advice":"FIRST_ADVICE","depends_on":[]})).collect::<Vec<_>>()})).unwrap();
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, "rust.first.s1", "2.0.0").unwrap();
    let outcome = engine.run("Explain Rust ownership", "deep").await.unwrap();
    assert_eq!(outcome.status, "unverified");
    assert_eq!(outcome.calls, 3);
    let trace = store.task(&outcome.task_id).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert!(!events.iter().any(|e| e["kind"] == "expert_model_snapshot"));
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "expert_phase_budget")
            .count(),
        1
    );
    let metadata = crate::evaluation::model_metadata::read(&store, &outcome.task_id).unwrap();
    assert!(metadata.bound_expert_models.is_empty());
    assert_eq!(
        serde_json::to_value(&metadata.expert_advice).unwrap()["phases_started"],
        1
    );
    let admission = events
        .iter()
        .position(|e| e["kind"] == "expert_dispatch" && e["payload"]["admitted"] == true)
        .unwrap();
    let phase = events
        .iter()
        .position(|e| e["kind"] == "expert_phase_budget")
        .unwrap();
    let dispatch = events
        .iter()
        .position(|e| e["kind"] == "model_dispatch")
        .unwrap();
    assert!(admission < phase && phase < dispatch);
}

#[tokio::test]
async fn all_refused_candidates_allocate_no_advice_phase_or_model_calls() {
    let (mut engine, store, backend) = engine(true, 0);
    engine.cfg.max_calls = 2;
    let outcome = engine.run("Explain Rust ownership", "deep").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    assert_eq!(
        dispatches(&trace),
        vec![
            ("rust.first.s1".into(), false),
            ("rust.second.s1".into(), false)
        ]
    );
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "expert_phase_budget" || e["kind"] == "expert_model_snapshot"));
    assert!(!backend
        .prompts
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("FIRST_ADVICE") || p.contains("SECOND_ADVICE")));
    assert_eq!(outcome.calls, 2);
    assert_eq!(outcome.resources.reserved_output_tokens, 640);
    let metadata = crate::evaluation::model_metadata::read(&store, &outcome.task_id).unwrap();
    assert_eq!(
        serde_json::to_value(metadata.expert_advice).unwrap()["phases_started"],
        0
    );
}
