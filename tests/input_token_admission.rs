//! Native llama.cpp protocol fixtures; no weights or live-model quality claims.
use cogfabric::{
    backend::{InferenceBackend, LocalBackend},
    config::Config,
    store::Store,
    token_admission::Settings,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("fabric-token-fixture-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type PreparationHook = Box<dyn FnMut(&str) + Send>;

struct Fixture {
    base: String,
    requests: Arc<Mutex<Vec<(String, Value)>>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}
impl Fixture {
    async fn new(token_count: usize, expected_key: Option<&str>, completion_fails: bool) -> Self {
        Self::with_hook(token_count, expected_key, completion_fails, None).await
    }
    async fn with_hook(
        token_count: usize,
        expected_key: Option<&str>,
        completion_fails: bool,
        mut hook: Option<PreparationHook>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let key = expected_key.map(str::to_string);
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            loop {
                let (mut socket, _) = tokio::select! { result = listener.accept() => result.unwrap(), _ = &mut stopped => break };
                let mut bytes = Vec::new();
                let (path, body, headers) = tokio::time::timeout(Duration::from_secs(3), async {
                    loop {
                        let mut chunk = [0u8; 4096];
                        let read = socket.read(&mut chunk).await.unwrap();
                        assert!(read > 0 && bytes.len() + read <= 300_000);
                        bytes.extend_from_slice(&chunk[..read]);
                        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                            let length = headers
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .and_then(|n| n.parse::<usize>().ok())
                                })
                                .unwrap();
                            if bytes.len() >= end + 4 + length {
                                let path = headers
                                    .lines()
                                    .next()
                                    .unwrap()
                                    .split_whitespace()
                                    .nth(1)
                                    .unwrap()
                                    .to_string();
                                let body: Value =
                                    serde_json::from_slice(&bytes[end + 4..end + 4 + length])
                                        .unwrap();
                                break (path, body, headers);
                            }
                        }
                    }
                })
                .await
                .unwrap();
                let authorization = headers.lines().find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                        .map(|(_, value)| value.trim())
                });
                assert_eq!(
                    authorization.map(str::to_owned),
                    key.as_ref().map(|value| format!("Bearer {value}"))
                );
                captured.lock().unwrap().push((path.clone(), body.clone()));
                if let Some(hook) = &mut hook {
                    hook(&path);
                }
                let response = match path.as_str() {
                    "/apply-template" => {
                        assert!(body["messages"].is_array());
                        json!({"prompt":"rendered complete messages"})
                    }
                    "/tokenize" => {
                        assert_eq!(body["content"], "rendered complete messages");
                        assert_eq!(body["add_special"], false);
                        json!({"tokens":vec![17u32; token_count]})
                    }
                    "/completion" => {
                        assert_eq!(body["prompt"], json!(vec![17u32; token_count]));
                        assert_eq!(body["cache_prompt"], false);
                        json!({"model":"fixture-model","content":"fixture answer","truncated":false,"stop_type":"eos","tokens_evaluated":token_count,"tokens_predicted":1})
                    }
                    _ => panic!("Unexpected native route: {path}"),
                };
                let response = response.to_string();
                let status = if path == "/completion" && completion_fails {
                    "503 Unavailable"
                } else {
                    "200 OK"
                };
                socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
            }
        });
        Self {
            base,
            requests,
            stop: Some(stop),
            handle: Some(handle),
        }
    }
    fn completion_count(&self) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(path, _)| path == "/completion")
            .count()
    }
    async fn close(mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.handle.take().unwrap().await.unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}
fn config(root: &Root, fixture: &Fixture, maximum: u64) -> Config {
    Config {
        home: root.0.clone(),
        backend: "openai-local".into(),
        model: "fixture-model".into(),
        api_base: fixture.base.clone(),
        interpret_goals: false,
        input_token_admission: Some(Settings {
            format_version: 1,
            max_input_tokens: maximum,
            max_context_tokens: 128,
            authenticate: false,
        }),
        ..Config::default()
    }
}

