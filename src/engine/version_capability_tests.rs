use super::*;
use crate::{backend::Generation, registry};
use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};
const QUERY: &str = r#"sort:{"values":[2,1]}"#;
struct Script {
    answers: Mutex<VecDeque<(&'static str, String)>>,
    store: Arc<Store>,
    replacement: Option<registry::Manifest>,
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
            .expect("Unexpected inference");
        assert_eq!(role, expected);
        assert!(!prompt.contains("old_workflow_sentinel"));
        assert!(!prompt.contains("late_workflow_sentinel"));
        if prompt.contains("Provide domain advice") {
            assert!(prompt.contains("new_workflow_sentinel"));
        }
        if role == "interface" {
            if let Some(replacement) = &self.replacement {
                registry::register(&self.store, replacement).unwrap();
                registry::activate(&self.store, &replacement.id, &replacement.version).unwrap();
            }
        }
        Box::pin(async move {
            Ok(Generation {
                text,
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
fn manifest(
    id: &str,
    capability_version: &str,
    workflow: &str,
    tags: &[&str],
) -> registry::Manifest {
    serde_json::from_value(json!({"id":id,"version":"1.0.0","kind":"workflow","description":"Synthetic versioned workflow","tags":tags,"capabilities":[{"id":"domain.sort","version":capability_version}],"source":"Synthetic fixture","license":"CC0-1.0","workflow":workflow})).unwrap()
}
#[tokio::test]
async fn interpreted_version_selects_only_compatible_pinned_workflow_and_preserves_original_verification(
) {
    for replacement in [false, true] {
        let store = Arc::new(Store::in_memory().unwrap());
        let old = manifest("expert.old", "1.9.0", "old_workflow_sentinel", &["sort"]);
        let new = manifest("expert.new", "2.1.0", "new_workflow_sentinel", &["fixture"]);
        for m in [&old, &new] {
            registry::register(&store, m).unwrap();
            registry::activate(&store, &m.id, &m.version).unwrap();
        }
        let mut late = new.clone();
        late.version = "2.0.0".into();
        late.capabilities[0].version = "3.0.0".into();
        late.workflow = Some("late_workflow_sentinel".into());
        let intent=json!({"objective":QUERY,"constraints":[],"required_capabilities":["domain.sort"],"capability_versions":{"domain.sort":"^2"},"clarification":null}).to_string();
        let backend = Arc::new(Script {
            answers: Mutex::new(
                vec![
                    ("interface", intent),
                    ("solver", "Optional advice".into()),
                    ("planner", "invalid graph".into()),
                    ("solver", "[1,2]".into()),
                ]
                .into(),
            ),
            store: store.clone(),
            replacement: replacement.then_some(late),
        });
        let cfg = Config {
            backend: "openai-local".into(),
            interpret_goals: true,
            ..Config::default()
        };
        let outcome = Engine::with_backend(cfg, store.clone(), backend.clone())
            .run(QUERY, "deep")
            .await
            .unwrap();
        assert_eq!(outcome.status, "completed");
        assert_eq!(outcome.answer, "[1,2]");
        assert_eq!(outcome.skills, vec!["expert.new"]);
        assert_eq!(outcome.calls, 4);
        assert!(backend.answers.lock().unwrap().is_empty());
        let trace = store.task(&outcome.task_id).unwrap();
        let events = trace["events"].as_array().unwrap();
        let resolution = &events
            .iter()
            .find(|e| e["kind"] == "capability_resolution")
            .unwrap()["payload"]["resolution"];
        assert_eq!(resolution["excluded_version_routes"], json!(["expert.old"]));
        assert_eq!(resolution["bindings"][0]["version_requirement"], "^2");
        assert_eq!(resolution["bindings"][0]["version_compatible"], true);
        assert_eq!(resolution["bindings"][0]["capability_version"], "2.1.0");
        assert!(events.iter().any(|e| e["kind"] == "expert_dispatch"
            && e["payload"]["id"] == "expert.new"
            && e["payload"]["version"] == "1.0.0"
            && e["payload"]["admitted"] == true));
    }
}
#[tokio::test]
async fn incompatible_versions_remain_visible_without_falling_back_to_old_expert() {
    let store = Arc::new(Store::in_memory().unwrap());
    let old = manifest("expert.old", "1.9.0", "old_workflow_sentinel", &["sort"]);
    registry::register(&store, &old).unwrap();
    registry::activate(&store, &old.id, &old.version).unwrap();
    let intent=json!({"objective":QUERY,"constraints":[],"required_capabilities":["domain.sort"],"capability_versions":{"domain.sort":"^9"},"clarification":null}).to_string();
    let backend = Arc::new(Script {
        answers: Mutex::new(
            vec![
                ("interface", intent),
                ("planner", "invalid graph".into()),
                ("solver", "[1,2]".into()),
            ]
            .into(),
        ),
        store: store.clone(),
        replacement: None,
    });
    let cfg = Config {
        backend: "openai-local".into(),
        interpret_goals: true,
        ..Config::default()
    };
    let outcome = Engine::with_backend(cfg, store.clone(), backend.clone())
        .run(QUERY, "deep")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert!(outcome.skills.is_empty());
    assert_eq!(outcome.calls, 3);
    assert!(backend.answers.lock().unwrap().is_empty());
    let trace = store.task(&outcome.task_id).unwrap();
    let resolution = &trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "capability_resolution")
        .unwrap()["payload"]["resolution"];
    assert_eq!(resolution["incompatible"], json!(["domain.sort"]));
    assert_eq!(resolution["missing"], json!([]));
    assert_eq!(resolution["bindings"][0]["version_compatible"], false);
    assert_eq!(resolution["bindings"][0]["selected"], false);
}
