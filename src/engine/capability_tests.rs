use super::*;
use crate::{backend::Generation, registry};
use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};
struct Backend {
    store: Arc<Store>,
    replacement: registry::Manifest,
    script: Mutex<VecDeque<(&'static str, &'static str)>>,
}
impl InferenceBackend for Backend {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            let (expected, text) = self
                .script
                .lock()
                .unwrap()
                .pop_front()
                .expect("Unexpected inference");
            assert_eq!(role, expected);
            if role == "interface" {
                assert!(prompt.contains("domain.sort"));
                registry::register(&self.store, &self.replacement).unwrap();
                registry::activate(&self.store, &self.replacement.id, &self.replacement.version)
                    .unwrap();
            }
            if prompt.contains("Provide domain advice") {
                assert!(prompt.contains("original sentinel"));
                assert!(!prompt.contains("replacement sentinel"));
            }
            Ok(Generation {
                text: text.into(),
                model: "fixture/v1".into(),
                input_tokens: Some(10),
                output_tokens: Some(5),
                elapsed_ms: 1.0,
                queue_wait_ms: None,
            })
        })
    }
}
#[tokio::test]
async fn interpreted_alias_routes_its_pinned_provider_without_granting_capabilities() {
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest:registry::Manifest=serde_json::from_value(json!({"id":"reviewed.sort.s1","version":"1.0.0","kind":"workflow","description":"Reviewed advisory sorting workflow","tags":["unmatched"],"source":"fixture","license":"CC0","workflow":"original sentinel","capabilities":[{"id":"domain.sort","version":"2.1.0"},{"id":"shell.execute","version":"1.0.0"}]})).unwrap();
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let mut replacement = manifest.clone();
    replacement.version = "2.0.0".into();
    replacement.workflow = Some("replacement sentinel".into());
    replacement.capabilities = vec![registry::Capability {
        id: "domain.other".into(),
        version: "1.0.0".into(),
    }];
    let backend=Arc::new(Backend{store:store.clone(),replacement,script:Mutex::new(vec![
        ("interface",r#"{"objective":"Sort the original values","constraints":[],"required_capabilities":["domain.sort"],"clarification":null}"#),
        ("solver","Optional unverified advice"),("planner","Plan"),("solver","[1,2,3]")].into())});
    let outcome = Engine::with_backend(
        Config {
            backend: "openai-local".into(),
            interpret_goals: true,
            ..Config::default()
        },
        store.clone(),
        backend.clone(),
    )
    .run(r#"sort:{"values":[3,1,2]}"#, "deep")
    .await
    .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 4);
    assert_eq!(outcome.skills, vec![manifest.id]);
    assert!(backend.script.lock().unwrap().is_empty());
    let trace = store.task(&outcome.task_id).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert!(!events.iter().any(|e| e["kind"] == "capability_gap"));
    let binding = &events
        .iter()
        .find(|e| e["kind"] == "route_refined")
        .unwrap()["payload"]["capability_bindings"][0];
    assert_eq!(binding["requested"], "domain.sort");
    assert_eq!(binding["provider"], "reviewed.sort.s1");
    assert_eq!(binding["provider_version"], "1.0.0");
    assert_eq!(binding["capability_version"], "2.1.0");
    assert_eq!(binding["selected"], true);
    assert_eq!(binding["permissions_granted"], false);
    assert!(events.iter().any(|e| e["kind"] == "expert_dispatch"
        && e["payload"]["version"] == "1.0.0"
        && e["payload"]["admitted"] == true));
    assert!(crate::security::authorize("jev-plus", "shell.execute").is_err());
    assert!(!events
        .iter()
        .any(|e| e["kind"] == "tool_dispatch" && e["payload"]["capability"] == "shell.execute"));
    assert_eq!(
        registry::lookup(&store, "domain.sort", "*").unwrap()["matches"],
        json!([])
    );
    assert_eq!(
        registry::lookup(&store, "domain.other", "*").unwrap()["matches"][0]["expert_version"],
        "2.0.0"
    );
}

#[tokio::test]
async fn large_capability_metadata_skips_optional_interpretation_without_losing_solver() {
    struct Solver;
    impl InferenceBackend for Solver {
        fn generate<'a>(
            &'a self,
            role: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async move {
                assert!(matches!(role, "planner" | "solver"));
                Ok(Generation {
                    text: if role == "planner" { "Plan" } else { "[1,2,3]" }.into(),
                    model: "fixture/v1".into(),
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    elapsed_ms: 1.0,
                    queue_wait_ms: None,
                })
            })
        }
    }
    let store = Arc::new(Store::in_memory().unwrap());
    for index in 0..350 {
        store
            .ensure_skill(
                &format!("cap.{index:03}.{}", "x".repeat(90)),
                "Declared metadata",
                &["unused".into()],
            )
            .unwrap();
    }
    let outcome = Engine::with_backend(
        Config {
            backend: "openai-local".into(),
            interpret_goals: true,
            ..Config::default()
        },
        store.clone(),
        Arc::new(Solver),
    )
    .run(r#"sort:{"values":[3,1,2]}"#, "deep")
    .await
    .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 2);
    assert!(store.task(&outcome.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "schedule"
            && e["payload"]["decision"] == "skip_interpretation"
            && e["payload"]["required_prompt_bytes"].as_u64().unwrap() > 32000));
}

