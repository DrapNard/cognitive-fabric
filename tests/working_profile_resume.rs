//! Recovery preserves configured role/target working profiles across parallel waves.
use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend, InferenceTarget, LocalBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    recovery::continuation,
    resource_scheduler::{Allocation, Configuration, Settings, WorkingProfile},
    store::Store,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    future::Future,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const PLAN: &str = r#"{"steps":[{"id":"a","objective":"Subgoal A","depends_on":[]},{"id":"c","objective":"Subgoal C","depends_on":["a"]},{"id":"b","objective":"Subgoal B","depends_on":[]},{"id":"d","objective":"Subgoal D","depends_on":[]}]}"#;

#[derive(Clone, Debug)]
struct Call {
    role: String,
    prompt: String,
}

struct FixtureBackend {
    cfg: Config,
    calls: Mutex<Vec<Call>>,
    first_wave: Arc<tokio::sync::Barrier>,
}

impl InferenceBackend for FixtureBackend {
    fn inference_target(&self, role: &str) -> Result<Option<InferenceTarget>> {
        LocalBackend(self.cfg.clone()).inference_target(role)
    }

    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.calls.lock().unwrap().push(Call {
            role: role.into(),
            prompt: prompt.into(),
        });
        let wait_for_first_wave =
            role == "solver" && (prompt.contains("Subgoal A") || prompt.contains("Subgoal B"));
        let first_wave = self.first_wave.clone();
        let text = if role == "planner" {
            PLAN
        } else if prompt.contains("Subgoal A") {
            "result-a"
        } else if prompt.contains("Subgoal B") {
            "result-b"
        } else if prompt.contains("Subgoal C") {
            "result-c"
        } else if prompt.contains("Subgoal D") {
            "result-d"
        } else {
            r#"{"a":1}"#
        }
        .to_owned();
        Box::pin(async move {
            if wait_for_first_wave {
                tokio::time::timeout(Duration::from_secs(3), first_wave.wait())
                    .await
                    .map_err(|_| anyhow::anyhow!("First-wave fixture calls did not overlap"))?;
            }
            Ok(Generation {
                text,
                model: "working-profile-fixture/v1".into(),
                input_tokens: Some(2),
                output_tokens: Some(2),
                elapsed_ms: 0.1,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}

struct Checkpoint {
    cfg: Config,
    store: Arc<Store>,
    task: String,
    original_settings: Value,
    profile_digest: String,
    listener: TcpListener,
}

fn configuration(endpoint: &str, with_profiles: bool) -> Configuration {
    let capacity = Allocation {
        compute_slots: 2,
        host_memory_bytes: 256,
        device_memory_bytes: 256,
    };
    let target = cogfabric::backend::ModelTarget {
        api_base: endpoint.into(),
        model: "working-profile-fixture/v1".into(),
    };
    let working_profiles = if with_profiles {
        vec![
            WorkingProfile {
                id: "planner_pool".into(),
                target: target.clone(),
                roles: vec!["planner".into()],
                allocation: Allocation {
                    compute_slots: 1,
                    host_memory_bytes: 16,
                    device_memory_bytes: 8,
                },
            },
            WorkingProfile {
                id: "solver_pool".into(),
                target,
                roles: vec!["solver".into()],
                allocation: Allocation {
                    compute_slots: 1,
                    host_memory_bytes: 32,
                    device_memory_bytes: 16,
                },
            },
        ]
    } else {
        Vec::new()
    };
    Configuration {
        settings: Settings {
            label: "profiled_pool".into(),
            capacity,
            max_pending: 4,
            max_wait_ms: 5_000,
            max_execution_ms: 5_000,
            scheduling: None,
        },
        allocation: Allocation {
            compute_slots: 1,
            host_memory_bytes: 8,
            device_memory_bytes: 8,
        },
        residents: Vec::new(),
        working_profiles,
    }
}

async fn completed_checkpoint(with_profiles: bool) -> Checkpoint {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let home = std::env::temp_dir().join(format!(
        "fabric-working-profile-resume-{}",
        uuid::Uuid::new_v4()
    ));
    let cfg = Config {
        home,
        backend: "openai-local".into(),
        model: "working-profile-fixture/v1".into(),
        api_base: endpoint.clone(),
        interpret_goals: false,
        max_calls: 10,
        max_cycles: 1,
        max_tokens: 2_400,
        max_tool_calls: 8,
        timeout_seconds: 120,
        subtask_parallelism: Some(2),
        resource_admission: Some(configuration(&endpoint, with_profiles)),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend = Arc::new(FixtureBackend {
        cfg: cfg.clone(),
        calls: Mutex::new(Vec::new()),
        first_wave: Arc::new(tokio::sync::Barrier::new(2)),
    });
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), backend.clone())
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, r#"{"a":1}"#);
    let original_calls = backend.calls.lock().unwrap();
    assert_eq!(original_calls.len(), 6);
    assert_eq!(original_calls[0].role, "planner");
    assert!(original_calls
        .iter()
        .any(|call| call.role == "solver" && call.prompt.contains("Subgoal A")));
    drop(original_calls);
    drop(backend);

    let trace = store.task(&outcome.task_id).unwrap();
    let settings = event(&trace, "execution_settings").clone();
    let profile_digest = if with_profiles {
        event(&trace, "resource_profile_intent")["payload"]["working_profiles_sha256"]
            .as_str()
            .unwrap()
            .to_owned()
    } else {
        String::new()
    };
    assert_eq!(events(&trace, "model_dispatch").len(), 6);
    assert_eq!(events(&trace, "cycle_reservation").len(), 1);
    assert_eq!(events(&trace, "subtask_resources").len(), 4);
    assert_first_wave_resource_chains_overlap(&trace);
    assert_eq!(
        events(&trace, "resource_profile_intent").len(),
        if with_profiles { 6 } else { 0 }
    );
    assert_eq!(
        events(&trace, "resource_profile").len(),
        if with_profiles { 6 } else { 0 }
    );
    if with_profiles {
        assert_eq!(
            events(&trace, "resource_profile")[0]["payload"]["profile_id"],
            "planner_pool"
        );
        assert_eq!(
            events(&trace, "resource_profile")[1]["payload"]["profile_id"],
            "solver_pool"
        );
    }

    // Keep the complete planner + first two-solver-wave resource histories.
    let cutoff = {
        let db = Connection::open(cfg.db()).unwrap();
        db.query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='subtask_resources' AND json_extract(payload,'$.state')='unverified' AND json_extract(payload,'$.node_id') IN ('a','b')",
            [&outcome.task_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    };
    crash_at(&cfg, &store, &outcome.task_id, cutoff);
    // Verify the retained crash-prefix itself contains both dispatched solver
    // calls before either resource completion, rather than inferring overlap
    // from model-dispatch reservations alone.
    assert_first_wave_resource_chains_overlap(&store.task(&outcome.task_id).unwrap());
    Checkpoint {
        cfg,
        store,
        task: outcome.task_id,
        original_settings: settings,
        profile_digest,
        listener,
    }
}

fn event<'a>(trace: &'a Value, kind: &str) -> &'a Value {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == kind)
        .unwrap_or_else(|| panic!("missing {kind} event"))
}

fn events<'a>(trace: &'a Value, kind: &str) -> Vec<&'a Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == kind)
        .collect()
}

