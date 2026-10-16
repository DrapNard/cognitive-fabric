use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
fn turn() -> Turn {
    serde_json::from_value(json!({"messages":[{"role":"user","content":"Read a test file"}],"tools":[{"type":"function","function":{"name":"read","description":"Read client workspace","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}}],"tool_choice":"required","max_tokens":128})).unwrap()
}
fn completion() -> Completion {
    Completion {
        content: None,
        tool_calls: vec![ToolCall {
            id: "call_read_1".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: "read".into(),
                arguments: r#"{"path":"test.txt"}"#.into(),
            },
        }],
        model: "fixture/v1".into(),
        finish_reason: "tool_calls".into(),
        input_tokens: Some(20),
        output_tokens: Some(10),
    }
}
struct Fixed(AtomicUsize);
impl ChatBackend for Fixed {
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(completion()) })
    }
}
#[tokio::test]
async fn agent_proposes_without_executing_and_enforces_budget_before_dispatch() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Fixed(AtomicUsize::new(0)));
    let cfg = Config {
        max_tokens: 128,
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend.clone()).unwrap();
    let outcome = runtime
        .run_until(turn(), std::future::pending())
        .await
        .unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    assert_eq!(trace["task"]["status"], "unverified");
    assert_eq!(backend.0.load(Ordering::SeqCst), 1);
    let mut too_large = turn();
    too_large.max_tokens = Some(129);
    assert!(runtime
        .run_until(too_large, std::future::pending())
        .await
        .is_err());
    assert_eq!(backend.0.load(Ordering::SeqCst), 1);
    assert!(!trace.to_string().contains("test.txt"));
    let db = store.lock().unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM events WHERE kind='tool_dispatch'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM events WHERE kind='agent_admission'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn native_tool_protocol_rejects_forged_results_names_and_arguments() {
    let mut request = turn();
    request.validate().unwrap();
    let c = completion();
    c.validate(&request, 1).unwrap();
    assert!(c.validate(&request, 0).is_err());
    let mut bad = c.clone();
    bad.tool_calls[0].function.name = "shell".into();
    assert!(bad.validate(&request, 4).is_err());
    bad = c.clone();
    bad.tool_calls[0].function.arguments = "not json".into();
    assert!(bad.validate(&request, 4).is_err());
    request.messages.push(Message {
        role: "assistant".into(),
        content: None,
        tool_calls: c.tool_calls.clone(),
        tool_call_id: None,
    });
    assert!(request.validate().is_err());
    request.messages.push(Message {
        role: "tool".into(),
        content: Some("client result".into()),
        tool_calls: vec![],
        tool_call_id: Some("wrong_call".into()),
    });
    assert!(request.validate().is_err());
    request.messages.last_mut().unwrap().tool_call_id = Some("call_read_1".into());
    request.validate().unwrap();
    assert!(
        c.validate(&request, 4).is_err(),
        "A provider must not reuse a completed historical call ID"
    );
    let mut fresh = c.clone();
    fresh.tool_calls[0].id = "call_read_2".into();
    fresh.validate(&request, 4).unwrap();
    request.tool_choice = Some(json!("none"));
    assert!(c.validate(&request, 4).is_err());
}
struct Pending;
impl ChatBackend for Pending {
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
}
#[tokio::test]
async fn cancellation_is_durable_and_agent_policy_never_grants_tools() {
    let store = Arc::new(Store::in_memory().unwrap());
    let runtime =
        AgentRuntime::with_backend(Config::default(), store.clone(), Arc::new(Pending)).unwrap();
    assert!(runtime
        .run_until(turn(), std::future::ready(()))
        .await
        .is_err());
    let db = store.lock().unwrap();
    let (id, status): (String, String) = db
        .query_row("SELECT id,status FROM tasks", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(status, "cancelled");
    let policy = TaskPolicy::agent(&id).unwrap();
    assert!(policy.authorize(&id, "jev-r", "llm.generate").is_ok());
    for capability in ["math.evaluate", "sql.query", "shell.execute", "file.write"] {
        assert!(policy.authorize(&id, "jev-r", capability).is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn wall_deadline_exhausts_a_pending_turn_without_refunding_reservations() {
    let store = Arc::new(Store::in_memory().unwrap());
    let cfg = Config {
        max_wall_seconds: 1,
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store.clone(), Arc::new(Pending)).unwrap();
    assert!(runtime
        .run_until(turn(), std::future::pending())
        .await
        .is_err());
    let db = store.lock().unwrap();
    assert_eq!(
        db.query_row("SELECT status FROM tasks", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "exhausted"
    );
    assert_eq!(db.query_row("SELECT json_extract(payload,'$.reserved_output_tokens') FROM events WHERE kind='agent_admission'",[],|r|r.get::<_,u64>(0)).unwrap(),128);
}

#[tokio::test]
async fn registered_native_http_turn_pins_metadata_before_operator_disable() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest = crate::models::Manifest {
        id: "agent.small".into(),
        version: "1.0.0".into(),
        model: "registered-native-model".into(),
        api_base: endpoint,
        source: "operator-local".into(),
        license: "Apache-2.0".into(),
        context_tokens: 4096,
        max_output_tokens: 256,
        weights_sha256: None,
    };
    let path =
        std::env::temp_dir().join(format!("fabric-native-model-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    store.register_model(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let selection = crate::models::Selection {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
    };
    let cfg = store
        .model_configuration(&Config::default(), &selection)
        .unwrap();
    let mut mismatch = cfg.clone();
    mismatch.model = "another-model".into();
    assert!(AgentRuntime::local(mismatch, store.clone()).is_err());
    let mut excessive = cfg.clone();
    excessive.max_tokens = 257;
    assert!(AgentRuntime::local(excessive, store.clone()).is_err());
    let runtime = AgentRuntime::local(cfg.clone(), store.clone()).unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let provider = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut raw = Vec::new();
        loop {
            let mut buf = [0; 4096];
            let count = stream.read(&mut buf).await.unwrap();
            assert!(count > 0);
            raw.extend_from_slice(&buf[..count]);
            assert!(raw.len() < 64000);
            if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&raw[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if raw.len() >= end + 4 + length {
                    let body: serde_json::Value =
                        serde_json::from_slice(&raw[end + 4..end + 4 + length]).unwrap();
                    assert_eq!(body["model"], "registered-native-model");
                    assert_eq!(body["max_tokens"], 128);
                    break;
                }
            }
        }
        entered.send(()).unwrap();
        released.await.unwrap();
        let body=json!({"model":"registered-native-model","choices":[{"message":{"role":"assistant","content":"Registered answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":3}}).to_string();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
    });
    let running = runtime.clone();
    let mut request = turn();
    request.tools.clear();
    request.tool_choice = None;
    let task =
        tokio::spawn(async move { running.run_until(request, std::future::pending()).await });
    tokio::time::timeout(Duration::from_secs(3), started)
        .await
        .unwrap()
        .unwrap();
    store.set_model_enabled(&selection, false).unwrap();
    release.send(()).unwrap();
    let result = task.await.unwrap().unwrap();
    provider.await.unwrap();
    let trace = store.task(&result.task_id).unwrap();
    let saved = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "model_registration")
        .unwrap();
    assert_eq!(
        saved["payload"]["registration"]["manifest"]["model"],
        "registered-native-model"
    );
    assert_eq!(saved["payload"]["registration"]["enabled"], true);
    assert_eq!(saved["payload"]["weight_identity_verified"], false);
    assert!(runtime
        .run_until(turn(), std::future::pending())
        .await
        .is_err());
    assert!(AgentRuntime::local(cfg, store.clone()).is_err());
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn already_cancelled_turn_never_constructs_backend_inference() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Fixed(AtomicUsize::new(0)));
    let runtime = AgentRuntime::with_backend(Config::default(), store, backend.clone()).unwrap();
    assert!(runtime
        .run_until(turn(), std::future::ready(()))
        .await
        .is_err());
    assert_eq!(backend.0.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn already_cancelled_managed_turn_does_not_launch_a_child() {
    let home = std::env::temp_dir().join(format!("fabric-native-cancel-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: home.clone(),
        backend: "openai-local".into(),
        inference_process: true,
        ..Config::default()
    };
    cfg.prepare().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let runtime = AgentRuntime::local(cfg, store.clone()).unwrap();
    assert!(runtime
        .run_until(turn(), std::future::ready(()))
        .await
        .is_err());
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE kind='native_worker_process_launch'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row("SELECT status FROM tasks", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "cancelled"
    );
    drop(runtime);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn oversized_native_frame_is_rejected_before_task_or_child_admission() {
    let home = std::env::temp_dir().join(format!("fabric-native-frame-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: home.clone(),
        backend: "openai-local".into(),
        inference_process: true,
        max_input_bytes: 1_000_000,
        ..Config::default()
    };
    cfg.prepare().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let runtime = AgentRuntime::local(cfg, store.clone()).unwrap();
    let mut request = turn();
    request.messages = vec![
        Message {
            role: "user".into(),
            content: Some("x".repeat(150000)),
            tool_calls: vec![],
            tool_call_id: None
        };
        2
    ];
    request.validate().unwrap();
    assert!(runtime
        .run_until(request, std::future::pending())
        .await
        .is_err());
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
    drop(runtime);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn resident_configuration(base: &str, model: &str) -> crate::resource_scheduler::Configuration {
    serde_json::from_value(json!({"settings":{"label":"gpu.native","capacity":{"compute_slots":1,"host_memory_bytes":100,"device_memory_bytes":100},"max_pending":4,"max_wait_ms":1000,"max_execution_ms":1000},"allocation":{"compute_slots":1,"host_memory_bytes":20,"device_memory_bytes":20},"residents":[{"id":"backbone.native","version":"1.0.0","weights_sha256":"a".repeat(64),"host_memory_bytes":60,"device_memory_bytes":60,"consumers":[{"api_base":base,"model":model}]}]})).unwrap()
}
#[tokio::test]
async fn native_unknown_target_fails_before_completion_callback_and_retains_admission() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Fixed(AtomicUsize::new(0)));
    let cfg = Config {
        resource_admission: Some(resident_configuration("http://127.0.0.1:1234/v1", "listed")),
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend.clone()).unwrap();
    assert!(runtime
        .run_until(turn(), std::future::pending())
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("target is unknown"));
    assert_eq!(backend.0.load(Ordering::SeqCst), 0);
    let id: String = store
        .lock()
        .unwrap()
        .query_row("SELECT id FROM tasks", [], |row| row.get(0))
        .unwrap();
    let trace = store.task(&id).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "agent_admission" && e["payload"]["calls"] == 1)
            .count(),
        1
    );
    assert!(events
        .iter()
        .any(|e| e["kind"] == "resource_target" && e["payload"]["allowed"] == false));
    assert!(!events
        .iter()
        .any(|e| e["kind"] == "resource_dispatch" || e["kind"] == "agent_result"));
}
#[tokio::test]
async fn native_local_unlisted_target_cannot_contact_the_provider() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let store = Arc::new(Store::in_memory().unwrap());
    let cfg = Config {
        backend: "openai-local".into(),
        model: "unlisted".into(),
        api_base: base.clone(),
        resource_admission: Some(resident_configuration(&base, "listed")),
        ..Config::default()
    };
    let runtime = AgentRuntime::local(cfg, store).unwrap();
    assert!(runtime
        .run_until(turn(), std::future::pending())
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("absent from the resident catalog"));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
}
