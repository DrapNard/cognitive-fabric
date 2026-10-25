//! Native turns must select and execute through one stable profile snapshot.
use anyhow::{bail, Result};
use cogfabric::{
    agent::{AgentRuntime, ChatBackend, Completion, Turn},
    backend::{InferenceTarget, ModelTarget},
    config::Config,
    engine::Engine,
    resource_scheduler::{Allocation, Configuration, Phase, Settings, WorkingProfile},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Notify, Semaphore};

fn target(model: &str) -> ModelTarget {
    ModelTarget {
        api_base: "http://127.0.0.1:1/v1".into(),
        model: model.into(),
    }
}
fn configuration() -> Config {
    let allocation = Allocation {
        compute_slots: 1,
        host_memory_bytes: 20,
        device_memory_bytes: 30,
    };
    Config {
        resource_admission: Some(Configuration {
            settings: Settings {
                label: "agent.profile.fixture".into(),
                capacity: allocation,
                max_pending: 4,
                max_wait_ms: 3000,
                max_execution_ms: 3000,
                scheduling: None,
            },
            allocation,
            residents: vec![],
            gpu_observation: None,
            working_profiles: ["old", "new"]
                .into_iter()
                .map(|model| WorkingProfile {
                    id: format!("agent.{model}"),
                    target: target(model),
                    roles: vec!["solver".into()],
                    allocation,
                })
                .collect(),
        }),
        max_wall_seconds: 10,
        ..Config::default()
    }
}
fn turn() -> Turn {
    serde_json::from_value(
        json!({"messages":[{"role":"user","content":"Answer briefly"}],"max_tokens":32}),
    )
    .unwrap()
}
fn completion(model: &str) -> Completion {
    Completion {
        content: Some("Fixture answer".into()),
        tool_calls: vec![],
        model: model.into(),
        finish_reason: "stop".into(),
        input_tokens: Some(2),
        output_tokens: Some(2),
    }
}

struct Unpinnable(AtomicUsize);
impl ChatBackend for Unpinnable {
    fn inference_target(&self) -> Result<Option<InferenceTarget>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Some(InferenceTarget::LocalHttp(target("old"))))
    }
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(completion("old")) })
    }
}

#[tokio::test]
async fn profiled_native_turn_refuses_an_adapter_without_a_snapshot_before_metadata_or_generation()
{
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Unpinnable(AtomicUsize::new(0)));
    let runtime = AgentRuntime::with_backend(configuration(), store, backend.clone()).unwrap();
    let error = runtime
        .run_until(turn(), std::future::pending())
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("pinnable agent backend"));
    assert_eq!(backend.0.load(Ordering::SeqCst), 0);
}

struct State {
    route: Mutex<ModelTarget>,
    pins: AtomicUsize,
    lookups: AtomicUsize,
    entries: Mutex<Vec<String>>,
    changed: Notify,
    release: Semaphore,
}
struct Router(Arc<State>);
struct Snapshot {
    target: ModelTarget,
    state: Arc<State>,
}
impl ChatBackend for Router {
    fn pin(self: Arc<Self>) -> Result<Option<Arc<dyn ChatBackend>>> {
        let target = self.0.route.lock().unwrap().clone();
        self.0.pins.fetch_add(1, Ordering::SeqCst);
        Ok(Some(Arc::new(Snapshot {
            target,
            state: self.0.clone(),
        })))
    }
    fn inference_target(&self) -> Result<Option<InferenceTarget>> {
        bail!("Mutable router metadata must never be used after pinning")
    }
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async { bail!("Mutable router must never execute a pinned turn") })
    }
}
impl ChatBackend for Snapshot {
    fn inference_target(&self) -> Result<Option<InferenceTarget>> {
        self.state.lookups.fetch_add(1, Ordering::SeqCst);
        self.state.changed.notify_one();
        Ok(Some(InferenceTarget::LocalHttp(self.target.clone())))
    }
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async move {
            self.state
                .entries
                .lock()
                .unwrap()
                .push(self.target.model.clone());
            self.state.changed.notify_one();
            self.state.release.acquire().await.unwrap().forget();
            Ok(completion(&self.target.model))
        })
    }
}
async fn wait_for(state: &State, condition: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let notified = state.changed.notified();
            if condition() {
                break;
            }
            notified.await;
        }
    })
    .await
    .unwrap();
}
fn profile_events(trace: &Value) -> Vec<&Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "resource_profile")
        .collect()
}

