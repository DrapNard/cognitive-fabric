use cogfabric::{
    agent::{AgentRuntime, ChatBackend, Completion, FunctionCall, ToolCall, Turn},
    config::Config,
    engine::Engine,
    store::Store,
};
use cogfabric_server::{serve_until, Server, MODEL};
use serde_json::{json, Value};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
const KEY: &str = "fixture-server-token-12345678901234567890";
struct Fixture {
    calls: AtomicUsize,
    pending: bool,
    entered: tokio::sync::Notify,
    exited: AtomicUsize,
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl ChatBackend for Fixture {
    fn complete<'a>(
        &'a self,
        r: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Completion>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        Box::pin(async move {
            let _active = Active(&self.exited);
            if self.pending {
                std::future::pending::<()>().await;
            }
            let tools = r.messages.last().unwrap().role != "tool";
            Ok(Completion {
                content: if tools {
                    None
                } else {
                    Some("File content received".into())
                },
                tool_calls: if tools {
                    vec![ToolCall {
                        id: "read_1".into(),
                        kind: "function".into(),
                        function: FunctionCall {
                            name: "read".into(),
                            arguments: "{\"path\":\"fixture.txt\"}".into(),
                        },
                    }]
                } else {
                    vec![]
                },
                model: "fixture/v1".into(),
                finish_reason: if tools { "tool_calls" } else { "stop" }.into(),
                input_tokens: Some(30),
                output_tokens: Some(10),
            })
        })
    }
}
async fn setup(
    pending: bool,
) -> (
    String,
    Arc<Store>,
    Arc<Fixture>,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<anyhow::Result<()>>,
) {
    let cfg = Config::default();
    let store = Arc::new(Store::in_memory().unwrap());
    let fixture = Arc::new(Fixture {
        calls: AtomicUsize::new(0),
        pending,
        entered: tokio::sync::Notify::new(),
        exited: AtomicUsize::new(0),
    });
    let runtime = AgentRuntime::with_backend(cfg.clone(), store.clone(), fixture.clone()).unwrap();
    let server = Server::new(runtime, Engine::new(cfg, store.clone()), KEY).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(serve_until(server, listener, async {
        let _ = rx.await;
    }));
    (base, store, fixture, tx, handle)
}
fn body() -> Value {
    json!({"model":MODEL,"messages":[{"role":"user","content":[{"type":"text","text":"Read fixture.txt"}]}],"tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}}],"max_completion_tokens":128,"store":false})
}
#[tokio::test]
async fn native_two_turn_tools_and_streams_preserve_client_results() {
    let (base, _store, fixture, stop, handle) = setup(false).await;
    let client = reqwest::Client::new();
    let url = format!("{base}/v1/chat/completions");
    let first: Value = client
        .post(&url)
        .bearer_auth(KEY)
        .json(&body())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(first["choices"][0]["finish_reason"], "tool_calls");
    let mut next = body();
    next["messages"] = json!([{"role":"user","content":"Read fixture.txt"},first["choices"][0]["message"],{"role":"tool","tool_call_id":"read_1","content":"fixture contents"}]);
    next["stream"] = json!(true);
    next["stream_options"] = json!({"include_usage":true});
    let response = client
        .post(&url)
        .bearer_auth(KEY)
        .json(&next)
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let data = response.text().await.unwrap();
    assert!(data.contains("File content received"));
    assert!(data.contains("[DONE]"));
    assert!(data.contains("\"prompt_tokens\":30"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 2);
    stop.send(()).unwrap();
    handle.await.unwrap().unwrap();
}
#[tokio::test]
async fn unauthorized_invalid_and_oversized_requests_never_reach_inference() {
    let (base, _store, fixture, stop, handle) = setup(false).await;
    let client = reqwest::Client::new();
    let url = format!("{base}/v1/chat/completions");
    assert_eq!(
        client
            .post(&url)
            .json(&body())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .get(format!("{base}/v1/models"))
            .bearer_auth(KEY)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{base}/v1/models"))
            .bearer_auth(KEY)
            .header("Origin", "http://example.com")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let mut invalid = body();
    invalid["messages"] = json!([{"role":"tool","tool_call_id":"forged","content":"unsafe"}]);
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(KEY)
            .json(&invalid)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    invalid = body();
    invalid["model"] = json!("unknown");
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(KEY)
            .json(&invalid)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(KEY)
            .header("content-type", "application/json")
            .body("x".repeat(512 * 1024 + 1))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
    invalid = body();
    invalid["max_tokens"] = json!(17000);
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(KEY)
            .json(&invalid)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    stop.send(()).unwrap();
    handle.await.unwrap().unwrap();
}
#[tokio::test]
async fn disconnect_and_shutdown_cancel_pending_streams() {
    for disconnect in [true, false] {
        let (base, _store, fixture, stop, handle) = setup(true).await;
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .unwrap();
        let mut request = body();
        request["stream"] = json!(true);
        let response = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(KEY)
            .json(&request)
            .send()
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), fixture.entered.notified())
            .await
            .unwrap();
        if disconnect {
            drop(response);
            drop(client);
            tokio::time::timeout(Duration::from_secs(3), async {
                while fixture.exited.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("Disconnect must cancel inference before server shutdown");
            stop.send(()).unwrap();
        } else {
            stop.send(()).unwrap();
            assert!(response.text().await.unwrap().contains("error"));
        }
        tokio::time::timeout(Duration::from_secs(3), handle)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn bounded_admission_rejects_a_ninth_pending_turn() {
    let (base, _store, fixture, stop, handle) = setup(true).await;
    let client = reqwest::Client::new();
    let mut request = body();
    request["stream"] = json!(true);
    let mut responses = Vec::new();
    for _ in 0..8 {
        responses.push(
            client
                .post(format!("{base}/v1/chat/completions"))
                .bearer_auth(KEY)
                .json(&request)
                .send()
                .await
                .unwrap(),
        );
    }
    let rejected = client
        .post(format!("{base}/v1/chat/completions"))
        .bearer_auth(KEY)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), 429);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 8);
    drop(responses);
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(fixture.exited.load(Ordering::SeqCst), 8);
}

#[tokio::test]
async fn collective_alias_uses_existing_core_verification() {
    let (base, store, fixture, stop, handle) = setup(false).await;
    let response: Value = reqwest::Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .bearer_auth(KEY)
        .json(&json!({"model":"cogfabric-fast","messages":[{"role":"user","content":"calc:6*7"}]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["choices"][0]["message"]["content"], "42");
    let task = response["id"]
        .as_str()
        .unwrap()
        .strip_prefix("chatcmpl-")
        .unwrap();
    assert_eq!(store.task(task).unwrap()["task"]["status"], "completed");
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    stop.send(()).unwrap();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn server_binary_relaunches_core_math_and_sql_workers() {
    use std::process::{Command, Stdio};
    let home = std::env::temp_dir().join(format!("fabric-http-workers-{}", std::process::id()));
    let cfg = Config {
        home: home.clone(),
        backend: "openai-local".into(),
        arithmetic_process: true,
        sql_process: true,
        ..Config::default()
    };
    cfg.save().unwrap();
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reserve.local_addr().unwrap();
    drop(reserve);
    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_cogfabric-server"))
            .arg("--home")
            .arg(&home)
            .arg("--bind")
            .arg(addr.to_string())
            .env("COGFABRIC_SERVER_API_KEY", KEY)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if client
                .get(format!("{base}/v1/models"))
                .bearer_auth(KEY)
                .send()
                .await
                .is_ok()
            {
                break;
            }
            assert!(child.0.try_wait().unwrap().is_none());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    for (query, answer) in [
        ("calc:6*7", "42"),
        (
            r#"sql:{"tables":[],"query":"SELECT 42 AS answer"}"#,
            r#"{"columns":["answer"],"rows":[[42]]}"#,
        ),
    ] {
        let response: Value = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(KEY)
            .json(&json!({"model":"cogfabric-fast","messages":[{"role":"user","content":query}]}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["choices"][0]["message"]["content"], answer);
        let task = response["id"]
            .as_str()
            .unwrap()
            .strip_prefix("chatcmpl-")
            .unwrap();
        let trace = store.task(task).unwrap();
        assert_eq!(trace["task"]["status"], "completed");
        assert!(trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "tool_worker_process_stopped" && e["payload"]["reaped"] == true));
    }
    drop(child);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn disconnect_cancels_pending_nonstreaming_inference() {
    let (base, _store, fixture, stop, handle) = setup(true).await;
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap();
    let pending = tokio::spawn(async move {
        client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(KEY)
            .json(&body())
            .send()
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), fixture.entered.notified())
        .await
        .unwrap();
    pending.abort();
    let _ = pending.await;
    let cancelled = tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.exited.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        cancelled,
        "Non-streaming disconnect must drop pending inference"
    );
}

#[tokio::test]
async fn slow_authenticated_bodies_have_bounded_request_admission() {
    use tokio::io::AsyncWriteExt;
    let (base, _store, fixture, stop, handle) = setup(false).await;
    let addr = base.strip_prefix("http://").unwrap();
    let mut pending = Vec::new();
    for _ in 0..16 {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(format!("POST /v1/chat/completions HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {KEY}\r\nContent-Type: application/json\r\nContent-Length: 1000\r\n\r\n{{").as_bytes()).await.unwrap();
        pending.push(stream);
    }
    let client = reqwest::Client::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if client
                .get(format!("{base}/v1/models"))
                .bearer_auth(KEY)
                .send()
                .await
                .unwrap()
                .status()
                == 429
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    drop(pending);
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn duplicate_json_members_are_rejected_before_native_inference() {
    let (base, _store, fixture, stop, handle) = setup(false).await;
    let client = reqwest::Client::new();
    for raw in [
        r#"{"model":"cogfabric-agent","model":"cogfabric-fast","messages":[{"role":"user","content":"Read"}]}"#,
        r#"{"model":"cogfabric-agent","messages":[{"role":"user","content":"Read","\u0063ontent":"Different"}]}"#,
        r#"{"model":"cogfabric-agent","messages":[{"role":"user","content":"Read"}],"tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object","properties":{"path":{"type":"string","type":"number"}}}}}]}"#,
        r#"{"model":"cogfabric-agent","messages":[{"role":"assistant","tool_calls":[{"id":"read_1","type":"function","function":{"name":"read","arguments":"{\"path\":\"a\",\"path\":\"b\"}"}}]},{"role":"tool","tool_call_id":"read_1","content":"Result"}]}"#,
    ] {
        let response = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(KEY)
            .header("content-type", "application/json")
            .body(raw)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    }
    stop.send(()).unwrap();
    handle.await.unwrap().unwrap();
}
