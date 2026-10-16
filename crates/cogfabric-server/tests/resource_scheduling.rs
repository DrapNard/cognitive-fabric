use axum::{extract::State, routing::post, Json, Router};
use cogfabric::{
    agent::AgentRuntime,
    config::Config,
    engine::Engine,
    resource_scheduler::{Allocation, Configuration, Settings},
    store::Store,
};
use cogfabric_server::{serve_until, Server};
use serde_json::{json, Value};
use std::{
    future::IntoFuture,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};
struct Provider {
    entered: AtomicUsize,
    changed: Notify,
    finish: Semaphore,
}
async fn provider(State(state): State<Arc<Provider>>) -> Json<Value> {
    state.entered.fetch_add(1, Ordering::SeqCst);
    state.changed.notify_one();
    state.finish.acquire().await.unwrap().forget();
    Json(
        json!({"model":"fixture/v1","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Fixture answer"}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}),
    )
}
#[tokio::test]
async fn real_http_native_and_collective_requests_share_configured_capacity() {
    let state = Arc::new(Provider {
        entered: AtomicUsize::new(0),
        changed: Notify::new(),
        finish: Semaphore::new(0),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let provider = tokio::spawn(
        axum::serve(
            listener,
            Router::new()
                .route("/v1/chat/completions", post(provider))
                .with_state(state.clone()),
        )
        .into_future(),
    );
    let cfg = Config {
        backend: "openai-local".into(),
        api_base: format!("http://{address}/v1"),
        interpret_goals: false,
        max_calls: 1,
        resource_admission: Some(Configuration {
            residents: serde_json::from_value(json!([{"id":"backbone.http_fixture","version":"1.0.0","weights_sha256":"a".repeat(64),"host_memory_bytes":65,"device_memory_bytes":65,"consumers":[{"api_base":format!("http://{address}/v1"),"model":"local-model"}]}])).unwrap(),
            settings: Settings {
                label: "gpu.main".into(),
                capacity: Allocation {
                    compute_slots: 2,
                    host_memory_bytes: 100,
                    device_memory_bytes: 100,
                },
                max_pending: 4,
                max_wait_ms: 3000,
                max_execution_ms: 3000,
            },
            allocation: Allocation {
                compute_slots: 1,
                host_memory_bytes: 20,
                device_memory_bytes: 20,
            },
        }),
        ..Config::default()
    };
    let store = Arc::new(Store::in_memory().unwrap());
    let engine = Engine::new(cfg.clone(), store.clone());
    let pool = engine.configured_resource_pool().unwrap().unwrap();
    let server = Server::new(
        AgentRuntime::local(cfg, store.clone()).unwrap(),
        engine,
        "fixture-resources-key-12345678901234567890",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let (stop, rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(serve_until(server, listener, async {
        let _ = rx.await;
    }));
    let request = |model: &str| {
        let url = base.clone();
        let body = json!({"model":model,"messages":[{"role":"user","content":"Describe a generic objective"}]});
        tokio::spawn(async move {
            reqwest::Client::new()
                .post(url)
                .bearer_auth("fixture-resources-key-12345678901234567890")
                .json(&body)
                .send()
                .await
                .unwrap()
        })
    };
    let first = request("cogfabric-agent");
    tokio::time::timeout(Duration::from_secs(3), state.changed.notified())
        .await
        .unwrap();
    let second = request("cogfabric-fast");
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let s = pool.snapshot().unwrap();
            if s.reservations.len() == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state.entered.load(Ordering::SeqCst), 1);
    assert_eq!(pool.snapshot().unwrap().allocated.compute_slots, 1);
    state.finish.add_permits(1);
    let first = first.await.unwrap();
    assert!(
        first.status().is_success(),
        "{}",
        first.text().await.unwrap()
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let changed = state.changed.notified();
            if state.entered.load(Ordering::SeqCst) == 2 {
                break;
            }
            changed.await;
        }
    })
    .await
    .unwrap();
    state.finish.add_permits(1);
    let second = second.await.unwrap();
    assert!(
        second.status().is_success(),
        "{}",
        second.text().await.unwrap()
    );
    let snapshot = pool.snapshot().unwrap();
    assert!(snapshot.reservations.is_empty());
    assert_eq!(
        snapshot.allocated,
        Allocation {
            compute_slots: 0,
            host_memory_bytes: 65,
            device_memory_bytes: 65
        }
    );
    assert_eq!(snapshot.working_allocated, Allocation::default());
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
    provider.abort();
}