#[tokio::test]
async fn capability_binding_audit_distinguishes_available_from_route_selected() {
    struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
    impl InferenceBackend for Script {
        fn generate<'a>(
            &'a self,
            role: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async move {
                let (expected, text) = self
                    .0
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("Unexpected inference");
                assert_eq!(role, expected);
                Ok(Generation {
                    text: text.into(),
                    model: "fixture/v1".into(),
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    elapsed_ms: 1.0,
                    queue_wait_ms: None,
                })
            })
        }
    }
    let store = Arc::new(Store::in_memory().unwrap());
    for index in 0..5 {
        let manifest:registry::Manifest=serde_json::from_value(json!({"id":format!("reviewed.cap{index}"),"version":"1.0.0","kind":"reference","description":"Reviewed declaration","tags":["unused"],"source":"fixture","license":"CC0","capabilities":[{"id":format!("domain.cap{index}"),"version":"2.0.0"}]})).unwrap();
        registry::register(&store, &manifest).unwrap();
        registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    }
    let backend=Arc::new(Script(Mutex::new(vec![("interface",r#"{"objective":"Sort original values","constraints":[],"required_capabilities":["domain.cap0","domain.cap1","domain.cap2","domain.cap3","domain.cap4","domain.missing"],"clarification":null}"#),("planner","Plan"),("solver","[1,2,3]")].into())));
    let outcome = Engine::with_backend(
        Config {
            backend: "openai-local".into(),
            interpret_goals: true,
            ..Config::default()
        },
        store.clone(),
        backend.clone(),
    )
    .run(r#"sort:{"values":[3,1,2]}"#, "deep")
    .await
    .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.skills.len(), 4);
    assert_eq!(outcome.calls, 3);
    let trace = store.task(&outcome.task_id).unwrap();
    let event = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "route_refined")
        .unwrap();
    let bindings = event["payload"]["capability_bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 6);
    for (index, binding) in bindings.iter().take(5).enumerate() {
        assert_eq!(binding["provider"], format!("reviewed.cap{index}"));
        assert_eq!(binding["provider_version"], "1.0.0");
        assert_eq!(binding["capability_version"], "2.0.0");
        assert_eq!(binding["selected"], index < 4);
        assert_eq!(binding["declaration_only"], true);
        assert_eq!(binding["permissions_granted"], false);
    }
    let missing = &bindings[5];
    assert_eq!(missing["requested"], "domain.missing");
    assert!(missing["provider"].is_null());
    assert!(missing["provider_version"].is_null());
    assert!(missing["capability_version"].is_null());
    assert_eq!(missing["selected"], false);
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "capability_gap"
            && e["payload"]["missing"] == json!(["domain.missing"])));
    assert!(backend.0.lock().unwrap().is_empty());
}
