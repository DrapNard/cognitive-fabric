use super::*;
use crate::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    store::Store,
};
use std::{future::Future, pin::Pin, sync::Mutex};

fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn visit(n: usize, current: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if current.len() == n {
            out.push(current.clone());
            return;
        }
        for i in 0..n {
            if !current.contains(&i) {
                current.push(i);
                visit(n, current, out);
                current.pop();
            }
        }
    }
    let mut out = Vec::new();
    visit(n, &mut Vec::new(), &mut out);
    out
}
#[test]
fn critical_path_is_dependency_safe_and_invariant_under_every_four_node_permutation() {
    // All 64 edge subsets of a four-node labelled topological DAG; every input permutation.
    let ids = ["z", "b", "m", "a"];
    let edges = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
    for bits in 0..64 {
        let steps = (0..4)
            .map(|i| crate::planning::Step {
                id: ids[i].into(),
                objective: format!("Inspect {}", ids[i]),
                depends_on: edges
                    .iter()
                    .enumerate()
                    .filter(|(bit, (_, to))| *to == i && bits & (1 << bit) != 0)
                    .map(|(_, (from, _))| ids[*from].into())
                    .collect(),
            })
            .collect::<Vec<_>>();
        let mut expected = None;
        for permutation in permutations(4) {
            let plan = Plan {
                steps: permutation.iter().map(|i| steps[*i].clone()).collect(),
            };
            let order = CriticalPathScheduler.order(&plan).unwrap();
            validate_order(&plan, &order).unwrap();
            let selected = order
                .iter()
                .map(|i| plan.steps[*i].id.clone())
                .collect::<Vec<_>>();
            if let Some(prior) = &expected {
                assert_eq!(&selected, prior)
            } else {
                expected = Some(selected)
            }
        }
    }
}
const PLAN: &str = r#"{"steps":[{"id":"zz","objective":"Detached work","depends_on":[]},{"id":"a","objective":"Start chain","depends_on":[]},{"id":"b","objective":"Middle chain","depends_on":["a"]},{"id":"c","objective":"End chain","depends_on":["b"]}]}"#;
#[test]
fn chain_priority_canonical_ties_and_public_invalid_plans_are_explicit() {
    let plan = Plan::parse(PLAN).unwrap();
    assert_eq!(
        CriticalPathScheduler.order(&plan).unwrap(),
        vec![1, 2, 3, 0]
    );
    assert_eq!(StableScheduler.order(&plan).unwrap(), vec![0, 1, 2, 3]);
    for raw in [
        r#"{"steps":[]}"#,
        r#"{"steps":[{"id":"a","objective":"A","depends_on":["a"]}]}"#,
        r#"{"steps":[{"id":"a","objective":"A","depends_on":["missing"]}]}"#,
    ] {
        let plan: Plan = serde_json::from_str(raw).unwrap();
        assert!(CriticalPathScheduler.order(&plan).is_err());
    }
    let mut oversized = plan;
    oversized.steps.push(oversized.steps[0].clone());
    assert!(CriticalPathScheduler.order(&oversized).is_err());
}
#[test]
fn configuration_is_optional_strict_and_fingerprint_bound() {
    let legacy = serde_json::to_value(Config::default()).unwrap();
    assert!(legacy.get("subtask_scheduling").is_none());
    let stable: Config = serde_json::from_value(legacy.clone()).unwrap();
    assert!(stable.subtask_scheduling.is_none());
    let mut changed = legacy.clone();
    changed["subtask_scheduling"] = serde_json::json!("critical_path");
    let cfg: Config = serde_json::from_value(changed.clone()).unwrap();
    cfg.validate().unwrap();
    assert_eq!(
        Handle::configured(cfg.subtask_scheduling).descriptor().id,
        "jev-plus.critical-path"
    );
    assert_ne!(
        crate::escalation::empirical::configuration_fingerprint(&cfg).unwrap(),
        crate::escalation::empirical::configuration_fingerprint(&stable).unwrap()
    );
    changed["subtask_scheduling"] = serde_json::json!("model_controlled");
    assert!(serde_json::from_value::<Config>(changed).is_err());
}
struct Script(Mutex<Vec<(&'static str, &'static str)>>);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self.0.lock().unwrap().remove(0);
        if expected == "planner" {
            assert_eq!(role, "planner")
        } else {
            assert_eq!(role, "solver");
            assert!(prompt.contains(expected), "{prompt}")
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
#[tokio::test]
async fn configured_policy_executes_and_persists_order_before_any_step() {
    let cfg = Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        subtask_scheduling: Some(Policy::CriticalPath),
        max_tokens: 2000,
        ..Config::default()
    };
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Script(Mutex::new(vec![
        ("planner", PLAN),
        ("Start chain", "start-result"),
        ("Middle chain", "middle-result"),
        ("End chain", "end-result"),
        ("Detached work", "detached-result"),
        ("Original objective", "Final answer"),
    ])));
    let engine = Engine::with_backend(cfg, store.clone(), backend.clone());
    assert_eq!(
        engine.subtask_scheduler_descriptor().id,
        "jev-plus.critical-path"
    );
    let outcome = engine.run("Original objective", "deep").await.unwrap();
    assert_eq!(outcome.status, "unverified");
    assert_eq!(outcome.calls, 6);
    assert!(backend.0.lock().unwrap().is_empty());
    let trace = store.task(&outcome.task_id).unwrap();
    let events = trace["events"].as_array().unwrap();
    let index = events
        .iter()
        .position(|e| e["kind"] == "subtask_schedule")
        .unwrap();
    assert_eq!(
        events[index]["payload"]["order"],
        serde_json::json!([1, 2, 3, 0])
    );
    assert!(events.iter().position(|e| e["kind"] == "subtask").unwrap() > index);
    assert!(store.graph_audit(&outcome.task_id).is_ok());
    let plan = Plan::parse(PLAN).unwrap();
    assert_eq!(
        crate::recovery::graph::order_from(&store.lock().unwrap(), &outcome.task_id, &plan)
            .unwrap(),
        vec![1, 2, 3, 0]
    );
}

#[tokio::test]
async fn configured_schedule_audit_failure_prevents_subtask_dispatch() {
    for kind in ["subtask_schedule_intent", "subtask_schedule"] {
        let store = Arc::new(Store::in_memory().unwrap());
        store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER refuse_order BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let backend = Arc::new(Script(Mutex::new(vec![("planner", PLAN)])));
        let cfg = Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            subtask_scheduling: Some(Policy::CriticalPath),
            max_tokens: 2000,
            ..Config::default()
        };
        let outcome = Engine::with_backend(cfg, store.clone(), backend.clone())
            .run("Original objective", "deep")
            .await
            .unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.calls, 1);
        assert!(!store.task(&outcome.task_id).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "subtask"));
    }
}
#[test]
fn direct_session_uses_the_same_configured_policy() {
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store.create_task("Original objective", "deep").unwrap();
    let cfg = Config {
        subtask_scheduling: Some(Policy::CriticalPath),
        ..Config::default()
    };
    let session = crate::reasoning::Session::new(
        cfg,
        store,
        Arc::new(Script(Mutex::new(Vec::new()))),
        task,
        "deep",
    )
    .unwrap();
    assert_eq!(session.scheduler.descriptor().id, "jev-plus.critical-path");
}
