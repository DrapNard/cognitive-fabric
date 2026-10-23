//! Engine-level tests for resource-aware module route selection.
use anyhow::{bail, Result};
use cogfabric::{
    backend::{Generation, InferenceBackend, InferenceTarget, ModelTarget},
    config::Config,
    engine::{Engine, ExecutionProfile},
    fabric::{Availability, ModuleDescriptor},
    resource_scheduler::{Allocation, Configuration, ResidentAllocation, Settings, WorkingProfile},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};

const BASE: &str = "http://127.0.0.1:4321/v1";
const QUERY: &str = r#"sort:{"values":[3,1,2]}"#;

fn target(model: &str) -> ModelTarget {
    ModelTarget {
        api_base: BASE.into(),
        model: model.into(),
    }
}

fn descriptor(id: &str, roles: &[&str]) -> ModuleDescriptor {
    ModuleDescriptor {
        id: id.into(),
        version: "1.0.0".into(),
        roles: roles.iter().map(|role| (*role).into()).collect(),
    }
}

fn allocation(compute_slots: u32, host_memory_bytes: u64, device_memory_bytes: u64) -> Allocation {
    Allocation {
        compute_slots,
        host_memory_bytes,
        device_memory_bytes,
    }
}

fn working_profile(id: &str, role: &str, model: &str, allocation: Allocation) -> WorkingProfile {
    WorkingProfile {
        id: id.into(),
        target: target(model),
        roles: vec![role.into()],
        allocation,
    }
}

fn resources(profiles: Vec<WorkingProfile>) -> Configuration {
    resources_with_residents(profiles, vec![])
}

fn resources_with_residents(
    working_profiles: Vec<WorkingProfile>,
    residents: Vec<ResidentAllocation>,
) -> Configuration {
    Configuration {
        settings: Settings {
            label: "resource.route.fixture".into(),
            capacity: allocation(8, 32_000, 32_000),
            max_pending: 8,
            max_wait_ms: 3_000,
            max_execution_ms: 3_000,
            scheduling: None,
        },
        allocation: allocation(1, 128, 128),
        residents,
        working_profiles,
    }
}

fn config(home: std::path::PathBuf, resource_admission: Option<Configuration>) -> Config {
    Config {
        home,
        backend: "openai-local".into(),
        api_base: BASE.into(),
        model: "solver-model".into(),
        interpret_goals: false,
        max_calls: 8,
        max_wall_seconds: 10,
        resource_admission,
        ..Config::default()
    }
}

fn home(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cogfabric-route-{label}-{}", uuid::Uuid::new_v4()))
}

struct Call {
    role: String,
    model: Option<String>,
}

type Callback = Box<dyn FnOnce() + Send + 'static>;

struct Backend {
    descriptor: ModuleDescriptor,
    targets: BTreeMap<String, Result<Option<InferenceTarget>, &'static str>>,
    calls: Arc<Mutex<Vec<Call>>>,
    metadata_reads: Arc<AtomicUsize>,
    action: Mutex<Option<Callback>>,
    fail_generation: bool,
    answer: Mutex<VecDeque<String>>,
    started: Option<Arc<Semaphore>>,
    release: Option<Arc<Semaphore>>,
    changed: Option<Arc<Notify>>,
}

impl Backend {
    fn new(
        descriptor: ModuleDescriptor,
        targets: impl IntoIterator<Item = (String, Result<Option<InferenceTarget>, &'static str>)>,
        answer: &str,
    ) -> Arc<Self> {
        Arc::new(Self {
            descriptor,
            targets: targets.into_iter().collect(),
            calls: Arc::new(Mutex::new(Vec::new())),
            metadata_reads: Arc::new(AtomicUsize::new(0)),
            action: Mutex::new(None),
            fail_generation: false,
            answer: Mutex::new(VecDeque::from([answer.into()])),
            started: None,
            release: None,
            changed: None,
        })
    }

    fn on_metadata(&self, callback: impl FnOnce() + Send + 'static) {
        *self.action.lock().unwrap() = Some(Box::new(callback));
    }

    fn fail_generation(&mut self) {
        self.fail_generation = true;
    }

    fn generations(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn metadata_count(&self) -> usize {
        self.metadata_reads.load(Ordering::SeqCst)
    }

    fn generated_models(&self) -> Vec<Option<String>> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| {
                let _role = &call.role;
                call.model.clone()
            })
            .collect()
    }
}