fn assert_first_wave_resource_chains_overlap(trace: &Value) {
    let solver_reservations = events(trace, "resource_intent")
        .into_iter()
        .filter(|item| item["payload"]["role"] == "solver")
        .take(2)
        .map(|item| {
            item["payload"]["reservation_id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<HashSet<_>>();
    assert_eq!(solver_reservations.len(), 2);
    let events = trace["events"].as_array().unwrap();
    let dispatches = events
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item["kind"] == "resource_dispatch"
                && solver_reservations.contains(
                    item["payload"]["reservation_id"]
                        .as_str()
                        .unwrap_or_default(),
                )
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let completions = events
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item["kind"] == "resource_completed"
                && solver_reservations.contains(
                    item["payload"]["reservation_id"]
                        .as_str()
                        .unwrap_or_default(),
                )
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(dispatches.len(), 2);
    assert_eq!(completions.len(), 2);
    assert!(dispatches[1] < completions[0]);
}

fn lock_path(cfg: &Config) -> std::path::PathBuf {
    let mut path = cfg.db().into_os_string();
    path.push(".runtime.lock");
    path.into()
}

/// Rewind only under the same exclusive flock required by public continuation.
fn crash_at(cfg: &Config, store: &Store, task: &str, cutoff: i64) {
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path(cfg))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    {
        let db = Connection::open(cfg.db()).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            rusqlite::params![task, cutoff],
        )
        .unwrap();
        db.execute(
            "UPDATE tasks SET status='running',answer='' WHERE id=?1",
            [task],
        )
        .unwrap();
    }
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN) }, 0);
    drop(lock);
    assert_eq!(store.recover_tasks().unwrap(), vec![task.to_owned()]);
}

async fn read_request(stream: &mut TcpStream) -> Value {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let count = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .expect("timed out reading provider request")
            .unwrap();
        assert!(count > 0, "provider closed before request completion");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(
            bytes.len() <= 64_000,
            "provider request exceeds fixture bound"
        );
        let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
        let length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .expect("request omitted content length")
            .trim()
            .parse::<usize>()
            .unwrap();
        if bytes.len() >= end + 4 + length {
            return serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
        }
    }
}

