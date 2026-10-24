use super::*;
use crate::{backend::Generation, registry};
use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};
const QUERY: &str = r#"sort:{"values":[2,1]}"#;
struct Script {
    answers: Mutex<VecDeque<(&'static str, String)>>,
    disable: Option<Arc<Store>>,
}
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("Unapproved extra call");
        assert_eq!(role, expected);
        if role == "interface" {
            if let Some(store) = &self.disable {
                registry::disable(store, "z.joint").unwrap();
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
fn manifest(id: &str, aliases: &[String], workflow: bool) -> registry::Manifest {
    serde_json::from_value(json!({"id":id,"version":"1.0.0","kind":if workflow {"workflow"} else {"reference"},"description":"Synthetic coverage fixture","tags":["fixture"],"capabilities":aliases.iter().map(|id|json!({"id":id,"version":"1.0.0"})).collect::<Vec<_>>(),"source":"Synthetic test fixture","license":"CC0-1.0","workflow":if workflow {Some("Give one sorting constraint")} else {None}})).unwrap()
}
fn add(store: &Store, m: &registry::Manifest) {
    registry::register(store, m).unwrap();
    registry::activate(store, &m.id, &m.version).unwrap();
}
fn cfg() -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: true,
        max_calls: 6,
        ..Config::default()
    }
}
fn intent(aliases: &[String]) -> String {
    json!({"objective":QUERY,"constraints":[],"required_capabilities":aliases,"clarification":null})
        .to_string()
}
#[tokio::test]
async fn joint_selection_executes_one_workflow_and_pins_declarations_across_disable() {
    for disable in [false, true] {
        let store = Arc::new(Store::in_memory().unwrap());
        let aliases = (0..6).map(|i| format!("domain.cap{i}")).collect::<Vec<_>>();
        for (i, alias) in aliases.iter().enumerate() {
            add(
                &store,
                &manifest(&format!("a.single{i}"), std::slice::from_ref(alias), false),
            );
        }
        add(&store, &manifest("z.joint", &aliases, true));
        let script = Arc::new(Script {
            answers: Mutex::new(
                vec![
                    ("interface", intent(&aliases)),
                    ("solver", "Advice".into()),
                    ("planner", "invalid graph".into()),
                    ("solver", "[1,2]".into()),
                ]
                .into(),
            ),
            disable: disable.then(|| store.clone()),
        });
        let outcome = Engine::with_backend(cfg(), store.clone(), script.clone())
            .run(QUERY, "deep")
            .await
            .unwrap();
        assert_eq!(outcome.status, "completed");
        assert_eq!(outcome.skills, vec!["z.joint"]);
        assert_eq!(outcome.calls, 4);
        assert!(script.answers.lock().unwrap().is_empty());
        let trace = store.task(&outcome.task_id).unwrap();
        let resolution = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "capability_resolution")
            .unwrap();
        assert_eq!(resolution["payload"]["resolution"]["unselected"], json!([]));
        assert_eq!(
            resolution["payload"]["resolution"]["bindings"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
        if disable {
            let next = store.task_context(QUERY).unwrap();
            let resolved = crate::capabilities::resolve(
                &next.skills,
                &next.manifests,
                &next.expert_models,
                &aliases,
                &std::collections::BTreeMap::new(),
                &[],
                true,
            )
            .unwrap();
            assert_eq!(resolved.skills.len(), 4);
            assert_eq!(resolved.unselected.len(), 2);
        }
    }
}
#[tokio::test]
async fn failed_resolution_audit_stops_after_interpretation_before_expert_or_solver() {
    let store = Arc::new(Store::in_memory().unwrap());
    add(&store, &manifest("z.joint", &["domain.one".into()], true));
    store.lock().unwrap().execute_batch("CREATE TRIGGER deny_resolution BEFORE INSERT ON events WHEN NEW.kind='capability_resolution' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    let script = Arc::new(Script {
        answers: Mutex::new(vec![("interface", intent(&["domain.one".into()]))].into()),
        disable: None,
    });
    let outcome = Engine::with_backend(cfg(), store.clone(), script.clone())
        .run(QUERY, "deep")
        .await
        .unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.calls, 1);
    assert!(script.answers.lock().unwrap().is_empty());
    assert!(!store.task(&outcome.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "route_refined" || e["kind"] == "expert_phase_budget"));
}
#[tokio::test]
async fn unavailable_bound_provider_cannot_hide_an_available_alias_provider() {
    let store = Arc::new(Store::in_memory().unwrap());
    let file = std::env::temp_dir().join(format!("fabric-cap-model-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(&file,json!({"id":"fixture.model","version":"1.0.0","model":"fixture-model","api_base":"http://127.0.0.1:1234/v1","source":"Synthetic metadata fixture","license":"CC0-1.0","context_tokens":4096,"max_output_tokens":192,"weights_sha256":null}).to_string()).unwrap();
    let registered = store.register_model(&file).unwrap();
    std::fs::remove_file(file).unwrap();
    let selection = crate::models::Selection {
        id: registered.manifest.id,
        version: registered.manifest.version,
    };
    let mut bound = manifest("a.bound", &["domain.one".into()], true);
    bound.model = Some(selection.clone());
    add(&store, &bound);
    add(&store, &manifest("z.ready", &["domain.one".into()], true));
    store.set_model_enabled(&selection, false).unwrap();
    let script = Arc::new(Script {
        answers: Mutex::new(
            vec![
                ("interface", intent(&["domain.one".into()])),
                ("solver", "Advice".into()),
                ("planner", "invalid graph".into()),
                ("solver", "[1,2]".into()),
            ]
            .into(),
        ),
        disable: None,
    });
    let outcome = Engine::with_backend(cfg(), store.clone(), script.clone())
        .run(QUERY, "deep")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.skills, vec!["z.ready"]);
    assert_eq!(outcome.calls, 4);
    assert!(script.answers.lock().unwrap().is_empty());
    assert!(!store.task(&outcome.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "expert_model_snapshot" && e["payload"]["expert"] == "a.bound"));
}