impl InferenceBackend for Backend {
    fn inference_target(&self, role: &str) -> Result<Option<InferenceTarget>> {
        self.metadata_reads.fetch_add(1, Ordering::SeqCst);
        if let Some(action) = self.action.lock().unwrap().take() {
            action();
        }
        match self.targets.get(role) {
            Some(Ok(target)) => Ok(target.clone()),
            Some(Err(message)) => bail!("{message}"),
            None => Ok(None),
        }
    }

    fn availability(&self) -> Availability {
        Availability::Connected
    }

    fn module_descriptor(&self) -> Option<&ModuleDescriptor> {
        Some(&self.descriptor)
    }

    fn generate<'a>(
        &'a self,
        role: &'a str,
        _prompt: &'a str,
        _max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let model = match self.targets.get(role) {
            Some(Ok(Some(InferenceTarget::LocalHttp(model)))) => Some(model.model.clone()),
            _ => None,
        };
        let answer = self
            .answer
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| "[1,2,3]".into());
        let calls = self.calls.clone();
        let started = self.started.clone();
        let release = self.release.clone();
        let changed = self.changed.clone();
        let fail_generation = self.fail_generation;
        Box::pin(async move {
            calls.lock().unwrap().push(Call {
                role: role.into(),
                model: model.clone(),
            });
            if let Some(started) = started {
                started.add_permits(1);
            }
            if let Some(changed) = changed {
                changed.notify_waiters();
            }
            if let Some(release) = release {
                release.acquire().await?.forget();
            }
            if fail_generation {
                bail!("fixture selected backend failed after dispatch")
            }
            Ok(Generation {
                text: answer,
                model: model.unwrap_or_else(|| "fixture-model".into()),
                input_tokens: Some(1),
                output_tokens: Some(2),
                elapsed_ms: 0.0,
                queue_wait_ms: None,
            })
        })
    }
}

fn base_backend() -> Arc<Backend> {
    Backend::new(
        descriptor("fixture.base", &["interface"]),
        [("interface".into(), Ok(None))],
        "unused",
    )
}

fn add_candidate(engine: &Engine, backend: Arc<Backend>, id: &str, rank: u32) {
    let mut module_descriptor = backend.descriptor.clone();
    module_descriptor.id = id.into();
    let backend_descriptor = backend.descriptor.clone();
    assert_eq!(module_descriptor.roles, backend_descriptor.roles);
    // The test backend binds a stable module identity. Construct another backend
    // with the broker identity before registration when IDs differ.
    let backend = if backend_descriptor.id == module_descriptor.id {
        backend
    } else {
        Arc::new(Backend {
            descriptor: module_descriptor.clone(),
            targets: backend.targets.clone(),
            calls: backend.calls.clone(),
            metadata_reads: backend.metadata_reads.clone(),
            action: Mutex::new(None),
            fail_generation: backend.fail_generation,
            answer: Mutex::new(backend.answer.lock().unwrap().clone()),
            started: backend.started.clone(),
            release: backend.release.clone(),
            changed: backend.changed.clone(),
        })
    };
    engine
        .broker
        .register_candidate(module_descriptor, backend, rank)
        .unwrap();
}

fn engine_with_resource_routes(
    resource_admission: Configuration,
) -> (Engine, Arc<Store>, Config, std::path::PathBuf) {
    let home = home("selection");
    let cfg = config(home.clone(), Some(resource_admission));
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let engine = Engine::with_backend(cfg.clone(), store.clone(), base_backend());
    (engine, store, cfg, home)
}

fn events(trace: &Value, kind: &str) -> Vec<Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == kind)
        .cloned()
        .collect()
}