#[tokio::test]
async fn local_native_adapter_dispatches_http_under_its_matching_profile() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(3), async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header_end, content_length) = loop {
                let mut chunk = [0; 1024];
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 8192);
                if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                    assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
                    let length = headers.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
                    }).unwrap();
                    assert!(length < 4096);
                    break (end + 4, length);
                }
            };
            while bytes.len() < header_end + content_length {
                let mut chunk = [0; 1024];
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 8192);
            }
            let body: Value = serde_json::from_slice(&bytes[header_end..header_end + content_length]).unwrap();
            assert_eq!(body["model"], "old");
            assert_eq!(body["max_tokens"], 32);
            let response = json!({"model":"old","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Fixture answer"}}],"usage":{"prompt_tokens":2,"completion_tokens":2}}).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).as_bytes()).await.unwrap();
        }).await.unwrap();
    });
    let mut cfg = configuration();
    cfg.backend = "openai-local".into();
    cfg.api_base = base.clone();
    cfg.model = "old".into();
    for profile in &mut cfg.resource_admission.as_mut().unwrap().working_profiles {
        profile.target.api_base = base.clone();
    }
    let store = Arc::new(Store::in_memory().unwrap());
    let runtime = AgentRuntime::local(cfg, store.clone()).unwrap();
    let result = runtime
        .run_until(turn(), std::future::pending())
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(result.completion.model, "old");
    let trace = store.task(&result.task_id).unwrap();
    let profiles = profile_events(&trace);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0]["payload"]["profile_id"], "agent.old");
    assert_eq!(profiles[0]["payload"]["target"]["target"]["api_base"], base);
    assert_eq!(profiles[0]["payload"]["allowed"], true);
    assert_eq!(trace["task"]["status"], "unverified");
}

#[tokio::test]
async fn queued_native_turn_retains_its_snapshot_and_new_turns_use_the_new_route() {
    let cfg = configuration();
    let store = Arc::new(Store::in_memory().unwrap());
    let state = Arc::new(State {
        route: Mutex::new(target("old")),
        pins: AtomicUsize::new(0),
        lookups: AtomicUsize::new(0),
        entries: Mutex::new(vec![]),
        changed: Notify::new(),
        release: Semaphore::new(0),
    });
    let runtime =
        AgentRuntime::with_backend(cfg.clone(), store.clone(), Arc::new(Router(state.clone())))
            .unwrap();
    let pool = Engine::new(cfg.clone(), store.clone())
        .configured_resource_pool()
        .unwrap()
        .unwrap();
    let first_runtime = runtime.clone();
    let first = tokio::spawn(async move {
        first_runtime
            .run_until(turn(), std::future::pending())
            .await
    });
    wait_for(&state, || state.entries.lock().unwrap().len() == 1).await;
    let second_runtime = runtime.clone();
    let second = tokio::spawn(async move {
        second_runtime
            .run_until(turn(), std::future::pending())
            .await
    });
    wait_for(&state, || state.lookups.load(Ordering::SeqCst) == 2).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = pool.snapshot().unwrap();
            if snapshot.reservations.len() == 2
                && snapshot
                    .reservations
                    .iter()
                    .any(|entry| entry.phase == Phase::Waiting)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state.entries.lock().unwrap().as_slice(), &["old"]);
    *state.route.lock().unwrap() = target("new");
    state.release.add_permits(1);
    let first = first.await.unwrap().unwrap();
    wait_for(&state, || state.entries.lock().unwrap().len() == 2).await;
    assert_eq!(state.entries.lock().unwrap().as_slice(), &["old", "old"]);
    state.release.add_permits(1);
    let second = second.await.unwrap().unwrap();
    state.release.add_permits(1);
    let third = runtime
        .run_until(turn(), std::future::pending())
        .await
        .unwrap();
    assert_eq!(
        state.entries.lock().unwrap().as_slice(),
        &["old", "old", "new"]
    );
    assert_eq!(state.pins.load(Ordering::SeqCst), 3);
    assert_eq!(state.lookups.load(Ordering::SeqCst), 3);
    for (result, model) in [(first, "old"), (second, "old"), (third, "new")] {
        assert_eq!(result.completion.model, model);
        let trace = store.task(&result.task_id).unwrap();
        assert_eq!(trace["task"]["status"], "unverified");
        let profiles = profile_events(&trace);
        assert_eq!(profiles.len(), 1);
        assert_eq!(
            profiles[0]["payload"]["profile_id"],
            format!("agent.{model}")
        );
        assert_eq!(profiles[0]["payload"]["target"]["target"]["model"], model);
        assert_eq!(
            profiles[0]["payload"]["allocation"],
            serde_json::to_value(cfg.resource_admission.as_ref().unwrap().allocation).unwrap()
        );
        for kind in [
            "resource_intent",
            "resource_admitted",
            "resource_dispatch",
            "resource_completed",
        ] {
            assert_eq!(
                trace["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|event| event["kind"] == kind)
                    .count(),
                1
            );
        }
    }
    let snapshot = pool.snapshot().unwrap();
    assert!(snapshot.reservations.is_empty());
    assert_eq!(snapshot.working_allocated, Allocation::default());
    assert!(!snapshot.physical_limits_enforced);
}
