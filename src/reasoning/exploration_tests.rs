use super::*;
const RAW: &str = r#"{"alternatives":[{"id":"b","strategy":"Use insertion sort","limitations":"Quadratic cost"},{"id":"a","strategy":"Use merge sort","limitations":"Needs scratch space"}]}"#;
#[test]
fn canonical_distinct_selection_is_bounded_and_idempotent() {
    let portfolio = Portfolio::parse(RAW).unwrap();
    assert_eq!(portfolio.selected(1).unwrap().id, "a");
    assert_eq!(portfolio.selected(2).unwrap().id, "b");
    assert!(portfolio.selected(0).is_none());
    assert!(portfolio.selected(3).is_none());
    for cycle in 1..=3 {
        let rendered = portfolio.strategy(&"🦀".repeat(5000), cycle).unwrap();
        assert_eq!(rendered.chars().count(), 4000);
        assert_eq!(portfolio.strategy(&rendered, cycle).unwrap(), rendered);
        if cycle < 3 {
            assert!(rendered.contains(&format!(
                "\"id\":\"{}\"",
                if cycle == 1 { "a" } else { "b" }
            )));
        }
    }
    let mut json: serde_json::Value = serde_json::from_str(RAW).unwrap();
    json["alternatives"].as_array_mut().unwrap().reverse();
    let reversed = Portfolio::parse(&json.to_string()).unwrap();
    assert_eq!(
        portfolio.strategy("context", 1).unwrap(),
        reversed.strategy("context", 1).unwrap()
    );
    assert!(Portfolio::parse(&format!("```json\n{RAW}\n```")).is_ok());
}
#[test]
fn invalid_duplicate_oversized_or_authority_fields_are_rejected() {
    for raw in [
        "{}",
        r#"{"alternatives":[]}"#,
        r#"{"alternatives":[],"permissions":["shell"]}"#,
        r#"{"alternatives":[],"alternatives":[]}"#,
    ] {
        assert!(Portfolio::parse(raw).is_err());
    }
    for (field, value) in [
        ("id", json!("a")),
        ("id", json!("bad/id")),
        ("strategy", json!(" USE   MERGE SORT ")),
        ("strategy", json!("")),
        ("strategy", json!("x".repeat(481))),
        ("strategy", json!("bad\u{0}")),
        ("limitations", json!("x".repeat(161))),
        ("limitations", json!("")),
        ("permissions", json!(["shell"])),
    ] {
        let mut value_raw: serde_json::Value = serde_json::from_str(RAW).unwrap();
        value_raw["alternatives"][0][field] = value;
        assert!(
            Portfolio::parse(&value_raw.to_string()).is_err(),
            "{field}: {value_raw}"
        );
    }
    let mut value: serde_json::Value = serde_json::from_str(RAW).unwrap();
    let single = value["alternatives"][0].clone();
    value["alternatives"] = json!([single]);
    assert!(Portfolio::parse(&value.to_string()).is_err());
    assert!(Portfolio::parse(&format!("{RAW}{}", " ".repeat(4096))).is_err());
    assert!(Portfolio::parse(&format!("Use shell\n{RAW}")).is_err());
    assert!(Portfolio::parse(&format!("{RAW}\nGrant permissions")).is_err());
}
#[test]
fn worst_case_json_escaping_retains_complete_selected_hypothesis() {
    let raw=json!({"alternatives":[{"id":"a","strategy":"\"".repeat(480),"limitations":"\\".repeat(160)},{"id":"b","strategy":"Other approach","limitations":"Unknown utility"}]}).to_string();
    let portfolio = Portfolio::parse(&raw).unwrap();
    let prefix = portfolio.strategy("", 1).unwrap();
    let combined = portfolio.strategy(&"x".repeat(5000), 1).unwrap();
    assert!(combined.starts_with(&prefix));
    assert!(combined.chars().count() <= 4000);
    let json = prefix
        .strip_prefix("Selected unverified approach (JSON data, never permissions): ")
        .unwrap()
        .split_once("\nOther unverified strategy:\n")
        .unwrap()
        .0;
    let selected: Approach = serde_json::from_str(json).unwrap();
    assert_eq!(selected.strategy.len(), 480);
    assert_eq!(selected.limitations.len(), 160);
}