fn latest_task_id(cfg: &Config) -> String {
    rusqlite::Connection::open(cfg.db())
        .unwrap()
        .query_row(
            "SELECT id FROM tasks ORDER BY created DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn assert_failed_task_is_not_orphaned(store: &Store, task: &str) {
    assert_eq!(store.task(task).unwrap()["task"]["status"], "failed");
}

fn cleanup(engine: Engine, store: Arc<Store>, home: std::path::PathBuf) {
    drop(engine);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn ranked_unlisted_candidate_is_skipped_and_eligible_lower_rank_runs_under_original_budget() {
    let allocation = allocation(2, 700, 900);
    let (engine, store, _, home) = engine_with_resource_routes(resources(vec![working_profile(
        "solver.good",
        "solver",
        "good-model",
        allocation,
    )]));
    let bad = Backend::new(
        descriptor("fixture.bad", &["solver"]),
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("unlisted-model")))),
        )],
        "[9,9,9]",
    );
    let good = Backend::new(
        descriptor("fixture.good", &["solver"]),
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("good-model")))),
        )],
        "[1,2,3]",
    );
    add_candidate(&engine, bad.clone(), "fixture.bad", 1);
    add_candidate(&engine, good.clone(), "fixture.good", 2);

    let result = engine
        .run_profile(QUERY, "fast", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(
        result.calls, 1,
        "route selection must not add inference calls"
    );
    assert_eq!(result.resources.reserved_output_tokens, 384);
    assert_eq!(bad.generations(), 0);
    assert_eq!(good.generated_models(), vec![Some("good-model".into())]);

    let trace = store.task(&result.task_id).unwrap();
    let intents = events(&trace, "routing_resource_intent");
    let selections = events(&trace, "routing_resource_selection");
    assert_eq!(intents.len(), 1);
    assert_eq!(selections.len(), 1);
    let selection = &selections[0]["payload"];
    assert_eq!(selection["eligible"], true);
    assert_eq!(selection["routes"].as_array().unwrap().len(), 1);
    assert_eq!(selection["routes"][0]["role"], "solver");
    assert_eq!(selection["routes"][0]["module_id"], "fixture.good");
    assert_eq!(
        selection["routes"][0]["resource"]["profile_id"],
        "solver.good"
    );
    assert!(selection["excluded_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| {
            item["module_id"] == "fixture.bad" && item["reason_code"] == "unlisted_profile"
        }));
    let snapshot = events(&trace, "module_snapshot").remove(0);
    assert_eq!(snapshot["payload"]["resource_routes"], selection.clone());
    assert_eq!(snapshot["payload"]["resource_routing_version"], 1);
    let dispatches = events(&trace, "resource_dispatch");
    let admitted = events(&trace, "resource_admitted");
    assert_eq!(dispatches.len(), 1);
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0]["payload"]["allocation"], json!(allocation));
    assert!(!selection["permissions_granted"].as_bool().unwrap());
    assert!(!selection["weights_attested"].as_bool().unwrap());
    assert!(!selection["physical_limits_enforced"].as_bool().unwrap());
    assert_eq!(selection["route_scope_sha256"].as_str().unwrap().len(), 64);
    cleanup(engine, store, home);
}