async fn accept_request(listener: &TcpListener) -> (TcpStream, Value) {
    let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("timed out waiting for model dispatch")
        .unwrap();
    let request = read_request(&mut stream).await;
    assert_eq!(request["model"], "working-profile-fixture/v1");
    (stream, request)
}

fn prompt(request: &Value) -> &str {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|message| message["content"].as_str())
        .next_back()
        .unwrap()
}

async fn respond(stream: &mut TcpStream, answer: &str) {
    let body = json!({
        "model":"working-profile-fixture/v1",
        "choices":[{"message":{"content":answer}}],
        "usage":{"prompt_tokens":2,"completion_tokens":2}
    })
    .to_string();
    stream
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            )
            .as_bytes(),
        )
        .await
        .unwrap();
}

async fn serve_pending_wave_and_final(listener: TcpListener) {
    // Do not answer either pending request until both sockets have arrived.
    // This makes the overlap assertion independent of timing guesses.
    let (mut first, first_request) = accept_request(&listener).await;
    let (mut second, second_request) = accept_request(&listener).await;
    let prompts = [prompt(&first_request), prompt(&second_request)];
    assert!(prompts.iter().any(|value| value.contains("Subgoal C")));
    assert!(prompts.iter().any(|value| value.contains("Subgoal D")));
    assert!(prompts.iter().any(|value| value.contains("result-a")));
    assert!(prompts.iter().all(|value| !value.contains("result-b")));
    respond(&mut second, "result-d-resumed").await;
    respond(&mut first, "result-c-resumed").await;

    let (mut solver, request) = accept_request(&listener).await;
    let final_prompt = prompt(&request);
    for expected in [
        "result-a",
        "result-b",
        "result-c-resumed",
        "result-d-resumed",
    ] {
        assert!(
            final_prompt.contains(expected),
            "final solver omitted {expected}"
        );
    }
    respond(&mut solver, r#"{"a":1}"#).await;
}

fn mutate_profile_audit(checkpoint: &Checkpoint, kind: &str) {
    let db = Connection::open(checkpoint.cfg.db()).unwrap();
    match kind {
        "intent" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.permissions_granted',true) WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile_intent' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap();
        }
        "intent_digest" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.working_profiles_sha256',?2) WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile_intent' ORDER BY id LIMIT 1)", rusqlite::params![checkpoint.task, "e".repeat(64)]).unwrap();
        }
        "decision" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.profile_id','other_pool') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap();
        }
        "target" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.target.target.model','unlisted-model') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap();
        }
        "digest" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.working_profiles_sha256',?2) WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile' ORDER BY id LIMIT 1)", rusqlite::params![checkpoint.task, "f".repeat(64)]).unwrap();
        }
        "allocation" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.allocation.host_memory_bytes',255) WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap();
        }
        "chronology" => {
            db.execute("UPDATE events SET kind='resource_profile_intent' WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap();
        }
        "denied" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.allowed',false,'$.reason_code','unknown_or_unlisted_target') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap();
        }
        "unmatched" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.reservation_id','00000000-0000-4000-8000-000000000000') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='resource_profile' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap();
        }
        _ => panic!("unknown profile audit tampering fixture"),
    }
}

