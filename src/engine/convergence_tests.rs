use super::*;
use crate::backend::Generation;
use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};
const QUERY: &str = r#"sort:{"values":[3,2,1]}"#;
const A: &str = "[3,2,1]";
const B: &str = "[2,3,1]";
const GOOD: &str = "[1,2,3]";
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
            .expect("Unapproved extra inference");
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
fn cfg() -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 12,
        max_cycles: 4,
        max_tokens: 3000,
        max_tool_calls: 8,
        ..Config::default()
    }
}
#[tokio::test]
async fn nonadjacent_rejection_stops_before_more_work_but_successful_check_still_wins() {
    for success in [false, true] {
        let store = Arc::new(Store::in_memory().unwrap());
        let mut answers = vec![
            ("planner", "invalid graph"),
            ("explorer", "Alternative strategy"),
            ("solver", A),
            ("solver", B),
            ("solver", if success { GOOD } else { A }),
        ];
        if !success {
            answers.push(("solver", GOOD));
        }
        let backend = Arc::new(Script(Mutex::new(answers.into())));
        let outcome = Engine::with_backend(cfg(), store.clone(), backend.clone())
            .run(QUERY, "research")
            .await
            .unwrap();
        assert_eq!(
            outcome.status,
            if success { "completed" } else { "rejected" }
        );
        assert_eq!(
            outcome.stop_reason,
            if success { "verified" } else { "no_progress" }
        );
        assert_eq!(outcome.calls, 5);
        assert_eq!(outcome.cycles, 3);
        assert_eq!(outcome.resources.tool_calls, 4);
        assert_eq!(backend.0.lock().unwrap().len(), usize::from(!success));
        let trace = store.task(&outcome.task_id).unwrap();
        let events = trace["events"].as_array().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e["kind"] == "verification")
                .count(),
            3
        );
        let progress = events
            .iter()
            .filter(|e| e["kind"] == "candidate_progress")
            .collect::<Vec<_>>();
        assert_eq!(progress.len(), if success { 2 } else { 3 });
        if !success {
            assert_eq!(progress[2]["payload"]["previous_cycles"], json!([1]));
            assert_eq!(progress[2]["payload"]["repeated"], true);
            assert!(outcome.evidence.is_empty());
        }
    }
}
#[tokio::test]
async fn recursive_ablation_ignores_solver_labelled_critique_and_reflection_content() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Script(Mutex::new(
        vec![
            ("solver", "A"),
            ("solver", r#"{"revise":true,"reason":"Try another answer"}"#),
            ("solver", "B"),
            ("solver", "B"),
            ("solver", r#"{"revise":true,"reason":"Revise again"}"#),
            ("solver", "A"),
            ("solver", "A"),
            ("solver", "Must remain unused"),
        ]
        .into(),
    )));
    let outcome = Engine::with_backend(cfg(), store, backend.clone())
        .run_profile(
            "Explain the original objective",
            "research",
            ExecutionProfile::Recursive,
        )
        .await
        .unwrap();
    assert_eq!(outcome.status, "unverified");
    assert_eq!(outcome.stop_reason, "no_progress");
    assert_eq!(outcome.calls, 7);
    assert_eq!(outcome.cycles, 3);
    assert_eq!(outcome.answer, "A");
    assert_eq!(backend.0.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn failed_progress_audit_never_admits_further_revision_calls() {
    let store = Arc::new(Store::in_memory().unwrap());
    store.lock().unwrap().execute_batch("CREATE TRIGGER deny_progress BEFORE INSERT ON events WHEN NEW.kind='candidate_progress' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    let backend = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid graph"),
            ("solver", A),
            ("solver", GOOD),
        ]
        .into(),
    )));
    let outcome = Engine::with_backend(cfg(), store.clone(), backend.clone())
        .run(QUERY, "deep")
        .await
        .unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.calls, 2);
    assert_eq!(outcome.cycles, 1);
    assert_eq!(backend.0.lock().unwrap().len(), 1);
    assert!(outcome.evidence.is_empty());
    assert!(!store.task(&outcome.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "escalation_intent"));
}