#[tokio::test]
async fn role_map_can_pin_distinct_eligible_modules_for_planner_and_solver() {
    let allocation = allocation(1, 600, 800);
    let (engine, store, _, home) = engine_with_resource_routes(resources(vec![
        working_profile("planner.profile", "planner", "planner-model", allocation),
        working_profile("solver.profile", "solver", "solver-model", allocation),
    ]));
    let planner = Backend::new(
        descriptor("fixture.planner", &["planner"]),
        [(
            "planner".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("planner-model")))),
        )],
        r#"{"steps":[{"id":"sort","objective":"sort values","depends_on":[]}]}"#,
    );
    let solver = Backend::new(
        descriptor("fixture.solver", &["solver"]),
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
        )],
        "[1,2,3]",
    );
    add_candidate(&engine, planner.clone(), "fixture.planner", 4);
    add_candidate(&engine, solver.clone(), "fixture.solver", 3);

    let result = engine
        .run_profile(QUERY, "standard", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(result.status, "completed");
    let selection = &events(
        &store.task(&result.task_id).unwrap(),
        "routing_resource_selection",
    )[0]["payload"];
    let routes = selection["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0]["role"], "planner");
    assert_eq!(routes[0]["module_id"], "fixture.planner");
    assert_eq!(routes[1]["role"], "solver");
    assert_eq!(routes[1]["module_id"], "fixture.solver");
    assert_eq!(planner.generations(), 1);
    assert!(solver.generations() >= 1);
    cleanup(engine, store, home);
}

#[tokio::test]
async fn unknown_deterministic_and_metadata_error_candidates_are_redacted_and_fall_through() {
    let allocation = allocation(1, 500, 600);
    for (label, value, reason) in [
        ("unknown", Ok(None), "unknown_target"),
        (
            "deterministic",
            Ok(Some(InferenceTarget::Deterministic)),
            "unlisted_profile",
        ),
        (
            "metadata-error",
            Err("SENTINEL_ROUTE_METADATA_SECRET"),
            "metadata_error",
        ),
    ] {
        let (engine, store, _, home) =
            engine_with_resource_routes(resources(vec![working_profile(
                "solver.good",
                "solver",
                "good-model",
                allocation,
            )]));
        let bad = Backend::new(
            descriptor("fixture.bad", &["solver"]),
            [("solver".into(), value)],
            "[9,9,9]",
        );
        let good = Backend::new(
            descriptor("fixture.good", &["solver"]),
            [(
                "solver".into(),
                Ok(Some(InferenceTarget::LocalHttp(target("good-model")))),
            )],
            "[1,2,3]",
        );
        add_candidate(&engine, bad.clone(), "fixture.bad", 1);
        add_candidate(&engine, good.clone(), "fixture.good", 2);
        let result = engine
            .run_profile(QUERY, "fast", ExecutionProfile::Collective)
            .await
            .unwrap();
        assert_eq!(result.status, "completed", "{label}");
        assert_eq!(bad.generations(), 0, "{label}");
        assert_eq!(good.generations(), 1, "{label}");
        let trace = store.task(&result.task_id).unwrap();
        let serialized = serde_json::to_string(&trace).unwrap();
        assert!(!serialized.contains("SENTINEL_ROUTE_METADATA_SECRET"));
        let selection = &events(&trace, "routing_resource_selection")[0]["payload"];
        let all_events = trace["events"].as_array().unwrap();
        let intent_index = all_events
            .iter()
            .position(|item| item["kind"] == "routing_resource_intent")
            .unwrap();
        let selection_index = all_events
            .iter()
            .position(|item| item["kind"] == "routing_resource_selection")
            .unwrap();
        assert!(intent_index < selection_index);
        assert!(selection["excluded_candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["module_id"] == "fixture.bad" && item["reason_code"] == reason }));
        cleanup(engine, store, home);
    }
}

#[tokio::test]
async fn resident_miss_is_skipped_in_favor_of_a_declared_consumer() {
    let resident = ResidentAllocation {
        id: "shared.backbone".into(),
        version: "1.0.0".into(),
        weights_sha256: "a".repeat(64),
        host_memory_bytes: 100,
        device_memory_bytes: 100,
        consumers: vec![target("resident-model")],
    };
    let (engine, store, _, home) =
        engine_with_resource_routes(resources_with_residents(vec![], vec![resident]));
    let miss = Backend::new(
        descriptor("fixture.miss", &["solver"]),
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("other-model")))),
        )],
        "[9,9,9]",
    );
    let match_ = Backend::new(
        descriptor("fixture.match", &["solver"]),
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("resident-model")))),
        )],
        "[1,2,3]",
    );
    add_candidate(&engine, miss.clone(), "fixture.miss", 1);
    add_candidate(&engine, match_.clone(), "fixture.match", 2);
    let result = engine
        .run_profile(QUERY, "fast", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(miss.generations(), 0);
    assert_eq!(match_.generations(), 1);
    let trace = store.task(&result.task_id).unwrap();
    let selection = &events(&trace, "routing_resource_selection")[0]["payload"];
    assert!(selection["excluded_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| {
            item["module_id"] == "fixture.miss" && item["reason_code"] == "resident_miss"
        }));
    assert_eq!(
        selection["routes"][0]["resource"]["resident_coverage"]["declared_consumer_matched"],
        true
    );
    cleanup(engine, store, home);
}

