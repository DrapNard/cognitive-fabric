use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
use cogfabric::{
    config::Config,
    models::{Manifest, Selection},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    os::unix::fs::DirBuilderExt,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
const KEY: &str = "native-worker-server-fixture-1234567890";
const UPSTREAM_KEY: &str = "native-worker-upstream-fixture-1234567890";
struct Provider {
    pending: bool,
    entered: tokio::sync::Notify,
    calls: AtomicUsize,
    dropped: AtomicUsize,
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
async fn infer(
    State(p): State<Arc<Provider>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    assert_eq!(headers["authorization"], format!("Bearer {UPSTREAM_KEY}"));
    assert_eq!(body["model"], "native-worker-fixture");
    assert_eq!(body["max_tokens"], 128);
    assert_eq!(body["tools"][0]["function"]["name"], "read");
    assert!(!body.to_string().contains(KEY));
    p.calls.fetch_add(1, Ordering::SeqCst);
    p.entered.notify_one();
    let _active = Active(&p.dropped);
    if p.pending {
        std::future::pending::<()>().await;
    }
    Json(
        json!({"model":"native-worker-fixture","choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"read_1","type":"function","function":{"name":"read","arguments":"{\"path\":\"fixture.txt\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":20,"completion_tokens":12}}),
    )
}
fn request(stream: bool) -> Value {
    json!({"model":"cogfabric-agent","stream":stream,"max_tokens":128,"messages":[{"role":"user","content":"Read client data"}],"tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object"}}}]})
}
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
async fn native_managed_http_preserves_registered_tool_call_and_reaps_child() {
    run(Mode::Success).await;
}
#[tokio::test]
async fn native_managed_disconnect_reaps_child_and_preserves_unknown_usage() {
    run(Mode::StreamDisconnect).await;
}
#[tokio::test]
async fn native_managed_shutdown_cancels_and_reaps_pending_child() {
    run(Mode::Shutdown).await;
}
#[tokio::test]
async fn native_owned_audit_failure_prevents_upstream_inference_and_reaps_child() {
    run(Mode::OwnerAuditFailure).await;
}
#[tokio::test]
async fn native_nonstream_disconnect_reaps_child_after_runtime_future_drop() {
    run(Mode::NonstreamDisconnect).await;
}
#[tokio::test]
async fn native_shutdown_drains_cleanup_after_nonstream_disconnect() {
    run(Mode::NonstreamShutdown).await;
}
#[tokio::test]
async fn native_stop_audit_failure_blocks_more_children_and_fails_shutdown() {
    run(Mode::StopAuditFailure).await;
}
enum Mode {
    Success,
    StreamDisconnect,
    Shutdown,
    OwnerAuditFailure,
    NonstreamDisconnect,
    NonstreamShutdown,
    StopAuditFailure,
}
async fn run(mode: Mode) {
    let pending = matches!(
        mode,
        Mode::StreamDisconnect
            | Mode::Shutdown
            | Mode::NonstreamDisconnect
            | Mode::NonstreamShutdown
    );
    let shutdown = matches!(mode, Mode::Shutdown | Mode::NonstreamShutdown);
    let audit_failure = matches!(mode, Mode::OwnerAuditFailure);
    let nonstream = matches!(mode, Mode::NonstreamDisconnect | Mode::NonstreamShutdown);
    let stop_failure = matches!(mode, Mode::StopAuditFailure);
    // Clock resolution does not guarantee distinct names for parallel cases.
    // Reserve the directory atomically rather than sharing an existing database.
    static NEXT_HOME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let home = (0..128)
        .find_map(|_| {
            let sequence = NEXT_HOME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fabric-native-process-{}-{sequence}",
                std::process::id()
            ));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => Some(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => panic!("Cannot reserve native worker fixture directory: {error}"),
            }
        })
        .expect("Cannot reserve a unique native worker fixture directory");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let provider = Arc::new(Provider {
        pending,
        entered: tokio::sync::Notify::new(),
        calls: AtomicUsize::new(0),
        dropped: AtomicUsize::new(0),
    });
    let (provider_stop, stopped) = tokio::sync::oneshot::channel();
    let app = Router::new()
        .route("/v1/chat/completions", post(infer))
        .with_state(provider.clone());
    let provider_task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap()
    });
    let mut cfg = Config {
        home: home.clone(),
        backend: "openai-local".into(),
        model: "native-worker-fixture".into(),
        api_base: endpoint.clone(),
        inference_auth_scope: Some(endpoint.clone()),
        inference_process: true,
        max_tokens: 128,
        max_tool_calls: 1,
        max_wall_seconds: 15,
        ..Config::default()
    };
    cfg.prepare().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let manifest = Manifest {
        id: "native.fixture".into(),
        version: "1.0.0".into(),
        model: cfg.model.clone(),
        api_base: endpoint,
        source: "operator-local".into(),
        license: "Apache-2.0".into(),
        context_tokens: 4096,
        max_output_tokens: 128,
        weights_sha256: None,
    };
    let path = home.join("manifest.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    store.register_model(&path).unwrap();
    cfg = store
        .model_configuration(
            &cfg,
            &Selection {
                id: manifest.id,
                version: manifest.version,
            },
        )
        .unwrap();
    cfg.save().unwrap();
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reserve.local_addr().unwrap();
    drop(reserve);
    let mut server = Owned(
        Command::new(env!("CARGO_BIN_EXE_cogfabric-server"))
            .arg("--home")
            .arg(&home)
            .arg("--bind")
            .arg(addr.to_string())
            .env_clear()
            .env("COGFABRIC_SERVER_API_KEY", KEY)
            .env("COGFABRIC_INFERENCE_API_KEY", UPSTREAM_KEY)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap();
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
            assert!(server.0.try_wait().unwrap().is_none());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    if audit_failure {
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        db.execute_batch("CREATE TRIGGER fail_native_owned BEFORE INSERT ON events WHEN NEW.kind='native_worker_process_owned' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    }
    if stop_failure {
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        db.execute_batch("CREATE TRIGGER fail_native_stop BEFORE INSERT ON events WHEN NEW.kind='native_worker_process_stopped' BEGIN SELECT RAISE(ABORT,'stop audit unavailable'); END;").unwrap();
    }
    if nonstream {
        let caller = client.clone();
        let url = format!("{base}/v1/chat/completions");
        let pending = tokio::spawn(async move {
            caller
                .post(url)
                .bearer_auth(KEY)
                .json(&request(false))
                .send()
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), provider.entered.notified())
            .await
            .unwrap();
        assert!(
            store.recover_workers().is_err(),
            "Live native worker leases must block recovery"
        );
        pending.abort();
        let _ = pending.await;
        if shutdown {
            assert_eq!(unsafe { libc::kill(server.0.id() as i32, libc::SIGINT) }, 0);
        }
    } else {
        let response = client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(KEY)
            .json(&request(pending))
            .send()
            .await
            .unwrap();
        if pending {
            tokio::time::timeout(Duration::from_secs(5), provider.entered.notified())
                .await
                .unwrap();
            assert!(
                store.recover_workers().is_err(),
                "Live native worker leases must block recovery"
            );
            if shutdown {
                assert_eq!(unsafe { libc::kill(server.0.id() as i32, libc::SIGINT) }, 0);
                assert!(response.text().await.unwrap().contains("error"));
            } else {
                drop(response);
            }
        } else if audit_failure || stop_failure {
            assert_eq!(response.status(), 502);
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                usize::from(stop_failure)
            );
        } else {
            let body: Value = response.json().await.unwrap();
            assert_eq!(
                body["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
                "{\"path\":\"fixture.txt\"}"
            );
        }
    }
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    if stop_failure {
        assert_eq!(
            client
                .post(format!("{base}/v1/chat/completions"))
                .bearer_auth(KEY)
                .json(&request(false))
                .send()
                .await
                .unwrap()
                .status(),
            502
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM events WHERE kind='native_worker_process_launch'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(unsafe { libc::kill(server.0.id() as i32, libc::SIGINT) }, 0);
    } else {
        tokio::time::timeout(Duration::from_secs(5),async {loop {
        let reaped:u64=db.query_row("SELECT count(*) FROM events WHERE kind='native_worker_process_stopped' AND json_extract(payload,'$.reaped')=1",[],|r|r.get(0)).unwrap();if reaped==1 {break}tokio::time::sleep(Duration::from_millis(10)).await;
    }}).await.unwrap();
    }
    let (status, task): (String, String) = db
        .query_row(
            "SELECT status,id FROM tasks WHERE query LIKE 'Agent turn %'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    if nonstream && shutdown {
        assert!(matches!(status.as_str(), "interrupted" | "cancelled"));
    } else {
        assert_eq!(
            status,
            if nonstream {
                "interrupted"
            } else if pending {
                "cancelled"
            } else if audit_failure || stop_failure {
                "failed"
            } else {
                "unverified"
            }
        );
    }
    if !audit_failure {
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        tokio::time::timeout(Duration::from_secs(3), async {
            while provider.dropped.load(Ordering::SeqCst) != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Owned child exit must eventually close its provider connection");
        assert_eq!(provider.dropped.load(Ordering::SeqCst), 1);
    }
    let trace = store.task(&task).unwrap();
    assert!(!trace.to_string().contains(KEY));
    assert!(!trace.to_string().contains(UPSTREAM_KEY));
    assert!(!trace.to_string().contains("fixture.txt"));
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "model_registration"
            && e["payload"]["registration"]["manifest"]["id"] == "native.fixture"));
    if !audit_failure {
        assert!(trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "worker.audit" && e["payload"]["charged_calls"] == 1));
    }
    if shutdown || stop_failure {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(status) = server.0.try_wait().unwrap() {
                    assert_eq!(status.success(), !stop_failure);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    drop(server);
    drop(db);
    drop(store);
    provider_stop.send(()).unwrap();
    provider_task.await.unwrap();
    std::fs::remove_dir_all(home).unwrap();
}
