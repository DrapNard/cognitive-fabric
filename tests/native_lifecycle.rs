use cogfabric::{
    config::Config,
    engine::Engine,
    residency::lifecycle::NativeLifecyclePolicy,
    service,
    store::{self, Store},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Child, Output, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn root() -> PathBuf {
    std::env::temp_dir().join(format!("cogfabric-native-task-{}", uuid::Uuid::new_v4()))
}

fn config(home: &Path, wall_seconds: u64) -> Config {
    Config {
        home: home.to_path_buf(),
        backend: "openai-local".into(),
        api_base: "http://127.0.0.1:1234/v1".into(),
        model: "alpha".into(),
        interpret_goals: false,
        max_calls: 4,
        max_cycles: 2,
        max_tokens: 1024,
        max_tool_calls: 4,
        timeout_seconds: 30,
        max_wall_seconds: wall_seconds,
        ..Config::default()
    }
}

fn write_policy(root: &Path, api_base: &str, release_after: bool) -> PathBuf {
    let path = root.join("native-policy.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "format_version": 1,
            "api_base": api_base,
            "authenticate": false,
            "models": [{"model":"alpha","context_length":2048}],
            "release_after": release_after,
            "max_lifecycle_seconds": 20
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn write_lazy_policy(root: &Path, api_base: &str, release_after: bool, models: &[&str]) -> PathBuf {
    let path = root.join("native-lazy-policy.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "format_version": 3,
            "api_base": api_base,
            "authenticate": false,
            "models": models.iter().map(|model| json!({"model":model,"context_length":2048})).collect::<Vec<_>>(),
            "release_after": release_after,
            "max_lifecycle_seconds": 20
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

#[derive(Clone, Copy)]
enum ChatReply {
    Correct,
    WrongInstance,
    Sorted,
    SortedThenWrong,
}

#[derive(Default)]
struct Seen {
    requests: Vec<String>,
    unloads: usize,
    chats: usize,
    loads: BTreeMap<String, usize>,
}

async fn http_request(stream: &mut tokio::net::TcpStream) -> (String, Value) {
    let mut bytes = Vec::new();
    loop {
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                let head = String::from_utf8_lossy(&bytes[..end]).to_string();
                let body = if length == 0 {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap()
                };
                return (head, body);
            }
        }
        let mut chunk = [0u8; 2048];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(
            count > 0,
            "provider closed before sending a complete request"
        );
        bytes.extend_from_slice(&chunk[..count]);
        assert!(
            bytes.len() <= 65_536,
            "native request exceeds fixture bounds"
        );
    }
}

async fn response(stream: &mut tokio::net::TcpStream, value: Value) {
    let body = value.to_string();
    stream
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .as_bytes(),
        )
        .await
        .unwrap();
}

async fn native_fixture(
    listener: tokio::net::TcpListener,
    reply: ChatReply,
    stop: tokio::sync::oneshot::Receiver<()>,
    stalled_load: Option<tokio::sync::oneshot::Sender<()>>,
) -> Arc<Mutex<Seen>> {
    native_fixture_with_preloaded(listener, reply, stop, stalled_load, BTreeMap::new()).await
}

async fn native_fixture_with_preloaded(
    listener: tokio::net::TcpListener,
    reply: ChatReply,
    stop: tokio::sync::oneshot::Receiver<()>,
    stalled_load: Option<tokio::sync::oneshot::Sender<()>>,
    preloaded: BTreeMap<String, Vec<(String, u64)>>,
) -> Arc<Mutex<Seen>> {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let server_seen = seen.clone();
    tokio::spawn(async move {
        let mut stop = Box::pin(stop);
        let mut stalled_load = stalled_load;
        let mut loaded = preloaded;
        loop {
            let accepted = tokio::select! {
                _ = &mut stop => break,
                accepted = tokio::time::timeout(Duration::from_millis(100), listener.accept()) => {
                    match accepted {
                        Ok(Ok(value)) => value,
                        Ok(Err(error)) => panic!("native fixture accept failed: {error}"),
                        Err(_) => continue,
                    }
                }
            };
            let (mut stream, _) = accepted;
            let (head, body) = http_request(&mut stream).await;
            let path = head.lines().next().unwrap_or_default().to_owned();
            server_seen.lock().unwrap().requests.push(path.clone());
            if path.starts_with("GET /api/v1/models ") {
                let models = ["alpha", "beta"].into_iter().map(|model|json!({
                    "key":model,"type":"llm","loaded_instances":loaded.get(model).cloned().unwrap_or_default().into_iter().map(|(id,context_length)|json!({"id":id,"config":{"context_length":context_length}})).collect::<Vec<_>>()
                })).collect::<Vec<_>>();
                let inventory = json!({"models":models});
                response(&mut stream, inventory).await;
            } else if path.starts_with("POST /api/v1/models/load ") {
                if let Some(sender) = stalled_load.take() {
                    let _ = sender.send(());
                    let mut byte = [0u8; 1];
                    let _ = stream.read(&mut byte).await;
                    continue;
                }
                let model = body["model"].as_str().unwrap().to_owned();
                let context = body["context_length"].as_u64().unwrap();
                let instance = format!("{model}-instance");
                *server_seen
                    .lock()
                    .unwrap()
                    .loads
                    .entry(model.clone())
                    .or_default() += 1;
                loaded
                    .entry(model)
                    .or_default()
                    .push((instance.clone(), context));
                response(&mut stream, json!({
                    "status":"loaded","type":"llm","instance_id":instance,"load_time_seconds":0.01
                })).await;
            } else if path.starts_with("POST /api/v1/models/unload ") {
                server_seen.lock().unwrap().unloads += 1;
                let instance = body["instance_id"].as_str().unwrap();
                for values in loaded.values_mut() {
                    values.retain(|(id, _)| id != instance);
                }
                response(&mut stream, json!({"instance_id":instance})).await;
            } else if path.starts_with("POST /api/v1/chat ") {
                server_seen.lock().unwrap().chats += 1;
                let model = body["model"].as_str().unwrap();
                assert!(matches!(model, "alpha" | "beta"));
                assert_eq!(body["stream"], false);
                assert_eq!(body["store"], false);
                assert_eq!(body["integrations"], json!([]));
                let instance = match reply {
                    ChatReply::SortedThenWrong if server_seen.lock().unwrap().chats >= 3 => {
                        "wrong-instance"
                    }
                    ChatReply::Correct | ChatReply::Sorted | ChatReply::SortedThenWrong => {
                        if model == "alpha" {
                            "alpha-instance"
                        } else {
                            "beta-instance"
                        }
                    }
                    ChatReply::WrongInstance => "wrong-instance",
                };
                response(
                    &mut stream,
                    json!({
                        "model_instance_id":instance,
                        "output":[{"type":"message","content":if matches!(reply,ChatReply::Sorted | ChatReply::SortedThenWrong) { "[1,2,3]" } else { "Hello from the local model." }}],
                        "stats":{"input_tokens":3,"total_output_tokens":5}
                    }),
                )
                .await;
            } else {
                panic!("unexpected native endpoint: {path}");
            }
        }
    });
    seen
}

async fn cli(home: &Path, args: &[&str]) -> Output {
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .env_clear()
        .arg("--home")
        .arg(home)
        .arg("--backend")
        .arg("openai-local")
        .arg("--json")
        .args(args)
        .output()
        .await
        .unwrap();
    if !output.status.success() {
        eprintln!(
            "Fixture CLI result: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    output
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

async fn wait_for_socket(cfg: &Config, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cfg.socket().exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "service exited during startup"
        );
        assert!(Instant::now() < deadline, "service startup timed out");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn cli_ask_uses_approved_native_instance_and_persists_policy_scope() {
    let home = root();
    let cfg = config(&home, 10);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy = write_policy(&home, &api_base, true);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;

    let output = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            policy.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "unverified");
    assert_eq!(result["answer"], "Hello from the local model.");
    let task_id = result["task_id"].as_str().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(task_id).unwrap();
    let events = trace["events"].as_array().unwrap();
    let task_scope = events
        .iter()
        .find(|event| event["kind"] == "native_task_scope")
        .unwrap();
    let policy_digest = task_scope["payload"]["policy_sha256"].as_str().unwrap();
    assert_eq!(policy_digest.len(), 64);
    assert_eq!(task_scope["payload"]["instance_binding_required"], true);
    assert!(events
        .iter()
        .any(|event| event["kind"] == "native_instance_dispatch"));
    assert_eq!(
        events
            .iter()
            .filter(|event| event["kind"] == "execution_options")
            .count(),
        1
    );
    assert!(seen.lock().unwrap().chats >= 1);
    assert_eq!(seen.lock().unwrap().unloads, 1);
    let _ = stop_tx.send(());
    tokio::time::sleep(Duration::from_millis(120)).await;
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn native_restoration_preflight_is_read_only_and_refuses_policy_or_binding_changes() {
    let home = root();
    let cfg = config(&home, 120);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let path = write_policy(&home, &api_base, false);
    let policy = NativeLifecyclePolicy::load(&path).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;
    let result = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            path.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    let task = result["task_id"].as_str().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    store.finish_task(task, "interrupted", "").unwrap();
    let before = store.task(task).unwrap();
    let chats = seen.lock().unwrap().chats;
    let report =
        cogfabric::residency::lifecycle::restoration::inspect(store.clone(), task, &policy)
            .await
            .unwrap();
    assert_eq!(report["receipt_identity_matches"], true);
    assert_eq!(report["resume_authorized"], false);
    assert_eq!(report["checkpoint_validated"], false);
    assert_eq!(report["model_mutation_performed"], false);
    assert!(report["original_remaining_ns"].as_u64().unwrap() > 0);
    assert_eq!(store.task(task).unwrap(), before);
    assert_eq!(seen.lock().unwrap().chats, chats);
    assert_eq!(seen.lock().unwrap().unloads, 0);
    let command = cli(
        &home,
        &[
            "native-resume-preview",
            task,
            "--policy",
            path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(
        command.status.success(),
        "{}",
        String::from_utf8_lossy(&command.stderr)
    );
    assert_eq!(store.task(task).unwrap(), before);
    let count = seen.lock().unwrap().requests.len();
    let mut changed = policy.clone();
    changed.models[0].context_length += 1;
    assert!(
        cogfabric::residency::lifecycle::restoration::inspect(store.clone(), task, &changed)
            .await
            .is_err()
    );
    assert_eq!(seen.lock().unwrap().requests.len(), count);
    let parent = report["lifecycle_task_id"].as_str().unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let mut statement = db.prepare("SELECT id,task_id,kind,payload FROM events WHERE (task_id=?1 AND kind IN ('native_task_scope','execution_settings')) OR (task_id=?2 AND kind='native_task_lifecycle' AND json_extract(payload,'$.state')='generation_confirmed')").unwrap();
    let saved = statement
        .query_map(rusqlite::params![task, parent], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    drop(statement);
    for scenario in 0..5 {
        for (id, _, kind, raw) in &saved {
            let mut payload: Value = serde_json::from_str(raw).unwrap();
            match (scenario, kind.as_str()) {
                (0, "native_task_scope") => payload["permissions_granted"] = json!(true),
                (1, "execution_settings") => {
                    payload["configuration"]["model"] = json!("unapproved-model")
                }
                (2, "execution_settings") => {
                    payload["execution_clock"]["started_ns"] = json!(0);
                    payload["execution_clock"]["deadline_ns"] = json!(120_000_000_000u64);
                }
                (3, "native_task_lifecycle") => payload["state"] = json!("generation_started"),
                (4, "native_task_lifecycle") => {
                    payload["endpoint"] = json!("http://127.0.0.1:1/api/v1/models")
                }
                _ => {}
            }
            db.execute(
                "UPDATE events SET payload=?2 WHERE id=?1",
                rusqlite::params![id, payload.to_string()],
            )
            .unwrap();
        }
        let child_before = store.task(task).unwrap();
        let parent_before = store.task(parent).unwrap();
        assert!(
            cogfabric::residency::lifecycle::restoration::inspect(store.clone(), task, &policy)
                .await
                .is_err(),
            "scenario {scenario}"
        );
        assert_eq!(
            seen.lock().unwrap().requests.len(),
            count,
            "scenario {scenario}"
        );
        assert_eq!(store.task(task).unwrap(), child_before);
        assert_eq!(store.task(parent).unwrap(), parent_before);
        for (id, _, _, raw) in &saved {
            db.execute(
                "UPDATE events SET payload=?2 WHERE id=?1",
                rusqlite::params![id, raw],
            )
            .unwrap();
        }
    }
    drop(db);
    rusqlite::Connection::open(cfg.db()).unwrap().execute("UPDATE events SET payload=json_set(payload,'$.bindings_sha256',?2) WHERE task_id=?1 AND kind='native_task_bindings'",rusqlite::params![parent,"0".repeat(64)]).unwrap();
    let tampered = store.task(task).unwrap();
    assert!(
        cogfabric::residency::lifecycle::restoration::inspect(store.clone(), task, &policy)
            .await
            .is_err()
    );
    assert_eq!(seen.lock().unwrap().requests.len(), count);
    assert_eq!(store.task(task).unwrap(), tampered);
    drop(store);
    let _ = stop_tx.send(());
    tokio::time::sleep(Duration::from_millis(120)).await;
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn lazy_native_restoration_loads_only_used_models_and_binds_activation_origin() {
    let home = root();
    let cfg = config(&home, 120);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let path = write_lazy_policy(&home, &api_base, false, &["alpha", "beta"]);
    let policy = NativeLifecyclePolicy::load(&path).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;

    let output = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            path.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let task = result["task_id"].as_str().unwrap();
    assert_eq!(seen.lock().unwrap().loads.get("alpha"), Some(&1));
    assert_eq!(seen.lock().unwrap().loads.get("beta"), None);

    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    store.finish_task(task, "interrupted", "").unwrap();
    let report =
        cogfabric::residency::lifecycle::restoration::inspect(store.clone(), task, &policy)
            .await
            .unwrap();
    assert_eq!(report["receipt_count"], 1);
    assert_eq!(
        report["activation_receipt_origins"]["alpha"],
        "newly_loaded"
    );
    assert_eq!(seen.lock().unwrap().loads.get("beta"), None);

    let lifecycle_task = report["lifecycle_task_id"].as_str().unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let (event_id, raw): (i64, String) = db
        .query_row(
            "SELECT id,payload FROM events WHERE task_id=?1 AND kind='native_task_model_binding' AND source='security-kernel'",
            [lifecycle_task],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let mut binding: Value = serde_json::from_str(&raw).unwrap();
    binding["origin"] = json!("reused");
    let mut unsigned = binding.clone();
    unsigned
        .as_object_mut()
        .unwrap()
        .remove("activation_binding_sha256");
    binding["activation_binding_sha256"] = json!(cogfabric::store::sha256(
        &serde_json::to_vec(&unsigned).unwrap()
    ));
    db.execute(
        "UPDATE events SET payload=?2 WHERE id=?1",
        rusqlite::params![event_id, binding.to_string()],
    )
    .unwrap();
    drop(db);
    let requests = seen.lock().unwrap().requests.len();
    assert!(
        cogfabric::residency::lifecycle::restoration::inspect(store, task, &policy)
            .await
            .is_err(),
        "origin mutation must be caught by the lifecycle commitment"
    );
    assert_eq!(seen.lock().unwrap().requests.len(), requests);
    let _ = stop_tx.send(());
}

#[tokio::test]
async fn lazy_native_reuses_existing_instance_without_releasing_it() {
    let home = root();
    let cfg = config(&home, 120);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let path = write_lazy_policy(&home, &api_base, true, &["alpha"]);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;
    let store = Store::open(&cfg.db()).unwrap();
    store::bootstrap(&store).unwrap();
    cogfabric::residency::managed::load(&store, &api_base, false, "alpha", 2048, true)
        .await
        .unwrap();

    let output = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            path.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(seen.lock().unwrap().loads.get("alpha"), Some(&1));
    assert_eq!(seen.lock().unwrap().unloads, 0);
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let lifecycle_task: String = db
        .query_row(
            "SELECT task_id FROM events WHERE kind='native_task_bindings' AND json_extract(payload,'$.child_task_id')=?1",
            [result["task_id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let origin: String = db
        .query_row(
            "SELECT json_extract(payload,'$.origin') FROM events WHERE task_id=?1 AND kind='native_task_model_binding'",
            [lifecycle_task],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(origin, "reused");
    let _ = stop_tx.send(());
}

#[tokio::test]
async fn lazy_native_refuses_unreceipted_instances_without_adoption_or_cleanup() {
    let home = root();
    let cfg = config(&home, 120);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy = write_lazy_policy(&home, &api_base, true, &["alpha"]);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let preloaded =
        BTreeMap::from([("alpha".to_owned(), vec![("foreign-alpha".to_owned(), 2048)])]);
    let seen =
        native_fixture_with_preloaded(listener, ChatReply::Correct, stop_rx, None, preloaded).await;
    let result = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            policy.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(!result.status.success());
    let outcome: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(outcome["status"], "failed");
    assert!(outcome["answer"]
        .as_str()
        .unwrap()
        .contains("adoption is refused"));
    let observations = seen.lock().unwrap();
    assert!(observations.loads.is_empty());
    assert_eq!(observations.chats, 0);
    assert_eq!(observations.unloads, 0);
    drop(observations);
    let _ = stop_tx.send(());
}

#[tokio::test]
async fn lazy_native_releases_only_its_newly_loaded_models_after_confirmed_work() {
    let home = root();
    config(&home, 120).save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy = write_lazy_policy(&home, &api_base, true, &["alpha", "beta"]);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;
    let result = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            policy.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let observations = seen.lock().unwrap();
    assert_eq!(observations.loads.get("alpha"), Some(&1));
    assert_eq!(observations.loads.get("beta"), None);
    assert_eq!(observations.chats, 1);
    assert_eq!(observations.unloads, 1);
    drop(observations);
    let _ = stop_tx.send(());
}

#[tokio::test]
async fn native_gpu_placement_requires_matching_echo_and_fresh_inventory() {
    for scenario in 0..4 {
        let home = root();
        let cfg = config(&home, 120);
        cfg.save().unwrap();
        let store = Store::open(&cfg.db()).unwrap();
        store::bootstrap(&store).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let requested = scenario != 3;
        let server = tokio::spawn(async move {
            let steps = if scenario == 1 { 2 } else { 3 };
            for step in 0..steps {
                let (mut stream, _) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let (head, body) = http_request(&mut stream).await;
                let value = if step == 1 {
                    assert!(head.starts_with("POST /api/v1/models/load "));
                    assert_eq!(body["offload_kv_cache_to_gpu"], requested);
                    assert_eq!(body["echo_load_config"], true);
                    json!({"status":"loaded","type":"llm","instance_id":"gpu-fixture","load_time_seconds":0.01,
                        "load_config":{"offload_kv_cache_to_gpu":if scenario == 1 {!requested} else {requested}}})
                } else {
                    assert!(head.starts_with("GET /api/v1/models "));
                    let instances = if step == 0 {
                        vec![]
                    } else {
                        vec![json!({"id":"gpu-fixture","config":{"context_length":2048,
                            "offload_kv_cache_to_gpu":if scenario == 2 {!requested} else {requested}}})]
                    };
                    json!({"models":[{"key":"alpha","type":"llm","loaded_instances":instances}]})
                };
                response(&mut stream, value).await;
            }
        });
        let loaded = cogfabric::residency::managed::load_with_gpu(
            &store,
            &base,
            false,
            "alpha",
            2048,
            Some(requested),
            true,
        )
        .await;
        if matches!(scenario, 0 | 3) {
            let report = loaded.unwrap();
            assert_eq!(report["receipt"]["format_version"], 2);
            assert_eq!(report["receipt"]["offload_kv_cache_to_gpu"], requested);
            assert_eq!(report["physical_residency_attested"], false);
            assert_eq!(report["generation_tested"], false);
        } else {
            assert!(loaded.is_err());
        }
        let ledger = cogfabric::residency::managed::list(&store, &base).unwrap();
        assert_eq!(
            ledger["managed_lifecycle_blocked"],
            matches!(scenario, 1 | 2)
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn lazy_native_deterministic_task_activates_zero_models() {
    let home = root();
    let cfg = config(&home, 120);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let path = write_lazy_policy(&home, &api_base, false, &["alpha", "beta"]);
    let policy = NativeLifecyclePolicy::load(&path).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;
    let output = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            path.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "calc: 6*7",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["answer"], "42");
    assert_eq!(seen.lock().unwrap().chats, 0);
    assert!(seen.lock().unwrap().loads.is_empty());
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let task = result["task_id"].as_str().unwrap();
    store.finish_task(task, "interrupted", "").unwrap();
    let report = cogfabric::residency::lifecycle::restoration::inspect(store, task, &policy)
        .await
        .unwrap();
    assert_eq!(report["receipt_count"], 0);
    let _ = stop_tx.send(());
}

#[tokio::test]
async fn lazy_native_resource_profile_denial_precedes_model_load() {
    let home = root();
    let mut cfg = config(&home, 120);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    cfg.resource_admission = Some(
        serde_json::from_value(json!({
            "settings":{"label":"lazy.native.resource","capacity":{"compute_slots":1,"host_memory_bytes":100,"device_memory_bytes":100},"max_pending":4,"max_wait_ms":5000,"max_execution_ms":5000},
            "allocation":{"compute_slots":1,"host_memory_bytes":20,"device_memory_bytes":20},
            "residents":[],
            "working_profiles":[{"id":"beta-only","target":{"api_base":api_base.clone(),"model":"beta"},"roles":["solver"],"allocation":{"compute_slots":1,"host_memory_bytes":20,"device_memory_bytes":20}}]
        }))
        .unwrap(),
    );
    cfg.save().unwrap();
    let path = write_lazy_policy(&home, &api_base, false, &["alpha", "beta"]);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;
    let output = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            path.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(!output.status.success());
    assert!(
        seen.lock()
            .unwrap()
            .requests
            .iter()
            .all(|path| !path.starts_with("POST /api/v1/models/load ")),
        "working-profile refusal must happen before model mutation"
    );
    assert_eq!(seen.lock().unwrap().chats, 0);
    let _ = stop_tx.send(());
}

#[tokio::test]
async fn cli_serve_preserves_options_and_progress_and_does_not_release_reused_receipt() {
    let home = root();
    let cfg = config(&home, 15);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy = write_policy(&home, &api_base, true);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;

    // The managed load command expects the application's schema to exist.
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    store::bootstrap(&store).unwrap();

    // Create a durable, owned instance before starting Serve. Automatic cleanup
    // must leave this pre-existing receipt resident even when release_after is on.
    let load = cli(
        &home,
        &[
            "residency-managed-load",
            "--api-base",
            &api_base,
            "--model",
            "alpha",
            "--context-length",
            "2048",
            "--human-approve",
        ],
    )
    .await;
    assert!(
        load.status.success(),
        "{}",
        String::from_utf8_lossy(&load.stderr)
    );
    let load: Value = serde_json::from_slice(&load.stdout).unwrap();
    let receipt_id = load["receipt"]["receipt_id"].as_str().unwrap().to_owned();

    let mut child = ChildGuard(
        std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .env_clear()
            .arg("--home")
            .arg(&home)
            .args([
                "--backend",
                "openai-local",
                "--json",
                "--native-lifecycle-policy",
            ])
            .arg(&policy)
            .args(["--approve-native-lifecycle", "serve"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for_socket(&cfg, &mut child.0).await;
    let conversation = store
        .create_task("Earlier conversation context", "fast")
        .unwrap();
    store
        .finish_task(
            &conversation,
            "unverified",
            "Historical context is untrusted",
        )
        .unwrap();
    let ask = cli(
        &home,
        &[
            "ask",
            "Say hello",
            "--mode",
            "fast",
            "--socket",
            "--conversation",
            &conversation,
            "--explain",
        ],
    )
    .await;
    assert!(
        ask.status.success(),
        "{}",
        String::from_utf8_lossy(&ask.stderr)
    );
    let result: Value = serde_json::from_slice(&ask.stdout).unwrap();
    let task_id = result["task_id"].as_str().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(task_id).unwrap();
    let options = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "execution_options")
        .unwrap();
    assert_eq!(options["payload"]["explain"], true);
    assert_eq!(options["payload"]["conversation_ref"], conversation);
    assert_eq!(trace["task"]["id"], task_id);
    assert!(!service::task_progress(&cfg, task_id, 0)
        .await
        .unwrap()
        .events
        .is_empty());
    assert_eq!(seen.lock().unwrap().unloads, 0);
    let rows = cogfabric::residency::managed::list(&store, &api_base).unwrap();
    assert_eq!(rows["receipts"][0]["receipt"]["receipt_id"], receipt_id);
    assert_eq!(rows["receipts"][0]["state"], "active");

    let _ = child.0.kill();
    let _ = child.0.wait();
    let _ = stop_tx.send(());
    tokio::time::sleep(Duration::from_millis(120)).await;
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn wrong_response_instance_quarantines_generation_and_blocks_replay_and_cleanup() {
    wrong_response_instance_case(false).await;
}

#[tokio::test]
async fn lazy_wrong_response_instance_quarantines_generation_and_blocks_replay_and_cleanup() {
    wrong_response_instance_case(true).await;
}

async fn wrong_response_instance_case(lazy: bool) {
    let home = root();
    let cfg = config(&home, 10);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy = if lazy {
        write_lazy_policy(&home, &api_base, true, &["alpha", "beta"])
    } else {
        write_policy(&home, &api_base, true)
    };
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::WrongInstance, stop_rx, None).await;

    let first = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            policy.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(!first.status.success());
    let outcome: Value = serde_json::from_slice(&first.stdout).unwrap();
    let task_id = outcome["task_id"].as_str().unwrap();
    assert_eq!(outcome["status"], "failed");
    assert_eq!(seen.lock().unwrap().unloads, 0);

    let retry = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            policy.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "Say hello again",
            "--mode",
            "fast",
        ],
    )
    .await;
    assert!(
        !retry.status.success(),
        "unknown dispatch must block replay"
    );
    assert!(String::from_utf8_lossy(&retry.stderr)
        .to_ascii_lowercase()
        .contains("unknown"));
    assert_eq!(seen.lock().unwrap().chats, 1);
    assert_eq!(seen.lock().unwrap().unloads, 0);
    let store = Store::open(&cfg.db()).unwrap();
    let lifecycle_id: String = rusqlite::Connection::open(cfg.db()).unwrap().query_row(
        "SELECT DISTINCT task_id FROM events WHERE kind='native_task_lifecycle' AND json_extract(payload,'$.child_task_id')=?1",
        [task_id], |row| row.get(0),
    ).unwrap();
    let trace = store.task(&lifecycle_id).unwrap();
    assert!(trace["events"].as_array().unwrap().iter().any(|event| {
        event["kind"] == "native_task_lifecycle"
            && event["payload"]["state"] == "outcome_unknown"
            && event["payload"]["child_task_id"] == task_id
    }));
    let _ = stop_tx.send(());
    tokio::time::sleep(Duration::from_millis(120)).await;
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn approval_and_changed_engine_configuration_fail_before_provider_io() {
    let outer = root();
    std::fs::create_dir_all(&outer).unwrap();
    let home = outer.join("not-created");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy = write_policy(&outer, &api_base, false);
    let denied = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .env_clear()
        .arg("--home")
        .arg(&home)
        .arg("--native-lifecycle-policy")
        .arg(&policy)
        .args(["--backend", "openai-local", "--json", "ask", "Say hello"])
        .output()
        .await
        .unwrap();
    assert!(!denied.status.success());
    assert!(
        !home.exists(),
        "approval gate must precede state initialization"
    );

    let cfg = config(&outer, 10);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    store::bootstrap(&store).unwrap();
    let policy_data = NativeLifecyclePolicy::load(&policy).unwrap();
    let mut engine = Engine::new(cfg.clone(), store);
    engine = engine
        .with_approved_native_lifecycle(policy_data, true)
        .unwrap();
    let invalid_profile = engine.run_with_options(
        "Say hello",
        "fast",
        cogfabric::language::TaskOptions {
            project_ref: Some("missing.project@1".into()),
            ..Default::default()
        },
        std::future::pending(),
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(1), invalid_profile)
            .await
            .expect("Unknown project context must be refused before native provider I/O")
            .is_err()
    );
    let tasks: i64 = rusqlite::Connection::open(cfg.db())
        .unwrap()
        .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        tasks, 0,
        "Invalid project context must not create a lifecycle task"
    );
    engine.cfg.model = "changed-after-approval".into();
    assert!(engine.run("Say hello", "fast").await.is_err());
    assert!(
        !home.exists(),
        "configuration drift must not initialize another task home"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(120), listener.accept())
            .await
            .is_err(),
        "rejected policy drift must not contact the native provider"
    );
    std::fs::remove_dir_all(outer).unwrap();
}

#[tokio::test]
async fn observed_gpu_admission_requires_lazy_lifecycle_before_any_provider_io() {
    let home = root();
    let mut cfg = config(&home, 10);
    cfg.resource_admission = Some(serde_json::from_value(json!({
        "settings":{"label":"gpu.observed","capacity":{"compute_slots":1,"host_memory_bytes":100,"device_memory_bytes":100},"max_pending":1,"max_wait_ms":100,"max_execution_ms":100},
        "allocation":{"compute_slots":1,"host_memory_bytes":1,"device_memory_bytes":1},
        "gpu_observation":{"device_uuid":"GPU-abcd","sensor":"nvidia-smi","headroom_bytes":0}
    })).unwrap());
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    store::bootstrap(&store).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let mut policy = NativeLifecyclePolicy::load(write_policy(&home, &base, false)).unwrap();
    assert!(Engine::new(cfg.clone(), store.clone())
        .with_approved_native_lifecycle(policy.clone(), true)
        .is_err());
    let request = cogfabric::residency::lifecycle::Request {
        format_version: 1,
        query: "Say hello".into(),
        mode: "fast".into(),
        api_base: base,
        authenticate: false,
        models: policy.models.clone(),
        release_after: false,
        typed_module_worker: false,
        max_lifecycle_seconds: 10,
    };
    assert!(
        cogfabric::residency::lifecycle::run(cfg.clone(), store.clone(), request, true)
            .await
            .is_err()
    );
    policy.format_version = 2;
    policy.models[0].offload_kv_cache_to_gpu = Some(true);
    assert!(Engine::new(cfg.clone(), store.clone())
        .with_approved_native_lifecycle(policy.clone(), true)
        .is_err());
    policy.format_version = 3;
    Engine::new(cfg, store)
        .with_approved_native_lifecycle(policy, true)
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn pre_cancelled_engine_task_creates_no_lifecycle_or_child_tasks() {
    let home = root();
    let cfg = config(&home, 10);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    store::bootstrap(&store).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy_path = write_policy(&home, &api_base, true);
    let policy = NativeLifecyclePolicy::load(&policy_path).unwrap();
    let engine = Engine::new(cfg.clone(), store.clone())
        .with_approved_native_lifecycle(policy, true)
        .unwrap();

    assert!(engine
        .run_until("Say hello", "fast", std::future::ready(()))
        .await
        .is_err());
    let task_count: i64 = rusqlite::Connection::open(cfg.db())
        .unwrap()
        .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        task_count, 0,
        "pre-cancellation must precede lifecycle admission"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "pre-cancellation must precede provider I/O"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn cancellation_and_original_deadline_abort_a_stalled_native_load_without_chat_or_unload() {
    for lazy in [false, true] {
        for deadline_case in [false, true] {
            let home = root();
            let cfg = config(&home, if deadline_case { 2 } else { 10 });
            cfg.save().unwrap();
            let store = Arc::new(Store::open(&cfg.db()).unwrap());
            store::bootstrap(&store).unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
            let policy_path = if lazy {
                write_lazy_policy(&home, &api_base, true, &["alpha", "beta"])
            } else {
                write_policy(&home, &api_base, true)
            };
            let policy = NativeLifecyclePolicy::load(&policy_path).unwrap();
            let (load_tx, load_rx) = tokio::sync::oneshot::channel();
            let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
            let seen = native_fixture(listener, ChatReply::Correct, stop_rx, Some(load_tx)).await;
            let engine = Engine::new(cfg.clone(), store.clone())
                .with_approved_native_lifecycle(policy, true)
                .unwrap();
            let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                engine
                    .run_until("Say hello", "fast", async move {
                        if deadline_case {
                            std::future::pending::<()>().await;
                        } else {
                            let _ = cancel_rx.await;
                        }
                    })
                    .await
            });
            tokio::time::timeout(Duration::from_secs(5), load_rx)
                .await
                .expect("native load request did not start")
                .expect("load fixture notification remains live");
            if deadline_case {
                let result = tokio::time::timeout(Duration::from_secs(5), task)
                    .await
                    .expect("original task deadline did not stop stalled load")
                    .unwrap();
                if let Ok(outcome) = result {
                    assert!(lazy, "Eager setup interruption must return an error");
                    assert!(matches!(
                        outcome.status.as_str(),
                        "cancelled" | "exhausted" | "failed"
                    ));
                }
            } else {
                cancel_tx.send(()).unwrap();
                if let Ok(outcome) = task.await.unwrap() {
                    assert!(lazy, "Eager setup interruption must return an error");
                    assert!(matches!(
                        outcome.status.as_str(),
                        "cancelled" | "exhausted" | "failed"
                    ));
                }
            }
            assert_eq!(seen.lock().unwrap().chats, 0);
            assert_eq!(seen.lock().unwrap().unloads, 0);
            if lazy && !deadline_case {
                let loads_before = seen
                    .lock()
                    .unwrap()
                    .requests
                    .iter()
                    .filter(|path| path.starts_with("POST /api/v1/models/load "))
                    .count();
                let retry_engine = Engine::new(cfg.clone(), store.clone())
                    .with_approved_native_lifecycle(
                        NativeLifecyclePolicy::load(&policy_path).unwrap(),
                        true,
                    )
                    .unwrap();
                assert!(retry_engine.run("Say hello again", "fast").await.is_err());
                let loads_after = seen
                    .lock()
                    .unwrap()
                    .requests
                    .iter()
                    .filter(|path| path.starts_with("POST /api/v1/models/load "))
                    .count();
                assert_eq!(loads_after, loads_before, "unresolved load must not retry");
            }
            let _ = stop_tx.send(());
            tokio::time::sleep(Duration::from_millis(120)).await;
            std::fs::remove_dir_all(home).unwrap();
        }
    }
}

#[tokio::test]
async fn service_reuses_native_policy_with_a_fresh_owned_worker_registry_per_task() {
    let home = root();
    let cfg = config(&home, 15);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy = write_policy(&home, &api_base, false);
    let mut policy_json: Value = serde_json::from_slice(&std::fs::read(&policy).unwrap()).unwrap();
    policy_json["typed_module_worker"] = json!(true);
    std::fs::write(&policy, serde_json::to_vec(&policy_json).unwrap()).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Correct, stop_rx, None).await;
    let mut child = ChildGuard(
        std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .env_clear()
            .arg("--home")
            .arg(&home)
            .args([
                "--backend",
                "openai-local",
                "--json",
                "--native-lifecycle-policy",
            ])
            .arg(&policy)
            .args(["--approve-native-lifecycle", "serve"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for_socket(&cfg, &mut child.0).await;
    let mut registries = std::collections::BTreeSet::new();
    for (query, answer, status) in [
        ("Say hello", "Hello from the local model.", "unverified"),
        (
            "Say hello again",
            "Hello from the local model.",
            "unverified",
        ),
        ("calc: 6*7", "42", "completed"),
    ] {
        let response = cli(&home, &["ask", query, "--mode", "fast", "--socket"]).await;
        assert!(
            response.status.success(),
            "{}",
            String::from_utf8_lossy(&response.stderr)
        );
        let response: Value = serde_json::from_slice(&response.stdout).unwrap();
        assert_eq!(response["answer"], answer);
        assert_eq!(response["status"], status);
        let store = Store::open(&cfg.db()).unwrap();
        let trace = store.task(response["task_id"].as_str().unwrap()).unwrap();
        let events = trace["events"].as_array().unwrap();
        let snapshot = events
            .iter()
            .find(|event| event["kind"] == "typed_module_snapshot")
            .unwrap();
        assert!(registries.insert(
            snapshot["payload"]["broker_id"]
                .as_str()
                .unwrap()
                .to_owned()
        ));
        if query.starts_with("calc:") {
            assert!(events
                .iter()
                .any(|event| event["kind"] == "module_process_stopped"
                    && event["payload"]["reaped"] == true));
            assert_eq!(response["calls"], 0);
        }
    }
    assert_eq!(seen.lock().unwrap().chats, 2);
    assert_eq!(seen.lock().unwrap().unloads, 0);
    let _ = child.0.kill();
    let _ = child.0.wait();
    let _ = stop_tx.send(());
    tokio::time::sleep(Duration::from_millis(120)).await;
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn native_cli_continuation_restores_owned_instances_and_builtin_worker_without_loading() {
    native_continuation_case(false, false).await;
}

#[tokio::test]
async fn uncertain_native_continuation_is_resolvable_without_replaying_inference() {
    native_continuation_case(true, false).await;
}

#[tokio::test]
async fn lazy_native_continuation_restores_receipt_and_keeps_cancel_source_alive() {
    native_continuation_case(false, true).await;
}

#[tokio::test]
async fn lazy_native_resume_activates_an_approved_model_on_its_first_resumed_call() {
    let home = root();
    let mut cfg = config(&home, 120);
    cfg.role_profiles.insert(
        "planner".into(),
        cogfabric::config::RoleProfile {
            model: "alpha".into(),
            temperature: 0.0,
        },
    );
    cfg.role_profiles.insert(
        "solver".into(),
        cogfabric::config::RoleProfile {
            model: "beta".into(),
            temperature: 0.0,
        },
    );
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let path = write_lazy_policy(&home, &api_base, false, &["alpha", "beta"]);
    let policy = NativeLifecyclePolicy::load(&path).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(listener, ChatReply::Sorted, stop_rx, None).await;
    let initial = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            path.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            "sort: {\"values\":[3,1,2]}",
            "--mode",
            "standard",
        ],
    )
    .await;
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let initial: Value = serde_json::from_slice(&initial.stdout).unwrap();
    let task = initial["task_id"].as_str().unwrap();
    assert_eq!(seen.lock().unwrap().loads.get("alpha"), Some(&1));
    assert_eq!(seen.lock().unwrap().loads.get("beta"), Some(&1));

    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let parent: String = db
        .query_row(
            "SELECT task_id FROM events WHERE kind='native_task_bindings' AND json_extract(payload,'$.child_task_id')=?1",
            [task],
            |row| row.get(0),
        )
        .unwrap();
    let beta_receipt = {
        let mut statement = db
            .prepare("SELECT receipt_id,receipt_json FROM managed_residency_receipts WHERE endpoint=?1 AND state='active'")
            .unwrap();
        let receipt_id = statement
            .query_map([format!("{api_base}/models")], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .find_map(|(id, raw)| {
                let receipt: Value = serde_json::from_str(&raw).ok()?;
                (receipt["model_key"] == "beta").then_some(id)
            })
            .unwrap();
        receipt_id
    };
    cogfabric::residency::managed::release(&store, &api_base, false, &beta_receipt, true)
        .await
        .unwrap();
    let beta = db
        .query_row(
            "SELECT min(id) FROM events WHERE task_id=?1 AND kind='native_task_lifecycle' AND json_extract(payload,'$.state')='generation_confirmed' AND json_extract(payload,'$.model_key')='beta'",
            [&parent],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    let alpha_confirmed = db
        .query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='native_task_lifecycle' AND json_extract(payload,'$.state')='generation_confirmed' AND json_extract(payload,'$.model_key')='alpha'",
            [&parent],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    let boundary: i64 = db
        .query_row(
            "SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary'",
            [task],
            |row| row.get(0),
        )
        .unwrap();
    assert!(alpha_confirmed < beta);
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, boundary],
    )
    .unwrap();
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND ((kind='native_task_model_binding' AND json_extract(payload,'$.model_key')='beta') OR (kind='native_task_lifecycle' AND (json_extract(payload,'$.model_key')='beta' OR json_extract(payload,'$.state')='execution_finished')))",
        [&parent],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [task],
    )
    .unwrap();
    drop(db);

    let resumed = cli(
        &home,
        &[
            "resume-task",
            task,
            "--human-approve",
            "--native-policy",
            path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert_eq!(seen.lock().unwrap().loads.get("alpha"), Some(&1));
    assert_eq!(seen.lock().unwrap().loads.get("beta"), Some(&2));
    let trace = store.task(task).unwrap();
    assert!(trace["events"].as_array().unwrap().iter().any(|event| {
        event["kind"] == "continuation"
            && event["payload"]["state"] == "started"
            && event["payload"]["original_budget_preserved"] == true
    }));
    let report =
        cogfabric::residency::lifecycle::restoration::inspect(store.clone(), task, &policy).await;
    assert!(
        report.is_err(),
        "completed task is not resumable for inspection"
    );
    let _ = stop_tx.send(());
}

async fn native_continuation_case(fail: bool, lazy: bool) {
    let home = root();
    let mut cfg = config(&home, 120);
    cfg.resource_admission = Some(serde_json::from_value(json!({
        "settings":{"label":"native.recovery","capacity":{"compute_slots":1,"host_memory_bytes":100,"device_memory_bytes":100},"max_pending":4,"max_wait_ms":5000,"max_execution_ms":5000},
        "allocation":{"compute_slots":1,"host_memory_bytes":20,"device_memory_bytes":20},"residents":[]
    })).unwrap());
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let path = if lazy {
        write_lazy_policy(&home, &api_base, false, &["alpha", "beta"])
    } else {
        write_policy(&home, &api_base, false)
    };
    let mut policy: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    policy["typed_module_worker"] = json!(true);
    std::fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let seen = native_fixture(
        listener,
        if fail {
            ChatReply::SortedThenWrong
        } else {
            ChatReply::Sorted
        },
        stop_rx,
        None,
    )
    .await;
    let initial = cli(
        &home,
        &[
            "--native-lifecycle-policy",
            path.to_str().unwrap(),
            "--approve-native-lifecycle",
            "ask",
            r#"sort:{"values":[3,1,2]}"#,
            "--mode",
            "standard",
        ],
    )
    .await;
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let result: Value = serde_json::from_slice(&initial.stdout).unwrap();
    assert_eq!(result["status"], "completed");
    let task = result["task_id"].as_str().unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let cutoff: i64 = db
        .query_row(
            "SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary'",
            [task],
            |r| r.get(0),
        )
        .unwrap();
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, cutoff],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [task],
    )
    .unwrap();
    drop(db);
    let chats = seen.lock().unwrap().chats;
    let requests = seen.lock().unwrap().requests.len();
    let denied = cli(
        &home,
        &[
            "resume-task",
            task,
            "--native-policy",
            path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(!denied.status.success());
    assert_eq!(seen.lock().unwrap().requests.len(), requests);
    let denied = cli(&home, &["resume-task", task, "--human-approve"]).await;
    assert!(!denied.status.success());
    assert_eq!(seen.lock().unwrap().requests.len(), requests);
    let resumed = cli(
        &home,
        &[
            "resume-task",
            task,
            "--human-approve",
            "--native-policy",
            path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let report: Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(
        report["status"],
        if fail { "interrupted" } else { "completed" }
    );
    if !fail {
        assert_eq!(report["answer"], "[1,2,3]");
    }
    assert_eq!(report["new_calls"], 1, "{report}");
    assert_eq!(
        report["restored_native_runtime"]["receipt_identity_matches"],
        true
    );
    assert_eq!(seen.lock().unwrap().chats, chats + 1);
    assert_eq!(seen.lock().unwrap().unloads, 0);
    assert_eq!(
        seen.lock()
            .unwrap()
            .requests
            .iter()
            .filter(|request| request.starts_with("POST /api/v1/models/load "))
            .count(),
        1
    );
    if lazy {
        assert_eq!(seen.lock().unwrap().loads.get("beta"), None);
    }
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(task).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert!(events
        .iter()
        .any(|e| e["kind"] == "builtin_registry_reconstruction"));
    assert_eq!(
        events
            .iter()
            .any(|e| e["kind"] == "module_process_stopped" && e["payload"]["reaped"] == true),
        !fail
    );
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE kind='native_continuation_bindings'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, i64::from(!lazy));
    let generation: String = if lazy {
        let (parent, started): (String, i64) = db.query_row(
            "SELECT task_id,count(*) FROM events WHERE kind='native_task_lifecycle' AND json_extract(payload,'$.state')='generation_started' AND json_extract(payload,'$.child_task_id')=?1 GROUP BY task_id",
            [task], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(started, (chats + 1) as i64);
        parent
    } else {
        db.query_row(
            "SELECT task_id FROM events WHERE kind='native_continuation_bindings'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    drop(db);
    if fail {
        let native_policy = NativeLifecyclePolicy::load(&path).unwrap();
        let shared = Arc::new(store);
        let before = seen.lock().unwrap().requests.len();
        assert!(cogfabric::residency::lifecycle::restoration::inspect(
            shared.clone(),
            task,
            &native_policy
        )
        .await
        .is_err());
        assert_eq!(seen.lock().unwrap().requests.len(), before);
        let resolution = cogfabric::residency::lifecycle::resolve_generation(
            shared.clone(),
            &api_base,
            false,
            &generation,
            true,
            true,
        )
        .await
        .unwrap();
        assert_eq!(resolution["automatic_replay"], false);
        assert_eq!(resolution["provider_stop_proven"], false);
        assert_eq!(seen.lock().unwrap().chats, chats + 1);
        let retry = cli(
            &home,
            &[
                "resume-task",
                task,
                "--human-approve",
                "--native-policy",
                path.to_str().unwrap(),
            ],
        )
        .await;
        assert!(!retry.status.success());
        assert_eq!(seen.lock().unwrap().chats, chats + 1);
        drop(shared);
    } else {
        drop(store);
    }
    let _ = stop_tx.send(());
    tokio::time::sleep(Duration::from_millis(120)).await;
    std::fs::remove_dir_all(home).unwrap();
}
