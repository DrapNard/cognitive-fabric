//! Registered model-backed workflow execution and lifecycle regressions.
use super::*;
use crate::{backend::Generation, models::Selection, registry};
use std::{future::Future, pin::Pin, sync::Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Main(Mutex<Vec<String>>);
impl InferenceBackend for Main {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.0.lock().unwrap().push(prompt.into());
        Box::pin(async move {
            Ok(Generation {
                text: if role == "critic" {
                    r#"{"revise":false,"reason":"No useful revision"}"#.into()
                } else {
                    "Unverified baseline answer".into()
                },
                model: "baseline-fixture".into(),
                input_tokens: Some(10),
                output_tokens: Some(10),
                elapsed_ms: 1.0,
                queue_wait_ms: None,
            })
        })
    }
}
fn register_model(store: &Store, endpoint: &str) -> Selection {
    let path =
        std::env::temp_dir().join(format!("fabric-expert-model-{}.json", uuid::Uuid::new_v4()));
    let manifest = json!({"id":"domain.local","version":"1.0.0","model":"domain-model","api_base":endpoint,"source":"fixture","license":"CC0","context_tokens":4096,"max_output_tokens":512,"weights_sha256":null});
    std::fs::write(&path, manifest.to_string()).unwrap();
    store.register_model(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    Selection {
        id: "domain.local".into(),
        version: "1.0.0".into(),
    }
}
fn workflow(selection: &Selection) -> registry::Manifest {
    serde_json::from_value(json!({"id":"rust.model.s1","version":"1.0.0","kind":"workflow","description":"Reviewed domain advice","tags":["ownership"],"source":"fixture","license":"CC0","workflow":"Inspect ownership evidence without claiming verification","model":selection})).unwrap()
}
fn config() -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 4,
        max_cycles: 1,
        ..Config::default()
    }
}
async fn server(
    fail: bool,
    disable: Option<Arc<Store>>,
) -> (String, tokio::task::JoinHandle<serde_json::Value>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut raw = Vec::new();
        let mut buffer = [0u8; 4096];
        let request = loop {
            let read = stream.read(&mut buffer).await.unwrap();
            assert!(read > 0);
            raw.extend_from_slice(&buffer[..read]);
            assert!(raw.len() < 64000);
            if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&raw[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if raw.len() >= end + 4 + length {
                    break serde_json::from_slice::<serde_json::Value>(
                        &raw[end + 4..end + 4 + length],
                    )
                    .unwrap();
                }
            }
        };
        if let Some(store) = disable {
            store
                .set_model_enabled(
                    &Selection {
                        id: "domain.local".into(),
                        version: "1.0.0".into(),
                    },
                    false,
                )
                .unwrap();
        }
        let body=json!({"model":"expert-provider/v1","choices":[{"message":{"content":"EXPERT ADVICE SENTINEL"}}],"usage":{"prompt_tokens":5,"completion_tokens":3}}).to_string();
        let response=format!("HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",if fail {"500 Internal Server Error"} else {"200 OK"},body.len(),body);
        stream.write_all(response.as_bytes()).await.unwrap();
        request
    });
    (endpoint, handle)
}

#[test]
fn binding_activation_and_snapshots_obey_registered_model_lifecycle() {
    let store = Store::in_memory().unwrap();
    let selection = Selection {
        id: "domain.local".into(),
        version: "1.0.0".into(),
    };
    let manifest = workflow(&selection);
    registry::register(&store, &manifest).unwrap();
    assert!(registry::activate(&store, &manifest.id, &manifest.version).is_err());
    register_model(&store, "http://127.0.0.1:9/v1");
    registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let pinned = store.task_context("ownership").unwrap();
    assert_eq!(
        pinned.expert_models[&manifest.id].manifest.model,
        "domain-model"
    );
    store.set_model_enabled(&selection, false).unwrap();
    assert!(store
        .task_context("ownership")
        .unwrap()
        .expert_models
        .is_empty());
    assert!(pinned.expert_models[&manifest.id].enabled);
    assert_eq!(
        registry::health(&store).unwrap()["active"][0]["model_health"],
        "unavailable"
    );
    assert!(registry::activate(&store, &manifest.id, &manifest.version).is_err());
    let mut invalid = manifest;
    invalid.kind = "reference".into();
    invalid.workflow = None;
    assert!(invalid.validate().is_err());
}

#[tokio::test]
async fn registered_expert_calls_its_backend_under_shared_budget_without_verification_claims() {
    let store = Arc::new(Store::in_memory().unwrap());
    let (endpoint, provider) = server(false, Some(store.clone())).await;
    let selection = register_model(&store, &endpoint);
    let manifest = workflow(&selection);
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let main = Arc::new(Main(Mutex::new(Vec::new())));
    let outcome = Engine::with_backend(config(), store.clone(), main.clone())
        .run("Explain ownership", "deep")
        .await
        .unwrap();
    let request = provider.await.unwrap();
    assert!(!store.model(&selection).unwrap().enabled);
    assert_eq!(request["model"], "domain-model");
    assert_eq!(request["max_tokens"], 192);
    assert_eq!(outcome.calls as usize, main.0.lock().unwrap().len() + 1);
    assert!(main
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("EXPERT ADVICE SENTINEL")));
    assert_eq!(outcome.status, "unverified");
    assert!(outcome.evidence.is_empty());
    let trace = store.task(&outcome.task_id).unwrap();
    assert!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "expert_model_snapshot"
                && e["payload"]["weights_attested"] == false)
    );
}