#[tokio::test]
async fn resumes_profiled_parallel_wave_with_saved_config_and_exact_accounting() {
    let checkpoint = completed_checkpoint(true).await;
    let before = checkpoint.store.task(&checkpoint.task).unwrap();
    let preview = continuation::preview(&checkpoint.store, &checkpoint.task).unwrap();
    assert_eq!(preview["checkpoint_eligible"], true);
    assert_eq!(preview["resource_history"]["completed_calls"], 3);
    assert_eq!(preview["parallel_history"]["completed_waves"], 1);
    assert_eq!(preview["consumed"]["calls"], 3);
    // The whole-goal solver cycle is reserved after all advisory waves; it is
    // not charged again when the completed first wave is restored.
    assert_eq!(preview["consumed"]["cycles"], 0);
    assert_eq!(preview["consumed"]["reserved_output_tokens"], 640);
    assert_eq!(checkpoint.store.task(&checkpoint.task).unwrap(), before);

    // The durable task snapshot, not a subsequently edited config file, governs recovery.
    let mut changed = checkpoint.cfg.clone();
    changed
        .resource_admission
        .as_mut()
        .unwrap()
        .working_profiles
        .iter_mut()
        .find(|profile| profile.id == "solver_pool")
        .unwrap()
        .allocation
        .host_memory_bytes = 64;
    changed.save().unwrap();

    let server = tokio::spawn(serve_pending_wave_and_final(checkpoint.listener));
    let report = continuation::resume(
        checkpoint.store.clone(),
        &checkpoint.task,
        true,
        std::future::pending::<()>(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["verified"], true);
    assert_eq!(report["new_calls"], 3);
    assert_eq!(report["charged_resources"]["calls"], 6);
    assert_eq!(report["charged_resources"]["reserved_output_tokens"], 1_408);
    assert_eq!(report["parallel_history"]["completed_waves"], 1);

    let trace = checkpoint.store.task(&checkpoint.task).unwrap();
    assert_eq!(
        event(&trace, "execution_settings"),
        &checkpoint.original_settings
    );
    assert_eq!(
        events(&trace, "model_dispatch").len(),
        6,
        "planner and first wave must not replay"
    );
    assert_eq!(events(&trace, "cycle_reservation").len(), 1);
    assert_eq!(events(&trace, "resource_profile_intent").len(), 6);
    assert_eq!(events(&trace, "resource_profile").len(), 6);
    assert_eq!(events(&trace, "resource_intent").len(), 6);
    assert_eq!(events(&trace, "resource_admitted").len(), 6);
    assert_eq!(events(&trace, "resource_dispatch").len(), 6);
    assert_eq!(events(&trace, "resource_completed").len(), 6);

    let profiles = events(&trace, "resource_profile");
    for profile in profiles {
        assert_eq!(
            profile["payload"]["working_profiles_sha256"],
            checkpoint.profile_digest
        );
    }
    for event in events(&trace, "resource_intent") {
        let allocation = &event["payload"]["allocation"];
        if event["payload"]["role"] == "planner" {
            assert_eq!(
                allocation,
                &json!({"compute_slots":1,"host_memory_bytes":16,"device_memory_bytes":8})
            );
        } else {
            assert_eq!(
                allocation,
                &json!({"compute_slots":1,"host_memory_bytes":32,"device_memory_bytes":16})
            );
        }
    }

    // The two solver chains start profiling before either can complete its HTTP request.
    let events = trace["events"].as_array().unwrap();
    let solver_intents = events
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item["kind"] == "resource_profile_intent" && item["payload"]["role"] == "solver"
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let solver_reservations = events
        .iter()
        .filter(|item| {
            item["kind"] == "resource_profile_intent" && item["payload"]["role"] == "solver"
        })
        .map(|item| {
            item["payload"]["reservation_id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<HashSet<_>>();
    let solver_completions = events
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item["kind"] == "resource_completed"
                && solver_reservations.contains(
                    item["payload"]["reservation_id"]
                        .as_str()
                        .unwrap_or_default(),
                )
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(solver_intents.len(), 5);
    assert_eq!(solver_completions.len(), 5);
    assert!(solver_intents[1] < solver_completions[0]);

    let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
}

#[tokio::test]
async fn profile_intent_decision_target_digest_allocation_order_denial_and_chain_tampering_fail_closed(
) {
    for kind in [
        "intent",
        "intent_digest",
        "decision",
        "target",
        "digest",
        "allocation",
        "chronology",
        "denied",
        "unmatched",
    ] {
        let checkpoint = completed_checkpoint(true).await;
        mutate_profile_audit(&checkpoint, kind);
        let before = checkpoint.store.task(&checkpoint.task).unwrap();
        assert!(
            continuation::preview(&checkpoint.store, &checkpoint.task).is_err(),
            "{kind} preview must fail closed"
        );
        assert!(
            continuation::resume(
                checkpoint.store.clone(),
                &checkpoint.task,
                true,
                std::future::pending::<()>(),
            )
            .await
            .is_err(),
            "{kind} resume must fail closed"
        );
        assert_eq!(checkpoint.store.task(&checkpoint.task).unwrap(), before);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), checkpoint.listener.accept())
                .await
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
    }
}

#[tokio::test]
async fn profile_empty_configuration_keeps_legacy_parallel_recovery() {
    let checkpoint = completed_checkpoint(false).await;
    let trace = checkpoint.store.task(&checkpoint.task).unwrap();
    assert!(events(&trace, "resource_profile_intent").is_empty());
    assert!(events(&trace, "resource_profile").is_empty());
    let server = tokio::spawn(serve_pending_wave_and_final(checkpoint.listener));
    let report = continuation::resume(
        checkpoint.store.clone(),
        &checkpoint.task,
        true,
        std::future::pending::<()>(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["new_calls"], 3);
    assert_eq!(report["charged_resources"]["calls"], 6);
    let after = checkpoint.store.task(&checkpoint.task).unwrap();
    assert!(events(&after, "resource_profile").is_empty());
    assert_eq!(events(&after, "model_dispatch").len(), 6);
    let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
}