#[tokio::test]
async fn exact_ids_are_submitted_and_failed_generation_never_refunds_input() {
    for fails in [false, true] {
        let root = Root::new();
        let fixture = Fixture::new(4, None, fails).await;
        let cfg = config(&root, &fixture, 6);
        let store = Store::open(&cfg.db()).unwrap();
        let task = store.create_task("Count exact IDs", "fast").unwrap();
        let backend = LocalBackend(cfg);
        let first = backend
            .generate_for(&task, "solver", "original objective", 16)
            .await;
        assert_eq!(first.is_ok(), !fails);
        assert!(backend
            .generate_for(&task, "solver", "another attempt", 16)
            .await
            .is_err());
        assert_eq!(fixture.completion_count(), 1);
        let trace = store.task(&task).unwrap();
        let charges: Vec<_> = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "input_token_admission")
            .collect();
        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0]["payload"]["cumulative_input_tokens"], 4);
        assert!(backend
            .generate("solver", "unscoped bypass", 16)
            .await
            .is_err());
        fixture.close().await;
    }
}

#[tokio::test]
async fn context_exhaustion_refuses_before_generation_and_charge() {
    let root = Root::new();
    let fixture = Fixture::new(120, None, false).await;
    let cfg = config(&root, &fixture, 1000);
    let store = Store::open(&cfg.db()).unwrap();
    let task = store.create_task("Context cap", "fast").unwrap();
    assert!(LocalBackend(cfg)
        .generate_for(&task, "solver", "bounded goal", 16)
        .await
        .is_err());
    assert_eq!(fixture.completion_count(), 0);
    assert!(!store.task(&task).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "input_token_admission"));
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_worker_preserves_exact_budget_and_isolates_native_credentials() {
    for authenticated in [false, true] {
        let root = Root::new();
        let key = authenticated.then_some("native-fixture-key-not-a-production-secret");
        let fixture = Fixture::new(4, key, false).await;
        let mut cfg = config(&root, &fixture, 32);
        cfg.input_token_admission.as_mut().unwrap().authenticate = authenticated;
        cfg.inference_auth_scope = Some(fixture.base.clone());
        cfg.inference_process = true;
        cfg.max_tokens = 1024;
        cfg.input_token_admission
            .as_mut()
            .unwrap()
            .max_context_tokens = 2048;
        cfg.max_calls = 4;
        cfg.save().unwrap();
        let output = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
                .arg("--home")
                .arg(&root.0)
                .args([
                    "--json",
                    "ask",
                    "Explain the supplied goal",
                    "--mode",
                    "fast",
                ])
                .env(
                    "COGFABRIC_INFERENCE_API_KEY",
                    "primary-key-must-not-be-forwarded-to-native-routes",
                )
                .env(
                    "COGFABRIC_LLAMA_API_KEY",
                    key.unwrap_or("must-not-be-forwarded-without-explicit-authentication"),
                )
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["answer"], "fixture answer");
        assert_eq!(fixture.completion_count(), 1);
        let store = Store::open(&cfg.db()).unwrap();
        let trace = store.task(result["task_id"].as_str().unwrap()).unwrap();
        assert!(trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "input_token_admission"));
        assert!(trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "worker_process_stopped"
                && event["payload"]["reaped"] == true));
        fixture.close().await;
    }
}