#[tokio::test]
async fn all_ineligible_candidates_fail_model_tasks_without_orphaning_but_inline_math_still_works()
{
    let resources = resources(vec![working_profile(
        "solver.allowed",
        "solver",
        "allowed-model",
        allocation(1, 500, 600),
    )]);
    let (engine, store, cfg, home) = engine_with_resource_routes(resources);
    let bad = Backend::new(
        descriptor("fixture.bad", &["solver"]),
        [("solver".into(), Ok(None))],
        "[1,2,3]",
    );
    add_candidate(&engine, bad.clone(), "fixture.bad", 1);
    let failure = engine
        .run_profile(QUERY, "fast", ExecutionProfile::Collective)
        .await;
    assert!(
        failure.is_err()
            || failure
                .as_ref()
                .is_ok_and(|outcome| outcome.status != "completed")
    );
    assert_eq!(bad.generations(), 0);
    let task = latest_task_id(&cfg);
    assert_failed_task_is_not_orphaned(&store, &task);
    let trace = store.task(&task).unwrap();
    let selection = &events(&trace, "routing_resource_selection")[0]["payload"];
    assert_eq!(selection["eligible"], false);
    assert!(selection["routes"].as_array().unwrap().is_empty());
    assert_eq!(events(&trace, "model_dispatch").len(), 1);
    assert!(events(&trace, "resource_dispatch").is_empty());

    let math = engine
        .run_profile("calc: 2 + 2", "fast", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(math.status, "completed");
    assert_eq!(math.answer, "4");
    assert_eq!(math.calls, 0);
    let math_trace = store.task(&math.task_id).unwrap();
    assert_eq!(
        events(&math_trace, "routing_resource_selection")[0]["payload"]["eligible"],
        false
    );
    assert!(events(&math_trace, "model_dispatch").is_empty());
    cleanup(engine, store, home);
}

#[tokio::test]
async fn intent_and_selection_audit_failures_prevent_provider_generation() {
    for (kind, callback_expected) in [
        ("routing_resource_intent", false),
        ("routing_resource_selection", true),
    ] {
        let (engine, store, cfg, home) =
            engine_with_resource_routes(resources(vec![working_profile(
                "solver.profile",
                "solver",
                "solver-model",
                allocation(1, 500, 600),
            )]));
        rusqlite::Connection::open(cfg.db())
            .unwrap()
            .execute_batch(&format!(
                "CREATE TRIGGER reject_route_audit BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'route audit refused'); END;"
            ))
            .unwrap();
        let backend = Backend::new(
            descriptor("fixture.candidate", &["solver"]),
            [(
                "solver".into(),
                Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
            )],
            "[1,2,3]",
        );
        add_candidate(&engine, backend.clone(), "fixture.candidate", 1);
        let result = engine
            .run_profile(QUERY, "fast", ExecutionProfile::Collective)
            .await;
        assert!(
            result.is_err()
                || result
                    .as_ref()
                    .is_ok_and(|outcome| outcome.status != "completed")
        );
        assert_eq!(backend.generations(), 0);
        assert_eq!(backend.metadata_count() > 0, callback_expected);
        let task = latest_task_id(&cfg);
        assert_failed_task_is_not_orphaned(&store, &task);
        cleanup(engine, store, home);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eligibility_callbacks_can_reenter_registry_and_store_without_locks_held() {
    let (engine, store, _, home) = engine_with_resource_routes(resources(vec![working_profile(
        "solver.profile",
        "solver",
        "solver-model",
        allocation(1, 500, 600),
    )]));
    let backend = Backend::new(
        descriptor("fixture.candidate", &["solver"]),
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
        )],
        "[1,2,3]",
    );
    add_candidate(&engine, backend.clone(), "fixture.candidate", 1);
    let broker = Arc::downgrade(&engine.broker);
    let store_for_callback = store.clone();
    backend.on_metadata(move || {
        let broker = broker.upgrade().unwrap();
        assert!(broker
            .discover()
            .unwrap()
            .iter()
            .any(|item| item.id == "fixture.candidate"));
        assert!(store_for_callback.skills().unwrap().is_empty());
    });
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        engine.run_profile(QUERY, "fast", ExecutionProfile::Collective),
    )
    .await
    .expect("registry and Store must be unlocked while callbacks run")
    .unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(backend.metadata_count(), 1);
    cleanup(engine, store, home);
}

#[tokio::test]
async fn registry_replacement_or_shutdown_inside_metadata_callback_fails_before_publication() {
    for mutate_shutdown in [false, true] {
        let (engine, store, cfg, home) =
            engine_with_resource_routes(resources(vec![working_profile(
                "solver.profile",
                "solver",
                "solver-model",
                allocation(1, 500, 600),
            )]));
        let old = Backend::new(
            descriptor("fixture.mutable", &["solver"]),
            [(
                "solver".into(),
                Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
            )],
            "[1,2,3]",
        );
        add_candidate(&engine, old.clone(), "fixture.mutable", 1);
        let broker = Arc::downgrade(&engine.broker);
        let replacement = Backend::new(
            ModuleDescriptor {
                id: "fixture.mutable".into(),
                version: "2.0.0".into(),
                roles: vec!["solver".into()],
            },
            [(
                "solver".into(),
                Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
            )],
            "[2,3,4]",
        );
        let replacement_for_callback = replacement.clone();
        old.on_metadata(move || {
            let broker = broker.upgrade().unwrap();
            if mutate_shutdown {
                broker.shutdown().unwrap();
            } else {
                broker
                    .register_candidate(
                        replacement_for_callback.descriptor.clone(),
                        replacement_for_callback,
                        1,
                    )
                    .unwrap();
            }
        });
        let result = engine
            .run_profile(QUERY, "fast", ExecutionProfile::Collective)
            .await;
        assert!(result.is_err(), "stale route should be rejected");
        assert_eq!(old.generations(), 0);
        assert_eq!(replacement.generations(), 0);
        let task = latest_task_id(&cfg);
        assert_failed_task_is_not_orphaned(&store, &task);
        let trace = store.task(&task).unwrap();
        assert_eq!(events(&trace, "routing_resource_intent").len(), 1);
        assert!(events(&trace, "routing_resource_selection").is_empty());
        cleanup(engine, store, home);
    }
}