#[tokio::test]
async fn unavailable_and_budget_refused_experts_do_not_consume_model_calls() {
    let store = Arc::new(Store::in_memory().unwrap());
    let selection = register_model(&store, "http://127.0.0.1:9/v1");
    let manifest = workflow(&selection);
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let main = Arc::new(Main(Mutex::new(Vec::new())));
    store.set_model_enabled(&selection, false).unwrap();
    let first = Engine::with_backend(config(), store.clone(), main.clone())
        .run("Explain ownership", "deep")
        .await
        .unwrap();
    assert_eq!(first.calls as usize, main.0.lock().unwrap().len());
    store.set_model_enabled(&selection, true).unwrap();
    main.0.lock().unwrap().clear();
    let mut cfg = config();
    cfg.max_calls = 2;
    let second = Engine::with_backend(cfg, store.clone(), main.clone())
        .run("Explain ownership", "deep")
        .await
        .unwrap();
    assert_eq!(second.calls as usize, main.0.lock().unwrap().len());
    assert!(store.task(&second.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "expert_dispatch" && e["payload"]["admitted"] == false));
}

#[tokio::test]
async fn frozen_corpus_cannot_restore_a_disabled_expert_model() {
    let store = Arc::new(Store::in_memory().unwrap());
    let selection = register_model(&store, "http://127.0.0.1:9/v1");
    let manifest = workflow(&selection);
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let frozen = Arc::new(store.freeze_corpus().unwrap());
    store.set_model_enabled(&selection, false).unwrap();
    let main = Arc::new(Main(Mutex::new(Vec::new())));
    let outcome = Engine::with_backend(config(), store.clone(), main.clone())
        .with_frozen_corpus(frozen)
        .run("Explain ownership", "deep")
        .await
        .unwrap();
    assert_eq!(outcome.calls as usize, main.0.lock().unwrap().len());
    assert!(store.task(&outcome.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "expert_dispatch" && e["payload"]["admitted"] == false));
}
#[tokio::test]
async fn unclassified_bound_expert_failure_stops_without_primary_dispatch() {
    let (endpoint, provider) = server(true, None).await;
    let store = Arc::new(Store::in_memory().unwrap());
    let selection = register_model(&store, &endpoint);
    let manifest = workflow(&selection);
    registry::register(&store, &manifest).unwrap();
    registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let main = Arc::new(Main(Mutex::new(Vec::new())));
    let outcome = Engine::with_backend(config(), store.clone(), main.clone())
        .run("Explain ownership", "deep")
        .await
        .unwrap();
    provider.await.unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.calls, 1);
    assert_eq!(outcome.resources.reserved_output_tokens, 192);
    assert!(main.0.lock().unwrap().is_empty());
    assert!(outcome.resources.input_tokens.is_none() && outcome.resources.output_tokens.is_none());
    assert!(!store.task(&outcome.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "expert_failure"));
}

#[tokio::test]
async fn expert_selection_obeys_route_rank_and_falls_back_before_model_dispatch() {
    let store = Arc::new(Store::in_memory().unwrap());
    let selection = register_model(&store, "http://127.0.0.1:9/v1");
    let mut lower = workflow(&selection);
    lower.id = "aaa.workflow.s1".into();
    lower.model = None;
    lower.workflow = Some("LOWER RANK SENTINEL".into());
    let mut higher = lower.clone();
    higher.id = "zzz.workflow.s1".into();
    higher.tags.push("rust".into());
    higher.workflow = Some("HIGHER RANK SENTINEL".into());
    for manifest in [&lower, &higher] {
        registry::register(&store, manifest).unwrap();
        registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    }
    let main = Arc::new(Main(Mutex::new(Vec::new())));
    Engine::with_backend(config(), store.clone(), main.clone())
        .run("Explain Rust ownership", "deep")
        .await
        .unwrap();
    assert!(main
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("HIGHER RANK SENTINEL")));
    assert!(!main
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("LOWER RANK SENTINEL")));
    higher.version = "1.0.1".into();
    higher.model = Some(selection.clone());
    registry::register(&store, &higher).unwrap();
    registry::activate(&store, &higher.id, &higher.version).unwrap();
    store.set_model_enabled(&selection, false).unwrap();
    main.0.lock().unwrap().clear();
    let outcome = Engine::with_backend(config(), store.clone(), main.clone())
        .run("Explain Rust ownership", "deep")
        .await
        .unwrap();
    assert!(main
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("LOWER RANK SENTINEL")));
    assert!(!main
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("HIGHER RANK SENTINEL")));
    let trace = store.task(&outcome.task_id).unwrap();
    let dispatches = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "expert_dispatch")
        .collect::<Vec<_>>();
    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0]["payload"]["id"], higher.id);
    assert_eq!(dispatches[0]["payload"]["admitted"], false);
    assert_eq!(dispatches[1]["payload"]["id"], lower.id);
    assert_eq!(dispatches[1]["payload"]["admitted"], true);
    assert_eq!(outcome.status, "unverified");
}
