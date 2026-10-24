use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend, InferenceTarget, ModelTarget},
    config::Config,
    engine::{Engine, ExecutionProfile},
    fabric::ModuleDescriptor,
    resource_scheduler::{Allocation, Configuration, Settings, WorkingProfile},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    process::{Command, Output},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::Semaphore;

fn alloc(compute_slots: u32, host_memory_bytes: u64, device_memory_bytes: u64) -> Allocation {
    Allocation {
        compute_slots,
        host_memory_bytes,
        device_memory_bytes,
    }
}

fn target(api_base: &str, model: &str) -> ModelTarget {
    ModelTarget {
        api_base: api_base.into(),
        model: model.into(),
    }
}

fn settings(capacity: Allocation) -> Settings {
    Settings {
        label: "gpu.test".into(),
        capacity,
        max_pending: 8,
        max_wait_ms: 3_000,
        max_execution_ms: 3_000,
        scheduling: None,
    }
}

fn configuration(capacity: Allocation, profiles: Vec<WorkingProfile>) -> Configuration {
    Configuration {
        settings: settings(capacity),
        allocation: alloc(1, 1, 1),
        residents: Vec::new(),
        working_profiles: profiles,
    }
}

fn profile(
    id: &str,
    target: ModelTarget,
    roles: &[&str],
    allocation: Allocation,
) -> WorkingProfile {
    WorkingProfile {
        id: id.into(),
        target,
        roles: roles.iter().map(|role| (*role).into()).collect(),
        allocation,
    }
}

fn base_config(home: std::path::PathBuf, base: &str, resources: Configuration) -> Config {
    Config {
        home,
        backend: "openai-local".into(),
        model: "solver-model".into(),
        api_base: base.into(),
        interpret_goals: false,
        max_calls: 8,
        resource_admission: Some(resources),
        ..Config::default()
    }
}

fn test_home(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cogfabric-{label}-{}", uuid::Uuid::new_v4()))
}

#[derive(Clone)]
struct BackendCall {
    target: Option<InferenceTarget>,
}

struct ScriptBackend {
    descriptor: Option<ModuleDescriptor>,
    targets: BTreeMap<String, Option<InferenceTarget>>,
    answers: Mutex<BTreeMap<String, VecDeque<&'static str>>>,
    calls: Arc<Mutex<Vec<BackendCall>>>,
    metadata_error: Mutex<Option<String>>,
    metadata_reads: AtomicUsize,
    blocked_dispatches_remaining: AtomicUsize,
    started: Arc<Semaphore>,
    release: Arc<Semaphore>,
}