#[tokio::test]
async fn active_task_keeps_original_role_map_after_registry_replacement() {
    let selected = allocation(1, 500, 600);
    let home = home("active-route");
    let mut resource_config = resources(vec![
        working_profile("planner.profile", "planner", "solver-model", selected),
        working_profile("solver.profile", "solver", "solver-model", selected),
    ]);
    resource_config.settings.capacity = selected;
    let mut cfg = config(home.clone(), Some(resource_config));
    cfg.subtask_parallelism = Some(2);
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let engine = Engine::with_backend(cfg, store.clone(), base_backend());
    let started = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let changed = Arc::new(Notify::new());
    let mut pinned = Backend::new(
        descriptor("fixture.stable", &["planner", "solver"]),
        ["planner", "solver"].into_iter().map(|role| {
            (
                role.into(),
                Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
            )
        }),
        r#"{"steps":[{"id":"a","objective":"Subgoal A","depends_on":[]},{"id":"b","objective":"Subgoal B","depends_on":[]}] }"#,
    );
    Arc::get_mut(&mut pinned).unwrap().started = Some(started.clone());
    Arc::get_mut(&mut pinned).unwrap().release = Some(release.clone());
    Arc::get_mut(&mut pinned).unwrap().changed = Some(changed);
    let first = pinned;
    engine
        .broker
        .register_candidate(first.descriptor.clone(), first.clone(), 1)
        .unwrap();
    let first_engine = engine.clone();
    let first_task = tokio::spawn(async move {
        first_engine
            .run_profile(QUERY, "standard", ExecutionProfile::Collective)
            .await
    });
    started.acquire().await.unwrap().forget();
    release.add_permits(1);
    tokio::time::timeout(Duration::from_secs(3), async {
        started.acquire().await.unwrap().forget();
        loop {
            let pool = engine.configured_resource_pool().unwrap().unwrap();
            if pool
                .snapshot()
                .unwrap()
                .reservations
                .iter()
                .any(|reservation| {
                    reservation.phase == cogfabric::resource_scheduler::Phase::Waiting
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("second parallel solver call should wait for the shared allocation");

    let mut replacement_descriptor = descriptor("fixture.stable", &["planner", "solver"]);
    replacement_descriptor.version = "2.0.0".into();
    let replacement = Backend::new(
        replacement_descriptor,
        ["planner", "solver"].into_iter().map(|role| {
            (
                role.into(),
                Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
            )
        }),
        "[9,9,9]",
    );
    engine
        .broker
        .register_candidate(replacement.descriptor.clone(), replacement.clone(), 1)
        .unwrap();
    release.add_permits(8);
    let result = tokio::time::timeout(Duration::from_secs(3), first_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.status, "completed");
    assert!(first.generations() >= 4);
    assert_eq!(replacement.generations(), 0);
    let trace = store.task(&result.task_id).unwrap();
    let selection = &events(&trace, "routing_resource_selection")[0]["payload"];
    assert_eq!(selection["routes"].as_array().unwrap().len(), 2);
    assert!(selection["routes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|route| route["module_version"] == "1.0.0"));
    cleanup(engine, store, home);
}

#[tokio::test]
async fn selected_backend_failure_does_not_fall_back_to_lower_rank_candidate() {
    let (engine, store, _, home) = engine_with_resource_routes(resources(vec![working_profile(
        "solver.profile",
        "solver",
        "solver-model",
        allocation(1, 500, 600),
    )]));
    let mut primary = Backend::new(
        descriptor("fixture.primary", &["solver"]),
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
        )],
        "[1,2,3]",
    );
    Arc::get_mut(&mut primary).unwrap().fail_generation();
    let fallback = Backend::new(
        descriptor("fixture.fallback", &["solver"]),
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
        )],
        "[1,2,3]",
    );
    add_candidate(&engine, primary.clone(), "fixture.primary", 1);
    add_candidate(&engine, fallback.clone(), "fixture.fallback", 2);
    let result = engine
        .run_profile(QUERY, "fast", ExecutionProfile::Collective)
        .await;
    assert!(
        result.is_err()
            || result
                .as_ref()
                .is_ok_and(|outcome| outcome.status != "completed")
    );
    assert_eq!(primary.generations(), 1);
    assert_eq!(fallback.generations(), 0);
    cleanup(engine, store, home);
}

#[tokio::test]
async fn route_scope_changes_when_same_profile_selects_a_different_target() {
    let allocation = allocation(1, 500, 600);
    let (engine, store, _, home) = engine_with_resource_routes(resources(vec![
        working_profile("solver.a", "solver", "solver-model-a", allocation),
        working_profile("solver.b", "solver", "solver-model-b", allocation),
    ]));
    let first = Backend::new(
        ModuleDescriptor {
            id: "fixture.route".into(),
            version: "1.0.0".into(),
            roles: vec!["solver".into()],
        },
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("solver-model-a")))),
        )],
        "[1,2,3]",
    );
    engine
        .broker
        .register_candidate(first.descriptor.clone(), first.clone(), 1)
        .unwrap();
    let result_a = engine
        .run_profile(QUERY, "fast", ExecutionProfile::Collective)
        .await
        .unwrap();
    let second = Backend::new(
        ModuleDescriptor {
            id: "fixture.route".into(),
            version: "2.0.0".into(),
            roles: vec!["solver".into()],
        },
        [(
            "solver".into(),
            Ok(Some(InferenceTarget::LocalHttp(target("solver-model-b")))),
        )],
        "[1,2,3]",
    );
    engine
        .broker
        .register_candidate(second.descriptor.clone(), second.clone(), 1)
        .unwrap();
    let result_b = engine
        .run_profile(QUERY, "fast", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(result_a.status, "completed");
    assert_eq!(result_b.status, "completed");
    let trace_a = store.task(&result_a.task_id).unwrap();
    let trace_b = store.task(&result_b.task_id).unwrap();
    let scope_a =
        &events(&trace_a, "routing_resource_selection")[0]["payload"]["route_scope_sha256"];
    let scope_b =
        &events(&trace_b, "routing_resource_selection")[0]["payload"]["route_scope_sha256"];
    assert_ne!(scope_a, scope_b);
    assert_eq!(first.generations(), 1);
    assert_eq!(second.generations(), 1);
    cleanup(engine, store, home);
}

