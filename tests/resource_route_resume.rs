//! Recovery keeps the original resource-aware module route mask and target set.
use cogfabric::{
    config::Config,
    engine::{Engine, ExecutionProfile},
    recovery::continuation,
    resource_scheduler::{Allocation, Configuration, Settings, WorkingProfile},
    store::Store,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const PLAN: &str = r#"{"steps":[{"id":"a","objective":"Subgoal A","depends_on":[]},{"id":"c","objective":"Subgoal C","depends_on":["a"]},{"id":"b","objective":"Subgoal B","depends_on":[]},{"id":"d","objective":"Subgoal D","depends_on":[]}] }"#;

struct Checkpoint {
    cfg: Config,
    store: Arc<Store>,
    task: String,
    settings: Value,
    selection: Value,
    listener: Arc<TcpListener>,
}

fn configuration(endpoint: &str) -> Configuration {
    let target = cogfabric::backend::ModelTarget {
        api_base: endpoint.to_owned(),
        model: "route-fixture/v1".to_owned(),
    };
    Configuration {
        settings: Settings {
            label: "route_pool".into(),
            capacity: Allocation {
                compute_slots: 2,
                host_memory_bytes: 256,
                device_memory_bytes: 256,
            },
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
        working_profiles: vec![
            WorkingProfile {
                id: "planner_route".into(),
                target: target.clone(),
                roles: vec!["planner".into()],
                allocation: Allocation {
                    compute_slots: 1,
                    host_memory_bytes: 16,
                    device_memory_bytes: 8,
                },
            },
            WorkingProfile {
                id: "solver_route".into(),
                target,
                roles: vec!["solver".into()],
                allocation: Allocation {
                    compute_slots: 1,
                    host_memory_bytes: 32,
                    device_memory_bytes: 16,
                },
            },
        ],
    }
}

async fn completed_checkpoint() -> Checkpoint {
    let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.unwrap());
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let home = std::env::temp_dir().join(format!("fabric-resource-route-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home,
        backend: "openai-local".into(),
        model: "route-fixture/v1".into(),
        api_base: endpoint.clone(),
        interpret_goals: false,
        max_calls: 10,
        max_cycles: 1,
        max_tokens: 2_400,
        max_tool_calls: 8,
        timeout_seconds: 120,
        subtask_parallelism: Some(2),
        resource_admission: Some(configuration(&endpoint)),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let server = tokio::spawn(serve_initial(listener.clone()));
    let outcome = Engine::new(cfg.clone(), store.clone())
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, r#"{"a":1}"#);
    server.await.unwrap();

    let trace = store.task(&outcome.task_id).unwrap();
    let settings = event(&trace, "execution_settings").clone();
    let route_intent = events(&trace, "routing_resource_intent");
    let route_selection = events(&trace, "routing_resource_selection");
    assert_eq!(route_intent.len(), 1);
    assert_eq!(route_selection.len(), 1);
    let selection = route_selection[0]["payload"].clone();
    let roles = selection["routes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|route| route["role"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(roles, vec!["planner", "solver"]);
    assert_eq!(selection["unmatched_roles"].as_array().unwrap().len(), 5);
    assert_eq!(selection["eligible"], true);
    assert_eq!(selection["permissions_granted"], false);
    assert_eq!(selection["weights_attested"], false);
    assert_eq!(selection["physical_limits_enforced"], false);
    assert_eq!(selection["routes"][0]["module_id"], "inference.local");
    assert_eq!(selection["routes"][0]["module_version"], "1.0.0");
    assert_eq!(
        selection["routes"][0]["resource"]["profile_id"],
        "planner_route"
    );
    assert_eq!(
        selection["routes"][1]["resource"]["profile_id"],
        "solver_route"
    );
    let snapshot = event(&trace, "module_snapshot");
    assert_eq!(snapshot["payload"]["resource_routing_version"], 1);
    assert_eq!(snapshot["payload"]["resource_routes"], selection);
    assert_eq!(snapshot["payload"]["modules"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["payload"]["modules"][0]["id"], "inference.local");
    assert_eq!(snapshot["payload"]["modules"][0]["version"], "1.0.0");
    assert_eq!(events(&trace, "model_dispatch").len(), 6);
    assert!(events(&trace, "model_dispatch").iter().all(|item| {
        item["payload"]["role"] == "planner" || item["payload"]["role"] == "solver"
    }));

    // Keep the planner and completed first subtask wave, creating the same
    // recoverable boundary used by the public continuation API.
    let cutoff = {
        let db = Connection::open(cfg.db()).unwrap();
        db.query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='subtask_resources' AND json_extract(payload,'$.state')='unverified' AND json_extract(payload,'$.node_id') IN ('a','b')",
            [&outcome.task_id], |row| row.get::<_, i64>(0),
        ).unwrap()
    };
    crash_at(&cfg, &store, &outcome.task_id, cutoff);
    Checkpoint {
        cfg,
        store,
        task: outcome.task_id,
        settings,
        selection,
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

fn lock_path(cfg: &Config) -> std::path::PathBuf {
    let mut path = cfg.db().into_os_string();
    path.push(".runtime.lock");
    path.into()
}

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
            params![task, cutoff],
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
            .unwrap()
            .unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() <= 64_000);
        let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
        let length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .unwrap()
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
        .unwrap()
        .unwrap();
    let request = read_request(&mut stream).await;
    assert_eq!(request["model"], "route-fixture/v1");
    (stream, request)
}

fn prompt(request: &Value) -> &str {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["content"].as_str())
        .next_back()
        .unwrap()
}

async fn respond(stream: &mut TcpStream, answer: &str) {
    let body = json!({"model":"route-fixture/v1","choices":[{"message":{"content":answer}}],"usage":{"prompt_tokens":2,"completion_tokens":2}}).to_string();
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
}

fn answer_for_prompt(prompt: &str) -> &'static str {
    if prompt.contains("Subgoal A") {
        "result-a"
    } else if prompt.contains("Subgoal B") {
        "result-b"
    } else if prompt.contains("Subgoal C") {
        "result-c"
    } else if prompt.contains("Subgoal D") {
        "result-d"
    } else if prompt.contains("Subgoal") {
        "result"
    } else if prompt.contains("You are the task planner") {
        PLAN
    } else {
        r#"{"a":1}"#
    }
}

async fn serve_initial(listener: Arc<TcpListener>) {
    for index in 0..6 {
        let (mut stream, request) = accept_request(&listener).await;
        // The OpenAI-compatible request does not carry the local reasoning
        // role, and the planner receives the original query just like later
        // calls. The first bounded call is the planner by construction.
        let answer = if index == 0 {
            PLAN
        } else {
            answer_for_prompt(prompt(&request))
        };
        respond(&mut stream, answer).await;
    }
}

async fn serve_resume(listener: Arc<TcpListener>) {
    // Keep both pending solver requests open until they have arrived.
    let (mut first, first_request) = accept_request(&listener).await;
    let (mut second, second_request) = accept_request(&listener).await;
    let prompts = [prompt(&first_request), prompt(&second_request)];
    assert!(prompts.iter().any(|item| item.contains("Subgoal C")));
    assert!(prompts.iter().any(|item| item.contains("Subgoal D")));
    assert!(prompts
        .iter()
        .all(|item| !item.contains("Subgoal A") && !item.contains("Subgoal B")));
    respond(&mut first, "result-c-resumed").await;
    respond(&mut second, "result-d-resumed").await;
    let (mut final_call, request) = accept_request(&listener).await;
    let final_prompt = prompt(&request);
    for expected in [
        "result-a",
        "result-b",
        "result-c-resumed",
        "result-d-resumed",
    ] {
        assert!(
            final_prompt.contains(expected),
            "final call omitted {expected}"
        );
    }
    respond(&mut final_call, r#"{"a":1}"#).await;
}

#[tokio::test]
async fn resumes_with_exact_saved_builtin_route_mask_and_scope() {
    let checkpoint = completed_checkpoint().await;
    let before = checkpoint.store.task(&checkpoint.task).unwrap();
    let preview = continuation::preview(&checkpoint.store, &checkpoint.task).unwrap();
    assert_eq!(preview["checkpoint_eligible"], true);
    assert_eq!(preview["consumed"]["calls"], 3);
    assert_eq!(preview["consumed"]["cycles"], 0);
    assert_eq!(checkpoint.store.task(&checkpoint.task).unwrap(), before);

    // Current config-file edits cannot change the original task's selected targets.
    let mut changed = checkpoint.cfg.clone();
    changed
        .resource_admission
        .as_mut()
        .unwrap()
        .working_profiles[1]
        .allocation
        .host_memory_bytes = 64;
    changed.save().unwrap();

    let server = tokio::spawn(serve_resume(checkpoint.listener));
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
    let trace = checkpoint.store.task(&checkpoint.task).unwrap();
    assert_eq!(event(&trace, "execution_settings"), &checkpoint.settings);
    assert_eq!(
        events(&trace, "model_dispatch").len(),
        6,
        "planner and completed calls were replayed"
    );
    assert!(events(&trace, "model_dispatch")
        .iter()
        .all(|item| item["payload"]["role"] == "planner" || item["payload"]["role"] == "solver"));
    let selection = events(&trace, "routing_resource_selection");
    assert_eq!(selection.len(), 1);
    assert_eq!(
        selection[0]["payload"]["route_scope_sha256"],
        checkpoint.selection["route_scope_sha256"]
    );
    assert_eq!(
        selection[0]["payload"]["routes"],
        checkpoint.selection["routes"]
    );
    assert_eq!(events(&trace, "routing_resource_intent").len(), 1);
    assert_eq!(events(&trace, "cycle_reservation").len(), 1);
    assert_eq!(events(&trace, "resource_completed").len(), 6);
    let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
}

fn tamper_route(checkpoint: &Checkpoint, case: &str) {
    let db = Connection::open(checkpoint.cfg.db()).unwrap();
    match case {
        "intent_source" => db.execute("UPDATE events SET source='forged' WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_intent' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "selection_role" => db.execute("UPDATE events SET payload=json_set(payload,'$.routes[0].role','explorer') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_selection' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "selection_target" => db.execute("UPDATE events SET payload=json_set(payload,'$.routes[0].resource.target.target.model','unlisted-model') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_selection' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "selection_digest" => db.execute("UPDATE events SET payload=json_set(payload,'$.route_scope_sha256',?2) WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_selection' ORDER BY id LIMIT 1)", params![checkpoint.task, "e".repeat(64)]).unwrap(),
        "selection_profile" => db.execute("UPDATE events SET payload=json_set(payload,'$.routes[1].resource.profile_id','other-profile') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_selection' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "selection_module" => db.execute("UPDATE events SET payload=json_set(payload,'$.routes[0].module_id','custom.inference') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_selection' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "snapshot_routes" => db.execute("UPDATE events SET payload=json_set(payload,'$.resource_routes.routes[0].role','explorer') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='module_snapshot' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "snapshot_marker" => db.execute("UPDATE events SET payload=json_remove(payload,'$.resource_routing_version') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='module_snapshot' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "duplicate_selection" => db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='routing_resource_selection' ORDER BY id LIMIT 1", [&checkpoint.task]).unwrap(),
        "missing_intent" => db.execute("DELETE FROM events WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_intent' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "missing_selection" => db.execute("DELETE FROM events WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_selection' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "chronology" => db.execute("UPDATE events SET kind='routing_resource_selection' WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='routing_resource_intent' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        "snapshot_module" => db.execute("UPDATE events SET payload=json_set(payload,'$.modules[0].id','custom.module') WHERE id=(SELECT id FROM events WHERE task_id=?1 AND kind='module_snapshot' ORDER BY id LIMIT 1)", [&checkpoint.task]).unwrap(),
        _ => panic!("unknown route tampering case: {case}"),
    };
}

#[tokio::test]
async fn routing_audit_and_snapshot_tampering_refuse_without_writes_or_provider_calls() {
    for case in [
        "intent_source",
        "selection_role",
        "selection_target",
        "selection_digest",
        "selection_profile",
        "selection_module",
        "snapshot_routes",
        "snapshot_marker",
        "duplicate_selection",
        "missing_intent",
        "missing_selection",
        "chronology",
        "snapshot_module",
    ] {
        let checkpoint = completed_checkpoint().await;
        tamper_route(&checkpoint, case);
        let before = checkpoint.store.task(&checkpoint.task).unwrap();
        assert!(
            continuation::preview(&checkpoint.store, &checkpoint.task).is_err(),
            "{case} preview should refuse"
        );
        assert!(
            continuation::resume(
                checkpoint.store.clone(),
                &checkpoint.task,
                true,
                std::future::pending::<()>()
            )
            .await
            .is_err(),
            "{case} resume should refuse"
        );
        assert_eq!(
            checkpoint.store.task(&checkpoint.task).unwrap(),
            before,
            "{case} changed the task"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), checkpoint.listener.accept())
                .await
                .is_err(),
            "{case} contacted provider"
        );
        let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);
    }
}

#[tokio::test]
async fn legacy_unmarked_snapshot_keeps_prior_profile_recovery_but_marker_requires_pair() {
    let checkpoint = completed_checkpoint().await;
    {
        let db = Connection::open(checkpoint.cfg.db()).unwrap();
        db.execute("DELETE FROM events WHERE task_id=?1 AND kind IN ('routing_resource_intent','routing_resource_selection')", [&checkpoint.task]).unwrap();
        db.execute("UPDATE events SET payload=json_remove(payload,'$.resource_routing_version','$.resource_routes') WHERE task_id=?1 AND kind='module_snapshot'", [&checkpoint.task]).unwrap();
        db.execute("UPDATE events SET payload=json_remove(payload,'$.resource_routing_version') WHERE task_id=?1 AND kind='execution_settings'", [&checkpoint.task]).unwrap();
    }
    let before = checkpoint.store.task(&checkpoint.task).unwrap();
    assert_eq!(
        continuation::preview(&checkpoint.store, &checkpoint.task).unwrap()["checkpoint_eligible"],
        true
    );
    assert_eq!(checkpoint.store.task(&checkpoint.task).unwrap(), before);
    let server = tokio::spawn(serve_resume(checkpoint.listener));
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
    let _ = std::fs::remove_dir_all(&checkpoint.cfg.home);

    let marked = completed_checkpoint().await;
    {
        let db = Connection::open(marked.cfg.db()).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND kind='routing_resource_selection'",
            [&marked.task],
        )
        .unwrap();
    }
    let before = marked.store.task(&marked.task).unwrap();
    assert!(continuation::preview(&marked.store, &marked.task).is_err());
    assert!(continuation::resume(
        marked.store.clone(),
        &marked.task,
        true,
        std::future::pending::<()>()
    )
    .await
    .is_err());
    assert_eq!(marked.store.task(&marked.task).unwrap(), before);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), marked.listener.accept())
            .await
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&marked.cfg.home);
}