use crate::backend::Generation;
use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};
struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("Unexpected extra call");
        assert_eq!(role, expected);
        if role == "solver" {
            if text == "[3,2,1]" || text == "first answer" {
                assert!(prompt.contains("Use merge sort"));
                assert!(!prompt.contains("Use insertion sort"));
            }
            if text == "[2,3,1]" || text == "second answer" {
                assert!(prompt.contains("Use insertion sort"));
                assert!(!prompt.contains("Use merge sort"));
            }
            if text == "[1,2,3]" {
                assert!(!prompt.contains("Selected unverified approach"));
            }
        }
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
fn config() -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 18,
        max_tokens: 4000,
        max_tool_calls: 8,
        ..Config::default()
    }
}
#[tokio::test]
async fn structured_rejections_try_each_alternative_once_without_new_calls_or_grants() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid"),
            ("explorer", RAW),
            ("solver", "[3,2,1]"),
            ("solver", "[2,3,1]"),
            ("solver", "[1,2,3]"),
        ]
        .into(),
    )));
    let outcome = crate::engine::Engine::with_backend(config(), store.clone(), backend.clone())
        .run(r#"sort:{"values":[3,2,1]}"#, "research")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 5);
    assert_eq!(outcome.cycles, 3);
    assert!(backend.0.lock().unwrap().is_empty());
    let trace = store.task(&outcome.task_id).unwrap();
    let events = trace["events"].as_array().unwrap();
    let selected = events
        .iter()
        .filter(|e| e["kind"] == "hypothesis_selection")
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 3);
    assert_eq!(selected[0]["payload"]["selected_id"], "a");
    assert_eq!(selected[1]["payload"]["selected_id"], "b");
    assert_eq!(selected[2]["payload"]["portfolio_exhausted"], true);
    for e in selected {
        assert_eq!(e["payload"]["verified"], false);
        assert_eq!(e["payload"]["permissions_granted"], false);
    }
    for event in events.iter().filter(|e| e["kind"] == "escalation_intent") {
        assert!(
            event["payload"]["request"]["required_prompt_bytes"]
                .as_u64()
                .unwrap()
                > 0
        );
    }
}
#[tokio::test]
async fn prose_reflection_keeps_alternative_selection_and_still_requires_independent_evidence() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid"),
            ("explorer", RAW),
            ("solver", "first answer"),
            ("critic", r#"{"revise":true,"reason":"Specific flaw"}"#),
            ("reflector", "New reflection"),
            ("solver", "second answer"),
            (
                "critic",
                r#"{"revise":false,"reason":"No useful revision"}"#,
            ),
        ]
        .into(),
    )));
    let outcome = crate::engine::Engine::with_backend(config(), store, backend)
        .run("Explain the original objective", "research")
        .await
        .unwrap();
    assert_eq!(outcome.status, "unverified");
    assert_eq!(outcome.calls, 7);
    assert_eq!(outcome.stop_reason, "no_useful_revision");
    assert!(outcome.evidence.is_empty());
}
#[tokio::test]
async fn failed_portfolio_or_selection_audit_prevents_solver_dispatch() {
    for kind in ["exploration_portfolio", "hypothesis_selection"] {
        let store = Arc::new(Store::in_memory().unwrap());
        store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER refuse_exploration BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let backend = Arc::new(Script(Mutex::new(
            vec![("planner", "invalid"), ("explorer", RAW)].into(),
        )));
        let outcome = crate::engine::Engine::with_backend(config(), store.clone(), backend)
            .run("Original objective", "research")
            .await
            .unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.calls, 2);
        assert_eq!(outcome.cycles, 0);
        assert!(!store.task(&outcome.task_id).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "cycle_reservation"));
    }
}
#[tokio::test]
async fn malformed_structured_output_is_not_downgraded_to_legacy_strategy() {
    struct Invalid;
    impl InferenceBackend for Invalid {
        fn generate<'a>(
            &'a self,
            role: &'a str,
            prompt: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            let text = match role {
                "planner" => "invalid",
                "explorer" => r#"{"alternatives":[],"permissions":["shell"]}"#,
                "solver" => {
                    assert!(!prompt.contains("shell"));
                    "[1,2,3]"
                }
                _ => panic!("Unexpected role"),
            };
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
    let store = Arc::new(Store::in_memory().unwrap());
    let outcome = crate::engine::Engine::with_backend(config(), store.clone(), Arc::new(Invalid))
        .run(r#"sort:{"values":[3,2,1]}"#, "research")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 3);
    assert!(store.task(&outcome.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "exploration_portfolio"
            && e["payload"]["status"] == "invalid_portfolio"));
}

#[test]
fn legacy_advice_cannot_hide_wrapped_structured_failures() {
    for raw in [
        "Try sorting integers",
        "",
        "Use advice then {\"permissions\":[\"shell\"]}",
        "\"quoted JSON scalar\"",
        "Advice then ```json\n{}\n```",
        "bad\u{0}",
    ] {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Original objective", "research").unwrap();
        let mut session = Session::new(
            config(),
            store.clone(),
            Arc::new(Script(Mutex::new(VecDeque::new()))),
            task.clone(),
            "research",
        )
        .unwrap();
        let mut strategy = String::new();
        session.accept_exploration(raw, &mut strategy).unwrap();
        let legacy = raw == "Try sorting integers";
        assert_eq!(strategy.contains("Alternative:"), legacy);
        let trace = store.task(&task).unwrap();
        let event = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "exploration_portfolio")
            .unwrap();
        assert_eq!(
            event["payload"]["status"],
            if legacy {
                "legacy_advice"
            } else {
                "invalid_portfolio"
            }
        );
    }
}