#[tokio::test]
async fn empty_resource_catalog_preserves_legacy_module_snapshot_and_events() {
    let home = home("legacy");
    let cfg = config(home.clone(), Some(resources(vec![])));
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend = Backend::new(
        ModuleDescriptor {
            id: "fixture.legacy".into(),
            version: "1.0.0".into(),
            roles: [
                "interface",
                "planner",
                "solver",
                "explorer",
                "critic",
                "reflector",
                "synthesizer",
            ]
            .iter()
            .map(|role| (*role).into())
            .collect(),
        },
        [
            "interface",
            "planner",
            "solver",
            "explorer",
            "critic",
            "reflector",
            "synthesizer",
        ]
        .iter()
        .map(|role| {
            (
                (*role).to_string(),
                Ok(Some(InferenceTarget::LocalHttp(target("solver-model")))),
            )
        }),
        "[1,2,3]",
    );
    let engine = Engine::with_backend(cfg, store.clone(), backend.clone());
    let result = engine
        .run_profile(QUERY, "fast", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(result.status, "completed");
    let trace = store.task(&result.task_id).unwrap();
    assert!(events(&trace, "routing_resource_intent").is_empty());
    assert!(events(&trace, "routing_resource_selection").is_empty());
    let snapshot = events(&trace, "module_snapshot").remove(0);
    assert!(snapshot["payload"].get("resource_routes").is_none());
    assert!(snapshot["payload"]
        .get("resource_routing_version")
        .is_none());
    assert_eq!(backend.generations(), 1);
    cleanup(engine, store, home);
}