#[tokio::test]
async fn concurrent_admissions_and_audit_failure_cannot_exceed_the_budget() {
    for deny_audit in [false, true] {
        let root = Root::new();
        let fixture = Fixture::new(4, None, false).await;
        let cfg = config(&root, &fixture, 6);
        let store = Store::open(&cfg.db()).unwrap();
        let task = store
            .create_task("Concurrent token reservations", "fast")
            .unwrap();
        if deny_audit {
            rusqlite::Connection::open(cfg.db()).unwrap().execute_batch("CREATE TRIGGER deny_token_admission BEFORE INSERT ON events WHEN NEW.kind='input_token_admission' BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
        }
        let backend = LocalBackend(cfg);
        let (first, second) = tokio::join!(
            backend.generate_for(&task, "solver", "first attempt", 16),
            backend.generate_for(&task, "solver", "second attempt", 16)
        );
        assert_eq!(
            usize::from(first.is_ok()) + usize::from(second.is_ok()),
            usize::from(!deny_audit)
        );
        assert_eq!(fixture.completion_count(), usize::from(!deny_audit));
        fixture.close().await;
    }
}

#[tokio::test]
async fn tool_free_chat_preserves_history_and_rejects_unsupported_requests_before_network() {
    use cogfabric::agent::{AgentRuntime, ChatBackend, LocalChat, Turn};
    let root = Root::new();
    let fixture = Fixture::new(4, None, false).await;
    let cfg = config(&root, &fixture, 32);
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let task = store.create_task("Text conversation", "fast").unwrap();
    let request: Turn = serde_json::from_value(json!({"messages":[{"role":"system","content":"Keep the original goal"},{"role":"user","content":"Hello"},{"role":"assistant","content":"Earlier reply"},{"role":"user","content":"Continue"}],"max_tokens":16})).unwrap();
    let completion = LocalChat(cfg.clone())
        .complete_for(&task, &request, 16)
        .await
        .unwrap();
    assert_eq!(completion.content.as_deref(), Some("fixture answer"));
    assert_eq!(completion.finish_reason, "stop");
    let received = fixture.requests.lock().unwrap().clone();
    assert_eq!(
        received[0].1["messages"],
        serde_json::to_value(&request.messages).unwrap()
    );
    let runtime = AgentRuntime::local(cfg, store).unwrap();
    let mut tool_request = request.clone();
    tool_request.tools = serde_json::from_value(
        json!([{"type":"function","function":{"name":"read","parameters":{"type":"object"}}}]),
    )
    .unwrap();
    assert!(runtime.validate(&tool_request).is_err());
    let mut effort_request = request;
    effort_request.reasoning_effort = Some("high".into());
    assert!(runtime.validate(&effort_request).is_err());
    assert_eq!(fixture.requests.lock().unwrap().len(), received.len());
    fixture.close().await;
}

#[tokio::test]
async fn missing_and_terminal_tasks_never_contact_native_preparation() {
    let root = Root::new();
    let fixture = Fixture::new(4, None, false).await;
    let cfg = config(&root, &fixture, 1000);
    let store = Store::open(&cfg.db()).unwrap();
    let backend = LocalBackend(cfg);
    let missing = uuid::Uuid::new_v4().to_string();
    assert!(backend
        .generate_for(&missing, "solver", "private prompt", 16)
        .await
        .is_err());
    for status in ["completed", "cancelled", "failed", "interrupted"] {
        let task = store.create_task("Stopped task", "fast").unwrap();
        store.finish_task(&task, status, "").unwrap();
        assert!(backend
            .generate_for(&task, "solver", "private prompt", 16)
            .await
            .is_err());
        assert!(!store.task(&task).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "input_token_admission"));
    }
    assert!(fixture.requests.lock().unwrap().is_empty());
    fixture.close().await;
}

#[tokio::test]
async fn task_stopped_during_preparation_cannot_obtain_a_completion_reservation() {
    let root = Root::new();
    let mut cfg = Config {
        home: root.0.clone(),
        ..Config::default()
    };
    let store = Store::open(&cfg.db()).unwrap();
    let task = store.create_task("Cancellation race", "fast").unwrap();
    let db_path = cfg.db();
    let task_to_stop = task.clone();
    let fixture = Fixture::with_hook(
        4,
        None,
        false,
        Some(Box::new(move |path| {
            if path == "/tokenize" {
                rusqlite::Connection::open(&db_path)
                    .unwrap()
                    .execute(
                        "UPDATE tasks SET status='cancelled' WHERE id=?1",
                        [&task_to_stop],
                    )
                    .unwrap();
            }
        })),
    )
    .await;
    cfg = config(&root, &fixture, 1000);
    assert!(LocalBackend(cfg)
        .generate_for(&task, "solver", "original prompt", 16)
        .await
        .is_err());
    assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    assert_eq!(fixture.completion_count(), 0);
    assert!(!store.task(&task).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "input_token_admission"));
    fixture.close().await;
}
