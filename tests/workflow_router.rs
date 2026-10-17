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