impl ScriptBackend {
    fn new(
        targets: BTreeMap<String, Option<InferenceTarget>>,
        answers: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> Arc<Self> {
        Self::with_options(targets, answers, None, false)
    }

    fn with_options(
        targets: BTreeMap<String, Option<InferenceTarget>>,
        answers: impl IntoIterator<Item = (&'static str, &'static str)>,
        descriptor: Option<ModuleDescriptor>,
        block_first: bool,
    ) -> Arc<Self> {
        let mut by_role = BTreeMap::<String, VecDeque<&'static str>>::new();
        for (role, answer) in answers {
            by_role.entry(role.into()).or_default().push_back(answer);
        }
        Arc::new(Self {
            descriptor,
            targets,
            answers: Mutex::new(by_role),
            calls: Arc::new(Mutex::new(Vec::new())),
            metadata_error: Mutex::new(None),
            metadata_reads: AtomicUsize::new(0),
            blocked_dispatches_remaining: AtomicUsize::new(usize::from(block_first)),
            started: Arc::new(Semaphore::new(0)),
            release: Arc::new(Semaphore::new(0)),
        })
    }

    async fn wait_started(&self, count: usize) {
        let permit_count = u32::try_from(count).unwrap();
        tokio::time::timeout(
            Duration::from_secs(3),
            self.started.clone().acquire_many_owned(permit_count),
        )
        .await
        .expect("backend dispatch should start")
        .expect("backend start semaphore remains open")
        .forget();
    }

    fn dispatch_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl InferenceBackend for ScriptBackend {
    fn inference_target(&self, role: &str) -> Result<Option<InferenceTarget>> {
        self.metadata_reads.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.metadata_error.lock().unwrap().clone() {
            anyhow::bail!(error)
        }
        Ok(self.targets.get(role).cloned().flatten())
    }

    fn module_descriptor(&self) -> Option<&ModuleDescriptor> {
        self.descriptor.as_ref()
    }

    fn generate<'a>(
        &'a self,
        role: &'a str,
        _prompt: &'a str,
        _max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let answer = self
            .answers
            .lock()
            .unwrap()
            .get_mut(role)
            .and_then(VecDeque::pop_front)
            .unwrap_or("[1,2,3]");
        let target = self.targets.get(role).cloned().flatten();
        self.calls.lock().unwrap().push(BackendCall { target });
        let mut remaining = self.blocked_dispatches_remaining.load(Ordering::SeqCst);
        let block = loop {
            if remaining == 0 {
                break false;
            }
            match self.blocked_dispatches_remaining.compare_exchange_weak(
                remaining,
                remaining - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break true,
                Err(current) => remaining = current,
            }
        };
        let started = self.started.clone();
        let release = self.release.clone();
        Box::pin(async move {
            started.add_permits(1);
            if block {
                release.acquire().await?.forget();
            }
            Ok(Generation {
                text: answer.into(),
                model: "scripted-profile-fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: None,
                provider_metrics: None,
            })
        })
    }
}

fn local_targets(base: &str) -> BTreeMap<String, Option<InferenceTarget>> {
    BTreeMap::from([
        (
            "planner".into(),
            Some(InferenceTarget::LocalHttp(target(base, "planner-model"))),
        ),
        (
            "solver".into(),
            Some(InferenceTarget::LocalHttp(target(base, "solver-model"))),
        ),
    ])
}

fn assert_profile_chain(
    events: &[Value],
    role: &str,
    expected_target: &ModelTarget,
    expected: Allocation,
) {
    let decisions = events
        .iter()
        .filter(|event| event["kind"] == "resource_profile" && event["payload"]["role"] == role)
        .collect::<Vec<_>>();
    assert!(!decisions.is_empty(), "missing profile decision for {role}");
    for event in decisions {
        let value = &event["payload"];
        assert_eq!(value["allowed"], true);
        assert_eq!(value["reason_code"], "configured_profile_match");
        assert_eq!(value["target"]["kind"], "local_http");
        assert_eq!(value["target"]["target"], json!(expected_target));
        assert_eq!(value["profile_id"], format!("{role}.profile"));
        assert_eq!(value["allocation"], json!(expected));
        assert_eq!(value["permissions_granted"], false);
        assert_eq!(value["physical_limits_enforced"], false);
        assert!(value["working_profiles_sha256"].as_str().unwrap().len() == 64);
        let reservation = value["reservation_id"].as_str().unwrap();
        let matching_intent = events
            .iter()
            .find(|candidate| {
                candidate["kind"] == "resource_profile_intent"
                    && candidate["payload"]["reservation_id"] == reservation
            })
            .unwrap();
        assert_eq!(matching_intent["payload"]["role"], role);
        assert_eq!(
            matching_intent["payload"]["working_profiles_sha256"],
            value["working_profiles_sha256"]
        );
        let resource_intent = events
            .iter()
            .find(|candidate| {
                candidate["kind"] == "resource_intent"
                    && candidate["payload"]["reservation_id"] == reservation
            })
            .unwrap();
        assert_eq!(resource_intent["payload"]["allocation"], json!(expected));
    }
}

#[test]
fn empty_profiles_are_omitted_and_configuration_rejects_invalid_catalogs() {
    let base = "http://127.0.0.1:4321/v1";
    let empty = configuration(alloc(2, 100, 100), Vec::new());
    empty.validate().unwrap();
    let serialized = serde_json::to_value(&empty).unwrap();
    assert!(serialized.get("working_profiles").is_none());
    let legacy: Configuration = serde_json::from_value(json!({
        "settings": serialized["settings"],
        "allocation": serialized["allocation"],
        "residents": []
    }))
    .unwrap();
    assert!(legacy.working_profiles.is_empty());

    let good = profile(
        "solver.profile",
        target(base, "solver-model"),
        &["solver"],
        alloc(1, 20, 30),
    );
    configuration(alloc(2, 100, 100), vec![good.clone()])
        .validate()
        .unwrap();

    let duplicate_id = profile(
        "solver.profile",
        target(base, "other"),
        &["critic"],
        alloc(1, 10, 10),
    );
    assert!(
        configuration(alloc(2, 100, 100), vec![good.clone(), duplicate_id])
            .validate()
            .is_err()
    );

    let overlap = profile(
        "second.profile",
        target(base, "solver-model"),
        &["solver"],
        alloc(1, 10, 10),
    );
    assert!(
        configuration(alloc(2, 100, 100), vec![good.clone(), overlap])
            .validate()
            .is_err()
    );

    let duplicate_role = profile(
        "duplicate.role",
        target(base, "other"),
        &["solver", "solver"],
        alloc(1, 10, 10),
    );
    assert!(configuration(alloc(2, 100, 100), vec![duplicate_role])
        .validate()
        .is_err());

    let unknown_role = profile(
        "unknown.role",
        target(base, "other"),
        &["invented"],
        alloc(1, 10, 10),
    );
    assert!(configuration(alloc(2, 100, 100), vec![unknown_role])
        .validate()
        .is_err());

    let noncanonical = profile(
        "noncanonical.target",
        target("http://127.0.0.1:4321/v1/", "other"),
        &["critic"],
        alloc(1, 10, 10),
    );
    assert!(configuration(alloc(2, 100, 100), vec![noncanonical])
        .validate()
        .is_err());

    for bad in [alloc(0, 10, 10), alloc(1, 101, 10), alloc(1, 10, 101)] {
        let item = profile("bad.allocation", target(base, "other"), &["critic"], bad);
        assert!(configuration(alloc(2, 100, 100), vec![item])
            .validate()
            .is_err());
    }

    let too_many = (0..33)
        .map(|index| {
            profile(
                &format!("p{index}"),
                target(base, &format!("m{index}")),
                &["solver"],
                alloc(1, 1, 1),
            )
        })
        .collect();
    assert!(configuration(alloc(2, 100, 100), too_many)
        .validate()
        .is_err());

    let oversized = (0..32)
        .map(|index| {
            let path = format!("http://127.0.0.1:4321/{}-{index}", "x".repeat(700));
            profile(
                &format!("p{index}"),
                target(&path, "m"),
                &["solver"],
                alloc(1, 1, 1),
            )
        })
        .collect();
    assert!(configuration(alloc(2, 100, 100), oversized)
        .validate()
        .is_err());
}

#[test]
fn cli_configures_and_roundtrips_profile_catalog_and_legacy_empty_shape() {
    fn cli(home: &std::path::Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(home)
            .arg("--json")
            .args(args)
            .env_clear()
            .output()
            .unwrap()
    }

    let root = test_home("working-profile-cli");
    std::fs::create_dir_all(&root).unwrap();
    let home = root.join("state");
    let input = root.join("resources.json");
    let base = "http://127.0.0.1:4321/v1";
    let resources = configuration(
        alloc(2, 100, 100),
        vec![profile(
            "solver.profile",
            target(base, "solver-model"),
            &["solver"],
            alloc(1, 20, 30),
        )],
    );
    std::fs::write(&input, serde_json::to_vec(&resources).unwrap()).unwrap();
    let configured = cli(&home, &["configure-resources", input.to_str().unwrap()]);
    assert!(
        configured.status.success(),
        "{}",
        String::from_utf8_lossy(&configured.stderr)
    );
    let loaded = Config::load(Some(home.clone()), None).unwrap();
    assert_eq!(
        loaded.resource_admission.unwrap().working_profiles,
        resources.working_profiles
    );
    assert!(cli(&home, &["config-show"]).status.success());

    let empty_home = root.join("empty");
    let empty_input = root.join("empty.json");
    let empty_resources = configuration(alloc(2, 100, 100), Vec::new());
    std::fs::write(&empty_input, serde_json::to_vec(&empty_resources).unwrap()).unwrap();
    let result = cli(
        &empty_home,
        &["configure-resources", empty_input.to_str().unwrap()],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved = std::fs::read(empty_home.join("config.json")).unwrap();
    let value: Value = serde_json::from_slice(&saved).unwrap();
    assert!(value["resource_admission"]
        .get("working_profiles")
        .is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn profiled_engine_selects_each_roles_target_allocation_and_releases_full_ledger() {
    let home = test_home("working-profile-targets");
    let base = "http://127.0.0.1:4321/v1";
    let planner = alloc(1, 40, 70);
    let solver = alloc(1, 70, 40);
    let resources = configuration(
        alloc(2, 100, 100),
        vec![
            profile(
                "planner.profile",
                target(base, "planner-model"),
                &["planner"],
                planner,
            ),
            profile(
                "solver.profile",
                target(base, "solver-model"),
                &["solver"],
                solver,
            ),
        ],
    );
    let mut cfg = base_config(home.clone(), base, resources);
    cfg.role_profiles.insert(
        "planner".into(),
        cogfabric::config::RoleProfile {
            model: "planner-model".into(),
            temperature: 0.0,
        },
    );
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend = ScriptBackend::new(
        local_targets(base),
        [("planner", "invalid plan"), ("solver", "[1,2,3]")],
    );
    let engine = Engine::with_backend(cfg.clone(), store.clone(), backend.clone());
    let pool = engine.configured_resource_pool().unwrap().unwrap();
    let result = engine
        .run_profile(
            r#"sort:{"values":[3,1,2]}"#,
            "deep",
            ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(result.status, "completed");
    let trace = store.task(&result.task_id).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert_profile_chain(events, "planner", &target(base, "planner-model"), planner);
    assert_profile_chain(events, "solver", &target(base, "solver-model"), solver);
    let completed = events
        .iter()
        .filter(|event| event["kind"] == "resource_completed")
        .count();
    let dispatched = events
        .iter()
        .filter(|event| event["kind"] == "resource_dispatch")
        .count();
    assert_eq!(completed, dispatched);
    assert_eq!(
        pool.snapshot().unwrap().working_allocated,
        Allocation::default()
    );
    assert!(pool.snapshot().unwrap().reservations.is_empty());
    let called_targets = backend
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|call| call.target.clone())
        .collect::<Vec<_>>();
    assert!(
        called_targets.contains(&Some(InferenceTarget::LocalHttp(target(
            base,
            "planner-model"
        ))))
    );
    assert!(
        called_targets.contains(&Some(InferenceTarget::LocalHttp(target(
            base,
            "solver-model"
        ))))
    );
    drop(engine);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn heterogeneous_profiles_wait_for_the_shared_pool_and_account_for_exact_allocations() {
    let home = test_home("working-profile-pressure");
    let base = "http://127.0.0.1:4321/v1";
    let solver_a = alloc(1, 70, 40);
    let solver_b = alloc(1, 40, 70);
    let resources = configuration(
        alloc(3, 130, 130),
        vec![
            profile(
                "solver.a.profile",
                target(base, "solver-a"),
                &["solver"],
                solver_a,
            ),
            profile(
                "solver.b.profile",
                target(base, "solver-b"),
                &["solver"],
                solver_b,
            ),
        ],
    );
    let cfg_a = base_config(home.clone(), base, resources.clone());
    let mut cfg_b = cfg_a.clone();
    cfg_b.model = "solver-b".into();
    let store = Arc::new(Store::open(&cfg_a.db()).unwrap());
    let backend_a = ScriptBackend::with_options(
        BTreeMap::from([(
            "solver".into(),
            Some(InferenceTarget::LocalHttp(target(base, "solver-a"))),
        )]),
        [("solver", "[1,2,3]"), ("solver", "[1,2,3]")],
        None,
        true,
    );
    let backend_b = ScriptBackend::with_options(
        BTreeMap::from([(
            "solver".into(),
            Some(InferenceTarget::LocalHttp(target(base, "solver-b"))),
        )]),
        [("solver", "[1,2,3]")],
        None,
        true,
    );
    let first_engine = Engine::with_backend(cfg_a.clone(), store.clone(), backend_a.clone());
    let second_engine = Engine::with_backend(cfg_b, store.clone(), backend_b.clone());
    let third_engine = first_engine.clone();
    let pool = first_engine.configured_resource_pool().unwrap().unwrap();

    let first = tokio::spawn(async move {
        first_engine
            .run_profile(
                r#"sort:{"values":[3,1,2]}"#,
                "fast",
                ExecutionProfile::Collective,
            )
            .await
    });
    backend_a.wait_started(1).await;
    let second = tokio::spawn(async move {
        second_engine
            .run_profile(
                r#"sort:{"values":[3,1,2]}"#,
                "fast",
                ExecutionProfile::Collective,
            )
            .await
    });
    backend_b.wait_started(1).await;
    let concurrent = pool.snapshot().unwrap();
    assert_eq!(concurrent.allocated, alloc(2, 110, 110));
    assert_eq!(concurrent.working_allocated, alloc(2, 110, 110));
    assert_eq!(concurrent.reservations.len(), 2);

    let third = tokio::spawn(async move {
        third_engine
            .run_profile(
                r#"sort:{"values":[3,1,2]}"#,
                "fast",
                ExecutionProfile::Collective,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = pool.snapshot().unwrap();
            if snapshot
                .reservations
                .iter()
                .any(|entry| entry.phase == cogfabric::resource_scheduler::Phase::Waiting)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("third solver profile should wait behind the heterogeneous allocations");

    let snapshot = pool.snapshot().unwrap();
    assert_eq!(snapshot.allocated, alloc(2, 110, 110));
    assert_eq!(snapshot.working_allocated, alloc(2, 110, 110));
    assert_eq!(snapshot.reservations.len(), 3);
    assert_eq!(
        snapshot
            .reservations
            .iter()
            .filter(|entry| entry.phase == cogfabric::resource_scheduler::Phase::Waiting)
            .count(),
        1
    );
    assert_eq!(
        backend_a.dispatch_count() + backend_b.dispatch_count(),
        2,
        "waiting work must not reach the provider"
    );

    backend_a.release.add_permits(1);
    backend_b.release.add_permits(1);
    let first = tokio::time::timeout(Duration::from_secs(3), first)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let second = tokio::time::timeout(Duration::from_secs(3), second)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let third = tokio::time::timeout(Duration::from_secs(3), third)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(first.status, "completed");
    assert_eq!(second.status, "completed");
    assert_eq!(third.status, "completed");
    assert_eq!(backend_a.dispatch_count() + backend_b.dispatch_count(), 3);
    let final_snapshot = pool.snapshot().unwrap();
    assert_eq!(final_snapshot.allocated, Allocation::default());
    assert_eq!(final_snapshot.working_allocated, Allocation::default());
    assert!(final_snapshot.reservations.is_empty());

    let third_events = store.task(&third.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .clone();
    assert!(third_events
        .iter()
        .filter(|event| event["kind"] == "resource_admitted")
        .any(|event| event["payload"]["wait_ms"].as_f64().unwrap() > 0.0));

    for result in [&first, &second, &third] {
        let trace = store.task(&result.task_id).unwrap();
        let events = trace["events"].as_array().unwrap();
        for event in events
            .iter()
            .filter(|event| event["kind"] == "resource_intent")
        {
            let reservation = event["payload"]["reservation_id"].as_str().unwrap();
            let decision = events
                .iter()
                .find(|candidate| {
                    candidate["kind"] == "resource_profile"
                        && candidate["payload"]["reservation_id"] == reservation
                })
                .unwrap();
            let expected = match decision["payload"]["profile_id"].as_str().unwrap() {
                "solver.a.profile" => solver_a,
                "solver.b.profile" => solver_b,
                other => panic!("unexpected profile id {other}"),
            };
            assert_eq!(event["payload"]["allocation"], json!(expected));
        }
        assert_eq!(
            events
                .iter()
                .filter(|event| event["kind"] == "resource_dispatch")
                .count(),
            events
                .iter()
                .filter(|event| event["kind"] == "resource_completed")
                .count()
        );
    }
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn unknown_deterministic_and_unlisted_targets_are_excluded_before_backend_dispatch() {
    let base = "http://127.0.0.1:4321/v1";
    for (case, target_value, expected_reason) in [
        ("unknown", None, "unknown_target"),
        ("metadata-error", None, "metadata_error"),
        (
            "deterministic",
            Some(InferenceTarget::Deterministic),
            "unlisted_profile",
        ),
        (
            "unlisted",
            Some(InferenceTarget::LocalHttp(target(base, "other-model"))),
            "unlisted_profile",
        ),
    ] {
        let home = test_home(&format!("working-profile-{case}"));
        let resources = configuration(
            alloc(1, 100, 100),
            vec![profile(
                "solver.profile",
                target(base, "solver-model"),
                &["solver"],
                alloc(1, 20, 30),
            )],
        );
        let cfg = base_config(home.clone(), base, resources);
        let backend = ScriptBackend::new(
            BTreeMap::from([("solver".into(), target_value)]),
            [("solver", "[1,2,3]")],
        );
        if case == "metadata-error" {
            *backend.metadata_error.lock().unwrap() =
                Some("SENTINEL_PROVIDER_METADATA_SECRET".into());
        }
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        let engine = Engine::with_backend(cfg.clone(), store.clone(), backend.clone());
        let result = engine
            .run_profile(
                r#"sort:{"values":[3,1,2]}"#,
                "fast",
                ExecutionProfile::Collective,
            )
            .await;
        assert!(
            result.is_err()
                || result
                    .as_ref()
                    .is_ok_and(|outcome| outcome.status != "completed"),
            "{case} target unexpectedly passed"
        );
        assert_eq!(
            backend.dispatch_count(),
            0,
            "{case} target reached provider"
        );
        if let Ok(outcome) = &result {
            assert_eq!(
                outcome.calls, 1,
                "{case} must retain the attempted-call budget"
            );
        }
        let task_ids: Vec<String> = {
            let db = rusqlite::Connection::open(cfg.db()).unwrap();
            let mut statement = db.prepare("SELECT id FROM tasks ORDER BY created").unwrap();
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .map(|row| row.unwrap())
                .collect()
        };
        assert_eq!(task_ids.len(), 1);
        let events = store.task(&task_ids[0]).unwrap()["events"]
            .as_array()
            .unwrap()
            .clone();
        let selection = events
            .iter()
            .find(|event| event["kind"] == "routing_resource_selection")
            .expect("initial resource routing selection should be audited");
        assert_eq!(selection["payload"]["eligible"], false);
        assert!(selection["payload"]["routes"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(selection["payload"]["excluded_candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| candidate["reason_code"] == expected_reason));
        assert_eq!(
            events
                .iter()
                .filter(|event| event["kind"] == "model_dispatch")
                .count(),
            1,
            "the attempted model call remains charged before the empty route fails"
        );
        if case == "metadata-error" {
            assert!(!serde_json::to_string(&events)
                .unwrap()
                .contains("SENTINEL_PROVIDER_METADATA_SECRET"));
        }
        assert!(!events
            .iter()
            .any(|event| event["kind"] == "resource_dispatch"));
        assert!(!events
            .iter()
            .any(|event| event["kind"] == "resource_admitted"));
        drop(engine);
        drop(store);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[tokio::test]
async fn profile_intent_audit_failure_prevents_target_lookup_and_provider_dispatch() {
    let home = test_home("working-profile-audit-failure");
    let base = "http://127.0.0.1:4321/v1";
    let resources = configuration(
        alloc(1, 100, 100),
        vec![profile(
            "solver.profile",
            target(base, "solver-model"),
            &["solver"],
            alloc(1, 20, 30),
        )],
    );
    let cfg = base_config(home.clone(), base, resources);
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let database = cfg.db();
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute_batch("CREATE TRIGGER refuse_profile_intent BEFORE INSERT ON events WHEN NEW.kind='resource_profile_intent' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")
        .unwrap();
    let backend = ScriptBackend::new(local_targets(base), [("solver", "[1,2,3]")]);
    let engine = Engine::with_backend(cfg, store.clone(), backend.clone());
    // Exercise the per-call resource admission wrapper directly. Engine routing
    // now inspects candidate metadata after its separate routing_resource_intent;
    // this assertion concerns the later resource_profile_intent that must precede
    // the per-call target lookup.
    let task = store
        .create_task("direct profiled admission fixture", "fast")
        .unwrap();
    let pool = engine.configured_resource_pool().unwrap().unwrap();
    let scheduled = pool
        .wrap_backend("human-cli", backend.clone(), alloc(1, 1, 1))
        .unwrap();
    let result = scheduled
        .generate_for(&task, "solver", "bounded fixture prompt", 8)
        .await;
    assert!(
        result.is_err(),
        "the required profile intent audit should fail"
    );
    assert_eq!(
        backend.metadata_reads.load(Ordering::SeqCst),
        0,
        "target metadata was inspected before its required intent audit"
    );
    assert_eq!(backend.dispatch_count(), 0);
    let db = rusqlite::Connection::open(&database).unwrap();
    let target_events: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE task_id=?1 AND kind='resource_profile'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    let dispatch_events: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE task_id=?1 AND kind='resource_dispatch'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(target_events, 0);
    assert_eq!(dispatch_events, 0);
    drop(db);
    drop(engine);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn cancelled_profiled_dispatch_quarantines_its_exact_allocation() {
    let home = test_home("working-profile-quarantine");
    let base = "http://127.0.0.1:4321/v1";
    let selected = alloc(1, 20, 30);
    let resources = configuration(
        alloc(1, 100, 100),
        vec![profile(
            "solver.profile",
            target(base, "solver-model"),
            &["solver"],
            selected,
        )],
    );
    let cfg = base_config(home.clone(), base, resources);
    let database = cfg.db();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend =
        ScriptBackend::with_options(local_targets(base), [("solver", "[1,2,3]")], None, true);
    let engine = Engine::with_backend(cfg, store.clone(), backend.clone());
    let pool = engine.configured_resource_pool().unwrap().unwrap();
    let (cancel_sender, cancel_receiver) = tokio::sync::oneshot::channel::<()>();
    let run = tokio::spawn(async move {
        engine
            .run_until(r#"sort:{"values":[3,1,2]}"#, "fast", async move {
                let _ = cancel_receiver.await;
            })
            .await
    });
    backend.wait_started(1).await;
    let dispatched = pool.snapshot().unwrap();
    assert_eq!(dispatched.allocated, selected);
    assert_eq!(dispatched.reservations.len(), 1);
    let reservation = dispatched.reservations[0].reservation_id.clone();
    cancel_sender.send(()).unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(3), run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(outcome.status, "cancelled");
    let quarantined = pool.snapshot().unwrap();
    assert_eq!(quarantined.reservations.len(), 1);
    assert_eq!(quarantined.reservations[0].reservation_id, reservation);
    assert_eq!(
        quarantined.reservations[0].phase,
        cogfabric::resource_scheduler::Phase::Quarantined
    );
    assert_eq!(quarantined.reservations[0].allocation, selected);
    assert_eq!(quarantined.allocated, selected);
    assert!(pool
        .acknowledge_stopped("human-cli", &reservation, true)
        .is_ok());
    assert_eq!(pool.snapshot().unwrap().allocated, Allocation::default());
    let trace_events: i64 = rusqlite::Connection::open(database)
        .unwrap()
        .query_row(
            "SELECT count(*) FROM events WHERE kind='resource_completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(trace_events, 0);
    drop(pool);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn queued_profile_keeps_its_pinned_module_when_registry_replaces_the_route() {
    let home = test_home("working-profile-pin");
    let base = "http://127.0.0.1:4321/v1";
    let old_target = target(base, "solver-model");
    let resources = configuration(
        alloc(1, 100, 100),
        vec![profile(
            "solver.profile",
            old_target.clone(),
            &["solver"],
            alloc(1, 20, 30),
        )],
    );
    let cfg = base_config(home.clone(), base, resources);
    let descriptor_v1 = ModuleDescriptor {
        id: "profile.route".into(),
        version: "1.0.0".into(),
        roles: vec!["solver".into()],
    };
    let descriptor_v2 = ModuleDescriptor {
        id: "profile.route".into(),
        version: "2.0.0".into(),
        roles: vec!["solver".into()],
    };
    let old = ScriptBackend::with_options(
        BTreeMap::from([(
            "solver".into(),
            Some(InferenceTarget::LocalHttp(old_target.clone())),
        )]),
        [("solver", "[1,2,3]"), ("solver", "[1,2,3]")],
        Some(descriptor_v1),
        true,
    );
    let new = ScriptBackend::new(
        BTreeMap::from([(
            "solver".into(),
            Some(InferenceTarget::LocalHttp(target(
                base,
                "replacement-model",
            ))),
        )]),
        [("solver", "[1,2,3]")],
    );
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let engine = Engine::with_backend(cfg.clone(), store.clone(), old.clone());
    let pool = engine.configured_resource_pool().unwrap().unwrap();
    let holder_engine = engine.clone();
    let holder = tokio::spawn(async move {
        holder_engine
            .run_profile(
                r#"sort:{"values":[3,1,2]}"#,
                "fast",
                ExecutionProfile::Collective,
            )
            .await
    });
    old.wait_started(1).await;
    let queued_engine = engine.clone();
    let queued = tokio::spawn(async move {
        queued_engine
            .run_profile(
                r#"sort:{"values":[3,1,2]}"#,
                "fast",
                ExecutionProfile::Collective,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if pool
                .snapshot()
                .unwrap()
                .reservations
                .iter()
                .any(|entry| entry.phase == cogfabric::resource_scheduler::Phase::Waiting)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("second task should queue with its first route pinned");
    engine
        .broker
        .register_candidate(descriptor_v2, new.clone(), 1)
        .unwrap();
    old.release.add_permits(1);
    let holder = tokio::time::timeout(Duration::from_secs(3), holder)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let queued = tokio::time::timeout(Duration::from_secs(3), queued)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(holder.status, "completed");
    assert_eq!(queued.status, "completed");
    assert_eq!(old.dispatch_count(), 2);
    assert_eq!(
        new.dispatch_count(),
        0,
        "queued request was redirected to an unchecked replacement"
    );
    for result in [holder, queued] {
        let events = store.task(&result.task_id).unwrap()["events"]
            .as_array()
            .unwrap()
            .clone();
        assert_profile_chain(&events, "solver", &old_target, alloc(1, 20, 30));
    }
    assert_eq!(pool.snapshot().unwrap().allocated, Allocation::default());
    drop(engine);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn empty_profile_table_keeps_legacy_fixed_allocation_and_skips_target_callbacks() {
    let home = test_home("working-profile-empty-runtime");
    let base = "http://127.0.0.1:4321/v1";
    let fallback = alloc(1, 30, 40);
    let mut resources = configuration(alloc(2, 100, 100), Vec::new());
    resources.allocation = fallback;
    let cfg = base_config(home.clone(), base, resources);
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend = ScriptBackend::new(
        BTreeMap::from([("solver".into(), None)]),
        [("solver", "[1,2,3]")],
    );
    let engine = Engine::with_backend(cfg, store.clone(), backend.clone());
    let result = engine
        .run_profile(
            r#"sort:{"values":[3,1,2]}"#,
            "fast",
            ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(backend.metadata_reads.load(Ordering::SeqCst), 0);
    let events = store.task(&result.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!events
        .iter()
        .any(|event| event["kind"] == "resource_profile_intent"
            || event["kind"] == "resource_profile"));
    let intent = events
        .iter()
        .find(|event| event["kind"] == "resource_intent")
        .unwrap();
    assert_eq!(intent["payload"]["allocation"], json!(fallback));
    assert_eq!(
        engine
            .configured_resource_pool()
            .unwrap()
            .unwrap()
            .snapshot()
            .unwrap()
            .working_allocated,
        Allocation::default()
    );
    drop(engine);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
