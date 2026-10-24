use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    registry::Manifest,
    routing::{
        empirical::{Evidence, EvidenceRouter, Measurement, Pair, Study, Thresholds},
        Descriptor, ExpertRouter, Request, TagRouter, WorkflowRequest,
    },
    store::Store,
};
use serde_json::json;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
const QUERY: &str = r#"sort:{"values":[3,1,2]}"#;
fn fixture() -> (Config, Arc<Store>) {
    let cfg = Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 6,
        ..Config::default()
    };
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest:Manifest=serde_json::from_value(json!({"id":"sort.advice","version":"1.0.0","kind":"workflow","description":"Sorting workflow fixture","tags":["sort"],"capabilities":[{"id":"sorting.help","version":"1.0.0"}],"source":"Synthetic test fixture","license":"CC0-1.0","workflow":"Suggest an ascending sorting strategy"})).unwrap();
    cogfabric::registry::register(&store, &manifest).unwrap();
    cogfabric::registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    (cfg, store)
}
struct Script(Mutex<VecDeque<(&'static str, String)>>);
impl Script {
    fn new(values: Vec<(&'static str, &str)>) -> Arc<Self> {
        Arc::new(Self(Mutex::new(
            values.into_iter().map(|(r, s)| (r, s.into())).collect(),
        )))
    }
}
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("Unapproved extra inference");
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text,
                model: "fixture-model".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
                provider_metrics: None,
            })
        })
    }
}
struct Capture {
    request: Mutex<Option<WorkflowRequest>>,
    ranked: AtomicUsize,
    order: Option<Vec<String>>,
    select_empty: bool,
    store: Arc<Store>,
}
impl Capture {
    fn new(store: Arc<Store>) -> Arc<Self> {
        Arc::new(Self {
            request: Mutex::new(None),
            ranked: AtomicUsize::new(0),
            order: None,
            select_empty: false,
            store,
        })
    }
}
impl ExpertRouter for Capture {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "fixture.capture".into(),
            version: "1.0.0".into(),
        }
    }
    fn select(&self, request: Request<'_>) -> Result<Vec<String>> {
        if self.select_empty {
            Ok(vec![])
        } else {
            TagRouter.select(request)
        }
    }
    fn rank_workflows(&self, request: &WorkflowRequest) -> Result<Vec<String>> {
        self.ranked.fetch_add(1, Ordering::SeqCst);
        let store = self.store.clone();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(store.skills());
        });
        receive.recv_timeout(std::time::Duration::from_secs(2))??;
        *self.request.lock().unwrap() = Some(request.clone());
        Ok(self
            .order
            .clone()
            .unwrap_or_else(|| request.candidates.iter().map(|c| c.id.clone()).collect()))
    }
}
fn data(request: &WorkflowRequest) -> Evidence {
    let candidate = &request.candidates[0];
    let sample = |verified, extra| Measurement {
        task_id: uuid::Uuid::new_v4().to_string(),
        verified,
        model_calls: 1 + u32::from(extra),
        reserved_output_tokens: 384 + if extra { 192 } else { 0 },
        prompt_bytes: 100 + if extra { 100 } else { 0 },
        tool_calls: 0,
        elapsed_ms: 10 + if extra { 10 } else { 0 },
    };
    Evidence {
        format_version: 1,
        mode: request.mode.clone(),
        criterion: request.criterion.clone().unwrap(),
        configuration_sha256: request.configuration_sha256.clone(),
        role_prompts_sha256: request.role_prompts_sha256.clone(),
        runtime_sources_sha256: request.runtime_sources_sha256.clone(),
        pinned_evidence_sha256: request.evidence_sha256.clone(),
        dataset_sha256: "d".repeat(64),
        source: "Synthetic paired fixture; no real utility evidence".into(),
        license: "CC0-1.0".into(),
        reviewer: "fixture.reviewer".into(),
        held_out_operator_attested: true,
        independent_outcomes_operator_attested: true,
        includes_failed_attempts_operator_attested: true,
        studies: vec![Study {
            workflow_id: candidate.id.clone(),
            workflow_version: candidate.version.clone(),
            workflow_sha256: candidate.snapshot_sha256.clone(),
            pairs: (0..4)
                .map(|i| Pair {
                    case_sha256: cogfabric::store::sha256(format!("case-{i}").as_bytes()),
                    baseline: sample(i == 3, false),
                    expert: sample(i < 3, true),
                })
                .collect(),
        }],
    }
}
fn thresholds(max: u64) -> Thresholds {
    Thresholds {
        format_version: 1,
        min_pairs: 4,
        min_net_successes: 2,
        min_net_gain_per_mille: 500,
        max_extra_output_tokens_per_net_success: max,
        main_solver_reserve_ms: 100,
    }
}
#[tokio::test]
async fn reviewed_policy_admits_or_declines_real_engine_workflow_without_bypassing_verification() {
    let (cfg, store) = fixture();
    let capture = Capture::new(store.clone());
    let engine = Engine::with_backend(
        cfg.clone(),
        store.clone(),
        Script::new(vec![
            ("solver", "Advice"),
            ("planner", "invalid plan"),
            ("solver", "[1,2,3]"),
        ]),
    )
    .with_expert_router(capture.clone())
    .unwrap();
    assert_eq!(
        engine
            .run_profile(QUERY, "deep", ExecutionProfile::Experts)
            .await
            .unwrap()
            .status,
        "completed"
    );
    let request = capture.request.lock().unwrap().clone().unwrap();
    let data = data(&request);
    let directory =
        std::env::temp_dir().join(format!("fabric-workflow-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let study = directory.join("study.json");
    let limits = directory.join("thresholds.json");
    let home = directory.join("absent-home");
    std::fs::write(&study, serde_json::to_vec(&data).unwrap()).unwrap();
    std::fs::write(&limits, serde_json::to_vec(&thresholds(384)).unwrap()).unwrap();
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("workflow-study-evaluate")
        .arg(&study)
        .arg("--thresholds")
        .arg(&limits)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["installed"], false);
    assert_eq!(report["permissions_granted"], false);
    assert!(!home.exists());
    std::fs::remove_dir_all(directory).unwrap();
    for allow in [true, false] {
        let router = Arc::new(
            EvidenceRouter::from_json(
                &serde_json::to_vec(&data).unwrap(),
                thresholds(if allow { 384 } else { 383 }),
            )
            .unwrap(),
        );
        let script = Script::new(if allow {
            vec![
                ("solver", "Advice"),
                ("planner", "invalid plan"),
                ("solver", "[1,2,3]"),
            ]
        } else {
            vec![("planner", "invalid plan"), ("solver", "[1,2,3]")]
        });
        let engine = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
            .with_expert_router(router)
            .unwrap();
        let result = engine
            .run_profile(QUERY, "deep", ExecutionProfile::Experts)
            .await
            .unwrap();
        assert_eq!(result.status, "completed");
        assert_eq!(result.calls, if allow { 3 } else { 2 });
        assert_eq!(result.resources.tool_calls, 2); // Retrieval and deterministic verification.
        assert!(script.0.lock().unwrap().is_empty());
        let trace = store.task(&result.task_id).unwrap();
        let order = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "workflow_routing")
            .unwrap();
        assert_eq!(
            order["payload"]["order"],
            if allow {
                json!(["sort.advice"])
            } else {
                json!([])
            }
        );
    }
}
#[tokio::test]
async fn capability_refinement_cannot_reintroduce_declined_workflow_execution() {
    let (mut cfg, store) = fixture();
    cfg.interpret_goals = true;
    let capture = Arc::new(Capture {
        request: Mutex::new(None),
        ranked: AtomicUsize::new(0),
        order: Some(vec![]),
        select_empty: true,
        store: store.clone(),
    });
    let intent=json!({"objective":QUERY,"constraints":[],"required_capabilities":["sorting.help"],"clarification":null}).to_string();
    let script = Script::new(vec![
        ("interface", &intent),
        ("planner", "invalid plan"),
        ("solver", "[1,2,3]"),
    ]);
    let engine = Engine::with_backend(cfg, store.clone(), script.clone())
        .with_expert_router(capture.clone())
        .unwrap();
    let result = engine
        .run_profile(QUERY, "deep", ExecutionProfile::Experts)
        .await
        .unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(result.calls, 3);
    assert_eq!(capture.ranked.load(Ordering::SeqCst), 1);
    assert_eq!(
        capture.request.lock().unwrap().as_ref().unwrap().candidates[0].id,
        "sort.advice"
    );
    assert!(script.0.lock().unwrap().is_empty());
    let trace = store.task(&result.task_id).unwrap();
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "expert_phase_budget"));
}
#[tokio::test]
async fn rejected_policy_audits_or_invalid_orders_never_admit_or_invoke_experts() {
    for kind in [
        "workflow_routing_intent",
        "workflow_routing",
        "invalid-order",
    ] {
        let (cfg, store) = fixture();
        let capture = Arc::new(Capture {
            request: Mutex::new(None),
            ranked: AtomicUsize::new(0),
            order: if kind == "invalid-order" {
                Some(vec!["unknown.expert".into()])
            } else {
                None
            },
            select_empty: false,
            store: store.clone(),
        });
        let _directory = if kind != "invalid-order" {
            let directory =
                std::env::temp_dir().join(format!("fabric-workflow-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&directory).unwrap();
            let path = directory.join("fabric.sqlite3");
            let persistent = Arc::new(Store::open(&path).unwrap());
            let manifest: Manifest = serde_json::from_value(json!({"id":"sort.advice","version":"1.0.0","kind":"workflow","description":"Sorting workflow fixture","tags":["sort"],"source":"Synthetic test fixture","license":"CC0-1.0","workflow":"Suggest an ascending sorting strategy"})).unwrap();
            cogfabric::registry::register(&persistent, &manifest).unwrap();
            cogfabric::registry::activate(&persistent, &manifest.id, &manifest.version).unwrap();
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch(&format!("CREATE TRIGGER refuse BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
            Some((directory, persistent))
        } else {
            None
        };
        let store = _directory
            .as_ref()
            .map(|(_, store)| store.clone())
            .unwrap_or(store);
        let engine = Engine::with_backend(cfg, store.clone(), Script::new(vec![]))
            .with_expert_router(capture)
            .unwrap();
        let result = engine
            .run_profile(QUERY, "deep", ExecutionProfile::Experts)
            .await
            .unwrap();
        assert_eq!(result.status, "failed");
        assert_eq!(result.calls, 0);
        let trace = store.task(&result.task_id).unwrap();
        assert!(!trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "expert_phase_budget"));
    }
}

#[tokio::test]
async fn cli_direct_and_service_pin_reviewed_policy_before_inference() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let directory =
        std::env::temp_dir().join(format!("fabric-routing-host-{}", uuid::Uuid::new_v4()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (mut cfg, _) = fixture();
    cfg.home = directory.join("state");
    cfg.api_base = format!("http://{}/v1", listener.local_addr().unwrap());
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    cogfabric::store::bootstrap(&store).unwrap();
    let manifest: Manifest = serde_json::from_value(json!({"id":"sort.advice","version":"1.0.0","kind":"workflow","description":"Sorting workflow fixture","tags":["sort"],"capabilities":[{"id":"sorting.help","version":"1.0.0"}],"source":"Synthetic test fixture","license":"CC0-1.0","workflow":"Suggest an ascending sorting strategy"})).unwrap();
    cogfabric::registry::register(&store, &manifest).unwrap();
    cogfabric::registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let capture = Capture::new(store.clone());
    let engine = Engine::with_backend(
        cfg.clone(),
        store.clone(),
        Script::new(vec![
            ("solver", "Advice"),
            ("planner", "invalid plan"),
            ("solver", "[1,2,3]"),
        ]),
    )
    .with_expert_router(capture.clone())
    .unwrap();
    let captured = engine.run(QUERY, "deep").await.unwrap();
    assert_eq!(captured.status, "completed");
    let evidence = data(capture.request.lock().unwrap().as_ref().unwrap());
    let study = directory.join("study.json");
    let limits = directory.join("thresholds.json");
    std::fs::write(&study, serde_json::to_vec(&evidence).unwrap()).unwrap();
    std::fs::write(&limits, serde_json::to_vec(&thresholds(384)).unwrap()).unwrap();
    let expected =
        EvidenceRouter::from_json(&serde_json::to_vec(&evidence).unwrap(), thresholds(384))
            .unwrap()
            .descriptor();
    let server = tokio::spawn(async move {
        // Direct accepted, direct declined, service accepted: no extra requests.
        for answer in [
            "Advice",
            "invalid plan",
            "[1,2,3]",
            "invalid plan",
            "[1,2,3]",
            "Advice",
            "invalid plan",
            "[1,2,3]",
        ] {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(10), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0u8; 2048];
                let n = stream.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 64_000);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let body = json!({"choices":[{"message":{"content":answer}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}).to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
        }
    });
    for allow in [true, false] {
        std::fs::write(
            &limits,
            serde_json::to_vec(&thresholds(if allow { 384 } else { 383 })).unwrap(),
        )
        .unwrap();
        let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&cfg.home)
            .arg("--json")
            .arg("ask")
            .arg(QUERY)
            .args(["--mode", "deep", "--workflow-study"])
            .arg(&study)
            .arg("--workflow-thresholds")
            .arg(&limits)
            .env_clear()
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(outcome["calls"], if allow { 3 } else { 2 });
        assert_eq!(outcome["status"], "completed");
    }
    std::fs::write(&limits, serde_json::to_vec(&thresholds(384)).unwrap()).unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&cfg.home)
        .arg("serve")
        .arg("--workflow-study")
        .arg(&study)
        .arg("--workflow-thresholds")
        .arg(&limits)
        .env_clear()
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Service exited during startup"
            );
            if cogfabric::service::status(&cfg).await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    // File mutation cannot replace a running host's pinned policy.
    std::fs::write(&study, "invalid replacement").unwrap();
    std::fs::write(&limits, "invalid replacement").unwrap();
    let outcome = cogfabric::service::request(&cfg, QUERY.into(), "deep".into())
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.calls, 3);
    let trace = store.task(&outcome.task_id).unwrap();
    let routing = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "workflow_routing")
        .unwrap();
    assert_eq!(
        routing["payload"]["router"],
        serde_json::to_value(expected).unwrap()
    );
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    server.await.unwrap();
    drop(engine);
    drop(capture);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_rejects_invalid_or_partial_policy_before_creating_state() {
    let directory =
        std::env::temp_dir().join(format!("fabric-routing-invalid-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let home = directory.join("absent");
    let study = directory.join("study.json");
    let limits = directory.join("limits.json");
    std::fs::write(&study, "{}").unwrap();
    std::fs::write(&limits, serde_json::to_vec(&thresholds(384)).unwrap()).unwrap();
    for command in ["ask", "serve"] {
        for partial in [true, false] {
            let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"));
            cmd.arg("--home").arg(&home).arg(command);
            if command == "ask" {
                cmd.arg(QUERY);
            }
            cmd.arg("--workflow-study").arg(&study);
            if !partial {
                cmd.arg("--workflow-thresholds").arg(&limits);
            }
            assert!(!cmd.env_clear().output().unwrap().status.success());
            assert!(!home.exists());
        }
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args(["ask", QUERY, "--socket", "--workflow-study"])
        .arg(&study)
        .arg("--workflow-thresholds")
        .arg(&limits)
        .env_clear()
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!home.exists());
    std::fs::remove_dir_all(directory).unwrap();
}
