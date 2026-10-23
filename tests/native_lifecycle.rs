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

#[derive(Clone, Copy)]
enum ChatReply {
    Correct,
    WrongInstance,
}

#[derive(Default)]
struct Seen {
    requests: Vec<String>,
    unloads: usize,
    chats: usize,
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
    let seen = Arc::new(Mutex::new(Seen::default()));
    let server_seen = seen.clone();
    tokio::spawn(async move {
        let mut stop = Box::pin(stop);
        let mut stalled_load = stalled_load;
        let mut loaded = BTreeMap::<String, Vec<(String, u64)>>::new();
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
                let inventory = json!({"models":[{
                    "key":"alpha","type":"llm","loaded_instances":loaded.get("alpha").cloned().unwrap_or_default().into_iter().map(|(id,context_length)|json!({"id":id,"config":{"context_length":context_length}})).collect::<Vec<_>>()
                }]});
                response(&mut stream, inventory).await;
            } else if path.starts_with("POST /api/v1/models/load ") {
                if let Some(sender) = stalled_load.take() {
                    let _ = sender.send(());
                    let mut byte = [0u8; 1];
                    let _ = stream.read(&mut byte).await;
                    return;
                }
                let model = body["model"].as_str().unwrap().to_owned();
                let context = body["context_length"].as_u64().unwrap();
                let instance = "alpha-instance";
                loaded
                    .entry(model)
                    .or_default()
                    .push((instance.into(), context));
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
                assert_eq!(body["model"], "alpha");
                assert_eq!(body["stream"], false);
                assert_eq!(body["store"], false);
                assert_eq!(body["integrations"], json!([]));
                let instance = match reply {
                    ChatReply::Correct => "alpha-instance",
                    ChatReply::WrongInstance => "wrong-instance",
                };
                response(
                    &mut stream,
                    json!({
                        "model_instance_id":instance,
                        "output":[{"type":"message","content":"Hello from the local model."}],
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
    tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .env_clear()
        .arg("--home")
        .arg(home)
        .arg("--backend")
        .arg("openai-local")
        .arg("--json")
        .args(args)
        .output()
        .await
        .unwrap()
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
    let home = root();
    let cfg = config(&home, 10);
    cfg.save().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let policy = write_policy(&home, &api_base, true);
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
    for deadline_case in [false, true] {
        let home = root();
        let cfg = config(&home, if deadline_case { 2 } else { 10 });
        cfg.save().unwrap();
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        store::bootstrap(&store).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api_base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let policy_path = write_policy(&home, &api_base, true);
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
            assert!(result.is_err());
        } else {
            cancel_tx.send(()).unwrap();
            assert!(task.await.unwrap().is_err());
        }
        assert_eq!(seen.lock().unwrap().chats, 0);
        assert_eq!(seen.lock().unwrap().unloads, 0);
        let _ = stop_tx.send(());
        tokio::time::sleep(Duration::from_millis(120)).await;
        std::fs::remove_dir_all(home).unwrap();
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
