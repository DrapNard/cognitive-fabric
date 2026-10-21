use super::*;
use std::{collections::VecDeque, future::Future, sync::Mutex};

const QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1,2,3]},{"id":"b","domain":[0,1,2,3]},{"id":"c","domain":[0,1,2,3]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"b","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"c","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const FIRST: &str = r#"{"a":1,"b":0,"c":1}"#;
const SECOND: &str = r#"{"a":1,"b":2,"c":1}"#;
const THIRD: &str = r#"{"a":1,"b":3,"c":1}"#;
const GOOD: &str = r#"{"a":1,"b":1,"c":1}"#;
struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
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
            })
        })
    }
}
fn config(enabled: bool) -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_tokens: 4096,
        max_calls: 12,
        max_tool_calls: 8,
        progress_stopping: enabled.then_some(progress::Settings { plateau_cycles: 2 }),
        ..Config::default()
    }
}
fn script(answers: &[&'static str]) -> Arc<Script> {
    let mut calls = VecDeque::from([
        ("planner", "invalid graph"),
        ("explorer", "Consider constraints"),
    ]);
    calls.extend(answers.iter().map(|answer| ("solver", *answer)));
    Arc::new(Script(Mutex::new(calls)))
}
fn assessments(trace: &serde_json::Value) -> Vec<serde_json::Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "reasoning_progress")
        .map(|e| e["payload"].clone())
        .collect()
}
#[test]
fn progress_configuration_is_opt_in_strict_and_bounded() {
    let cfg = Config::default();
    let old = serde_json::to_value(&cfg).unwrap();
    assert!(old.get("progress_stopping").is_none());
    assert!(serde_json::from_value::<Config>(old)
        .unwrap()
        .progress_stopping
        .is_none());
    for value in [0, 1, 5, u32::MAX] {
        let mut cfg = config(true);
        cfg.progress_stopping.as_mut().unwrap().plateau_cycles = value;
        assert!(cfg.validate().is_err());
    }
    for value in 2..=4 {
        let mut cfg = config(true);
        cfg.progress_stopping.as_mut().unwrap().plateau_cycles = value;
        cfg.validate().unwrap();
        let restored: Config = serde_json::from_value(serde_json::to_value(&cfg).unwrap()).unwrap();
        assert_eq!(restored.progress_stopping.unwrap().plateau_cycles, value);
    }
    assert!(serde_json::from_value::<Config>(
        json!({"progress_stopping":{"plateau_cycles":2,"permissions":true}})
    )
    .is_err());
}
#[tokio::test]
async fn distinct_equal_scores_stop_before_another_solver_only_when_enabled() {
    for enabled in [false, true] {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = script(&[FIRST, SECOND, THIRD, GOOD]);
        let outcome =
            crate::engine::Engine::with_backend(config(enabled), store.clone(), backend.clone())
                .run(QUERY, "research")
                .await
                .unwrap();
        assert_eq!(
            outcome.status,
            if enabled { "rejected" } else { "completed" }
        );
        assert_eq!(outcome.calls, if enabled { 5 } else { 6 });
        assert_eq!(outcome.cycles, if enabled { 3 } else { 4 });
        assert_eq!(
            outcome.stop_reason,
            if enabled { "no_progress" } else { "verified" }
        );
        assert_eq!(backend.0.lock().unwrap().len(), usize::from(enabled));
        assert!(store.inference_audit(&outcome.task_id).is_ok());
        let trace = store.task(&outcome.task_id).unwrap();
        let observations = assessments(&trace);
        assert_eq!(observations.len(), if enabled { 3 } else { 0 });
        if enabled {
            for (i, observation) in observations.iter().enumerate() {
                assert_eq!(observation["score"], 1);
                assert_eq!(observation["non_improving_cycles"], i);
                assert_eq!(observation["stop"], i == 2);
                assert_eq!(observation["verified"], false);
                assert_eq!(observation["permissions_granted"], false);
            }
            assert_eq!(outcome.resources.tool_calls, 4); // Retrieval and three full checks.
        } else {
            assert!(trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "reasoning_boundary")
                .all(|e| e["payload"].get("progress_state").is_none()));
        }
    }
}
#[tokio::test]
async fn improvement_resets_plateau_and_valid_witness_wins_before_stopping() {
    for answers in [
        vec![r#"{"a":0,"b":0,"c":1}"#, FIRST, SECOND, THIRD],
        vec![FIRST, SECOND, GOOD],
        vec![FIRST, "invalid assignment", SECOND, GOOD],
    ] {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = script(&answers);
        let outcome =
            crate::engine::Engine::with_backend(config(true), store.clone(), backend.clone())
                .run(QUERY, "research")
                .await
                .unwrap();
        assert!(backend.0.lock().unwrap().is_empty());
        let observations = assessments(&store.task(&outcome.task_id).unwrap());
        if answers[0].contains("\"a\":0") {
            assert_eq!(outcome.stop_reason, "no_progress");
            assert_eq!(
                observations
                    .iter()
                    .map(|v| v["score"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                vec![2, 1, 1, 1]
            );
            assert_eq!(observations[1]["non_improving_cycles"], 0);
        } else {
            assert_eq!(outcome.status, "completed");
            assert_eq!(outcome.answer, GOOD);
            assert!(observations.iter().all(|v| v["stop"] == false));
            if answers.len() == 4 {
                assert!(observations[1]["score"].is_null());
                assert_eq!(observations[2]["non_improving_cycles"], 1);
            }
        }
    }
}
#[tokio::test]
async fn progress_audit_failure_prevents_further_inference() {
    let store = Arc::new(Store::in_memory().unwrap());
    store.lock().unwrap().execute_batch("CREATE TRIGGER refuse_progress BEFORE INSERT ON events WHEN NEW.kind='reasoning_progress' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    let backend = script(&[FIRST]);
    let outcome = crate::engine::Engine::with_backend(config(true), store.clone(), backend.clone())
        .run(QUERY, "research")
        .await
        .unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.calls, 3);
    assert_eq!(outcome.resources.tool_calls, 2);
    assert!(outcome.evidence.is_empty());
    assert!(backend.0.lock().unwrap().is_empty());
    assert!(assessments(&store.task(&outcome.task_id).unwrap()).is_empty());
}
