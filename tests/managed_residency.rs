use cogfabric::{config::Config, engine::Engine, store::Store};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::Output,
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static LOOPBACK_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    LOOPBACK_TEST_LOCK.lock().await
}

fn home() -> PathBuf {
    std::env::temp_dir().join(format!("fabric-managed-residency-{}", uuid::Uuid::new_v4()))
}

fn create_home(path: &Path) -> Config {
    let cfg = Config {
        home: path.to_path_buf(),
        ..Config::default()
    };
    cfg.save().unwrap();
    Store::open(&cfg.db()).unwrap();
    cfg
}

async fn cli(home: &Path, args: impl IntoIterator<Item = String>) -> Output {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .env_clear()
        .arg("--home")
        .arg(home)
        .arg("--json")
        .args(args)
        .output()
        .await
        .unwrap()
}

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

fn base(listener: &tokio::net::TcpListener) -> String {
    format!("http://{}/api/v1", listener.local_addr().unwrap())
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, Value) {
    let mut bytes = Vec::new();
    loop {
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .map(|value| value.trim().parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                let head = String::from_utf8_lossy(&bytes[..end]).to_string();
                let request = if length == 0 {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap()
                };
                return (head, request);
            }
        }
        let mut chunk = [0u8; 2048];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0, "provider closed before a complete HTTP request");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() < 32768, "HTTP request exceeds fixture bounds");
    }
}

async fn exchange(
    listener: &tokio::net::TcpListener,
    expected: &str,
    status: &str,
    body: Value,
) -> Value {
    let (mut stream, _) = listener.accept().await.unwrap();
    let (head, request) = read_request(&mut stream).await;
    assert!(
        head.starts_with(expected),
        "unexpected provider request: {head}"
    );
    let body = body.to_string();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    request
}

async fn no_request(listener: &tokio::net::TcpListener) {
    assert!(
        tokio::time::timeout(Duration::from_millis(120), listener.accept())
            .await
            .is_err(),
        "operation unexpectedly contacted the provider"
    );
}

fn models(instances: &BTreeMap<String, Vec<(String, u64)>>) -> Value {
    json!({"models":[
        {"key":"alpha","type":"llm","loaded_instances":instances.get("alpha").cloned().unwrap_or_default().into_iter().map(|(id,context_length)|json!({"id":id,"config":{"context_length":context_length}})).collect::<Vec<_>>()},
        {"key":"beta","type":"llm","loaded_instances":instances.get("beta").cloned().unwrap_or_default().into_iter().map(|(id,context_length)|json!({"id":id,"config":{"context_length":context_length}})).collect::<Vec<_>>()}
    ]})
}

fn audit_post(db_path: &Path, body: &Value, expected_receipt: Option<&str>) {
    let db = rusqlite::Connection::open(db_path).unwrap();
    let pending: i64 = db
        .query_row(
            "SELECT count(*) FROM events e JOIN tasks t ON t.id=e.task_id WHERE e.kind='model_residency' AND e.source='human-cli' AND json_extract(e.payload,'$.managed')=1 AND json_extract(e.payload,'$.state')='requested' AND t.status='running'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(pending >= 1, "managed request intent must precede POST");
    if let Some(receipt_id) = expected_receipt {
        let state: String = db
            .query_row(
                "SELECT state FROM managed_residency_receipts WHERE receipt_id=?1",
                [receipt_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "release_requested");
        assert!(body["instance_id"].as_str().is_some());
    }
}

#[derive(Debug)]
struct ReceiptRow {
    receipt_id: String,
    instance_id: String,
    state: String,
    current_digest: String,
    load_task_id: String,
    release_task_id: Option<String>,
}

fn receipts(db_path: &Path) -> Vec<ReceiptRow> {
    let db = rusqlite::Connection::open(db_path).unwrap();
    let mut statement = db
        .prepare("SELECT receipt_id,instance_id,state,current_inventory_sha256,load_task_id,release_task_id FROM managed_residency_receipts ORDER BY created,receipt_id")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok(ReceiptRow {
                receipt_id: row.get(0)?,
                instance_id: row.get(1)?,
                state: row.get(2)?,
                current_digest: row.get(3)?,
                load_task_id: row.get(4)?,
                release_task_id: row.get(5)?,
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn managed_command(action: &str, api_base: &str, model_or_receipt: &str) -> Vec<String> {
    if action == "load" {
        vec![
            "residency-managed-load".into(),
            "--api-base".into(),
            api_base.into(),
            "--model".into(),
            model_or_receipt.into(),
            "--context-length".into(),
            "2048".into(),
            "--human-approve".into(),
        ]
    } else {
        vec![
            "residency-managed-release".into(),
            "--api-base".into(),
            api_base.into(),
            "--receipt-id".into(),
            model_or_receipt.into(),
            "--human-approve".into(),
        ]
    }
}

#[tokio::test]
async fn two_managed_loads_release_and_provider_id_reuse_advance_the_exact_cohort_digest() {
    let _serial = serial().await;
    let home = home();
    let cfg = create_home(&home);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = base(&listener);
    let server_db = cfg.db();
    let server = tokio::spawn(async move {
        let mut loaded = BTreeMap::<String, Vec<(String, u64)>>::new();
        for _ in 0..12 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (head, request) = read_request(&mut stream).await;
            if head.starts_with("GET /api/v1/models ") {
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else if head.starts_with("POST /api/v1/models/load ") {
                audit_post(&server_db, &request, None);
                let model = request["model"].as_str().unwrap().to_owned();
                let context = request["context_length"].as_u64().unwrap();
                let id = if model == "alpha" {
                    "alpha-instance"
                } else {
                    "beta-instance"
                };
                loaded
                    .entry(model.clone())
                    .or_default()
                    .push((id.to_owned(), context));
                let body = json!({"status":"loaded","type":"llm","instance_id":id,"load_time_seconds":0.1}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else if head.starts_with("POST /api/v1/models/unload ") {
                let id = request["instance_id"].as_str().unwrap();
                let receipt = receipts(&server_db)
                    .into_iter()
                    .find(|row| row.instance_id == id && row.state == "release_requested")
                    .unwrap();
                audit_post(&server_db, &request, Some(&receipt.receipt_id));
                for values in loaded.values_mut() {
                    values.retain(|(instance, _)| instance != id);
                }
                // Native unload responses may omit a status field. Exact post-inventory is the confirmation.
                let body = json!({"instance_id":id}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else {
                panic!("unexpected request: {head}");
            }
        }
        no_request(&listener).await;
    });

    let first = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["server_confirmed"], true);
    assert_eq!(first["ownership_established"], false);
    assert_eq!(first["weights_attested"], false);
    assert_eq!(first["physical_residency_attested"], false);
    let first_receipt = first["receipt"]["receipt_id"].as_str().unwrap().to_owned();
    assert_eq!(
        receipts(&cfg.db())
            .iter()
            .find(|row| row.receipt_id == first_receipt)
            .unwrap()
            .load_task_id,
        first["receipt"]["load_task_id"]
    );

    let second = cli(&home, managed_command("load", &api_base, "beta")).await;
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    let second_receipt = second["receipt"]["receipt_id"].as_str().unwrap().to_owned();
    assert_ne!(first_receipt, second_receipt);
    let rows = receipts(&cfg.db());
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.state == "active"));
    assert!(rows
        .iter()
        .all(|row| row.current_digest == second["after_inventory_sha256"]));

    let wrong_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let wrong_base = base(&wrong_listener);
    let wrong_server = tokio::spawn(async move {
        no_request(&wrong_listener).await;
    });
    let wrong_endpoint = cli(
        &home,
        managed_command("release", &wrong_base, &first_receipt),
    )
    .await;
    assert!(!wrong_endpoint.status.success());
    wrong_server.await.unwrap();
    assert_eq!(
        receipts(&cfg.db())
            .iter()
            .find(|row| row.receipt_id == first_receipt)
            .unwrap()
            .state,
        "active"
    );

    let release = cli(&home, managed_command("release", &api_base, &first_receipt)).await;
    assert!(
        release.status.success(),
        "{}",
        String::from_utf8_lossy(&release.stderr)
    );
    let release: Value = serde_json::from_slice(&release.stdout).unwrap();
    assert_eq!(release["server_confirmed"], true);
    assert_eq!(release["receipt_id"], first_receipt);
    let rows = receipts(&cfg.db());
    let released = rows
        .iter()
        .find(|row| row.receipt_id == first_receipt)
        .unwrap();
    assert_eq!(released.state, "released");
    assert!(released.release_task_id.is_some());
    let remaining = rows
        .iter()
        .find(|row| row.receipt_id == second_receipt)
        .unwrap();
    assert_eq!(remaining.state, "active");
    assert_eq!(remaining.current_digest, release["after_inventory_sha256"]);

    // A later managed load may receive the same provider instance ID, but it must
    // create a fresh immutable receipt UUID after the original receipt is released.
    let reused = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(
        reused.status.success(),
        "{}",
        String::from_utf8_lossy(&reused.stderr)
    );
    let reused: Value = serde_json::from_slice(&reused.stdout).unwrap();
    assert_eq!(reused["receipt"]["instance_id"], "alpha-instance");
    assert_ne!(reused["receipt"]["receipt_id"], first_receipt);
    let rows = receipts(&cfg.db());
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .filter(|row| row.state == "active")
        .all(|row| row.current_digest == reused["after_inventory_sha256"]));
    server.await.unwrap();
    drop(Store::open(&cfg.db()).unwrap());
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn dropping_pending_managed_operations_preserves_quarantine_and_blocks_retries() {
    let _serial = serial().await;

    for release_stage in [false, true] {
        let home = home();
        let cfg = create_home(&home);
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api_base = base(&listener);

        let mut receipt_id = None;

        let (request_seen_tx, request_seen_rx) = tokio::sync::oneshot::channel();
        let (monitor_tx, monitor_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut loaded = BTreeMap::<String, Vec<(String, u64)>>::new();
            if release_stage {
                for step in 0..3 {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let (head, request) = read_request(&mut stream).await;
                    if step == 1 {
                        assert!(head.starts_with("POST /api/v1/models/load "));
                        assert_eq!(request["model"], "alpha");
                        loaded.insert(
                            "alpha".into(),
                            vec![("cancel-release-instance".into(), 2048)],
                        );
                        let body = json!({"status":"loaded","type":"llm","instance_id":"cancel-release-instance","load_time_seconds":0.1}).to_string();
                        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                    } else {
                        assert!(head.starts_with("GET /api/v1/models "));
                        let body = models(&loaded).to_string();
                        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                    }
                }
                loaded.insert(
                    "alpha".into(),
                    vec![("cancel-release-instance".into(), 2048)],
                );
                let (mut stream, _) = listener.accept().await.unwrap();
                let (head, _) = read_request(&mut stream).await;
                assert!(head.starts_with("GET /api/v1/models "));
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (head, _) = read_request(&mut stream).await;
                assert!(head.starts_with("GET /api/v1/models "));
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            let (mut stream, _) = listener.accept().await.unwrap();
            let (head, request) = read_request(&mut stream).await;
            if release_stage {
                assert!(head.starts_with("POST /api/v1/models/unload "));
                assert_eq!(request["instance_id"], "cancel-release-instance");
            } else {
                assert!(head.starts_with("POST /api/v1/models/load "));
                assert_eq!(request["model"], "alpha");
            }
            request_seen_tx.send(()).unwrap();
            let mut byte = [0u8; 1];
            assert_eq!(
                stream.read(&mut byte).await.unwrap(),
                0,
                "aborting the operation must close its provider connection"
            );
            monitor_rx
                .await
                .expect("test completes local refusal checks");
            no_request(&listener).await;
        });

        if release_stage {
            let seeded = cli(&home, managed_command("load", &api_base, "alpha")).await;
            assert!(
                seeded.status.success(),
                "{}",
                String::from_utf8_lossy(&seeded.stderr)
            );
            let seeded: Value = serde_json::from_slice(&seeded.stdout).unwrap();
            receipt_id = Some(seeded["receipt"]["receipt_id"].as_str().unwrap().to_owned());
        }

        let operation_store = store.clone();
        let operation_base = api_base.clone();
        let operation_receipt = receipt_id.clone();
        let operation = tokio::spawn(async move {
            if let Some(receipt_id) = operation_receipt {
                cogfabric::residency::managed::release(
                    &operation_store,
                    &operation_base,
                    false,
                    &receipt_id,
                    true,
                )
                .await
            } else {
                cogfabric::residency::managed::load(
                    &operation_store,
                    &operation_base,
                    false,
                    "alpha",
                    2048,
                    true,
                )
                .await
            }
        });

        tokio::time::timeout(Duration::from_secs(5), request_seen_rx)
            .await
            .expect("provider must receive mutation request before cancellation")
            .expect("fixture barrier sender remains live");
        operation.abort();
        assert!(operation.await.unwrap_err().is_cancelled());
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        let task: (String, String, i64) = db
            .query_row(
                "SELECT t.id,t.status,count(e.id) FROM tasks t JOIN events e ON e.task_id=t.id WHERE e.kind='model_residency' AND e.source='human-cli' AND json_extract(e.payload,'$.managed')=1 AND json_extract(e.payload,'$.state')='requested' GROUP BY t.id,t.status ORDER BY max(e.id) DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            task.1, "running",
            "cancelled provider operation stays pending"
        );
        assert_eq!(
            task.2, 1,
            "cancellation must not synthesize a completion or retry event"
        );
        if release_stage {
            let receipt = receipts(&cfg.db())
                .into_iter()
                .find(|row| Some(row.receipt_id.clone()) == receipt_id)
                .unwrap();
            assert_eq!(receipt.state, "release_requested");
            assert_eq!(receipt.release_task_id.as_deref(), Some(task.0.as_str()));
            let task_receipts: i64 = db
                .query_row(
                    "SELECT count(*) FROM managed_residency_receipts WHERE release_task_id=?1",
                    [&task.0],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                task_receipts, 1,
                "release cancellation retains its durable receipt association"
            );
        } else {
            let receipt_count: i64 = db
                .query_row(
                    "SELECT count(*) FROM managed_residency_receipts WHERE load_task_id=?1",
                    [&task.0],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(receipt_count, 0, "cancelled load must not create a receipt");
        }
        drop(db);

        let blocked_load = tokio::time::timeout(
            Duration::from_millis(250),
            cogfabric::residency::managed::load(&store, &api_base, false, "beta", 2048, true),
        )
        .await
        .expect("blocked load refusal must be local and prompt");
        assert!(
            blocked_load.is_err(),
            "durable pending work blocks a later load"
        );
        if let Some(receipt_id) = receipt_id {
            let blocked_release = tokio::time::timeout(
                Duration::from_millis(250),
                cogfabric::residency::managed::release(&store, &api_base, false, &receipt_id, true),
            )
            .await
            .expect("release-requested receipt refusal must be local and prompt");
            assert!(
                blocked_release.is_err(),
                "release-requested receipt cannot be retried"
            );
        }
        let local_status = cogfabric::residency::managed::list(&store, &api_base).unwrap();
        assert_eq!(local_status["unresolved_provider_operation"], true);
        assert_eq!(local_status["managed_lifecycle_blocked"], true);
        assert_eq!(local_status["server_contacted"], false);
        monitor_tx.send(()).unwrap();
        server.await.unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[tokio::test]
async fn approval_existing_database_and_unowned_inventory_gates_precede_mutation() {
    let _serial = serial().await;
    let missing_home = home();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = base(&listener);
    let denied = cli(
        &missing_home,
        vec![
            "residency-managed-load".into(),
            "--api-base".into(),
            api_base.clone(),
            "--model".into(),
            "alpha".into(),
        ],
    )
    .await;
    assert!(!denied.status.success());
    assert!(
        !missing_home.exists(),
        "approval must be checked before config/bootstrap"
    );
    no_request(&listener).await;

    let no_db_home = home();
    let cfg = Config {
        home: no_db_home.clone(),
        ..Config::default()
    };
    cfg.save().unwrap();
    let missing_db = cli(
        &no_db_home,
        vec![
            "residency-managed-status".into(),
            "--api-base".into(),
            api_base.clone(),
        ],
    )
    .await;
    assert!(!missing_db.status.success());
    assert!(
        !cfg.db().exists(),
        "managed status requires an existing database and must not bootstrap one"
    );
    no_request(&listener).await;

    let home = home();
    let cfg = create_home(&home);
    let listener = Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    let api_base = base(&listener);
    let provider = listener.clone();
    let server = tokio::spawn(async move {
        let body = json!({"models":[{"key":"alpha","type":"llm","loaded_instances":[{"id":"foreign-instance","config":{"context_length":2048}}]},{"key":"beta","type":"llm","loaded_instances":[]}]});
        exchange(&provider, "GET /api/v1/models ", "200 OK", body).await;
        assert!(
            tokio::time::timeout(Duration::from_secs(2), provider.accept())
                .await
                .is_err(),
            "unowned load or receipt lookup must not dispatch another provider request"
        );
    });
    let rejected = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(!rejected.status.success());
    assert!(
        receipts(&cfg.db()).is_empty(),
        "foreign inventory must not be adopted"
    );
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let task_count: i64 = db
        .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        task_count, 0,
        "preexisting provider instances must be rejected before a lifecycle task"
    );
    drop(db);
    let invalid_uuid = cli(&home, managed_command("release", &api_base, "not-a-uuid")).await;
    assert!(!invalid_uuid.status.success());
    let missing_receipt = cli(
        &home,
        managed_command("release", &api_base, &uuid::Uuid::new_v4().to_string()),
    )
    .await;
    assert!(!missing_receipt.status.success());
    server.await.unwrap();
    std::fs::remove_dir_all(missing_home).unwrap_or(());
    std::fs::remove_dir_all(no_db_home).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn unknown_load_is_retained_unadopted_and_blocks_all_automatic_retries() {
    let _serial = serial().await;
    let home = home();
    let cfg = create_home(&home);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = base(&listener);
    let listener = Arc::new(listener);
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        let empty = BTreeMap::new();
        let body = models(&empty).to_string();
        let (mut stream, _) = server_listener.accept().await.unwrap();
        let (head, _) = read_request(&mut stream).await;
        assert!(head.starts_with("GET /api/v1/models "));
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let (mut stream, _) = server_listener.accept().await.unwrap();
        let (head, request) = read_request(&mut stream).await;
        assert!(head.starts_with("POST /api/v1/models/load "));
        assert_eq!(request["model"], "alpha");
        drop(stream); // Provider accepted the request but gave no outcome.
        assert!(
            tokio::time::timeout(Duration::from_secs(2), server_listener.accept())
                .await
                .is_err(),
            "pending endpoint gate must reject before HTTP"
        );
    });
    let result = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(!result.status.success());
    assert!(
        receipts(&cfg.db()).is_empty(),
        "unknown instance must not receive a receipt"
    );
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let (task, state, unknown): (String, String, i64) = db.query_row(
        "SELECT t.id,t.status,(SELECT count(*) FROM events e WHERE e.task_id=t.id AND json_extract(e.payload,'$.state')='outcome_unknown') FROM tasks t ORDER BY t.created DESC LIMIT 1",
        [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    assert_eq!(state, "interrupted");
    assert_eq!(unknown, 1);
    db.execute("UPDATE tasks SET updated=0 WHERE id=?1", [&task])
        .unwrap();
    drop(db);
    let store = Store::open(&cfg.db()).unwrap();
    let retention = store.retention_preview(unix_now(), 100).unwrap();
    assert!(
        !retention.task_ids.contains(&task),
        "unknown managed load intent must remain retained without a receipt"
    );
    drop(store);

    let status = cli(
        &home,
        vec![
            "residency-managed-status".into(),
            "--api-base".into(),
            api_base.clone(),
        ],
    )
    .await;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["unresolved_provider_operation"], true);
    assert_eq!(status["managed_lifecycle_blocked"], true);
    assert_eq!(status["server_contacted"], false);
    assert_eq!(status["model_mutation_performed"], false);
    let blocked = cli(&home, managed_command("load", &api_base, "beta")).await;
    assert!(!blocked.status.success());
    server.await.unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn malformed_historical_managed_event_fails_closed_before_provider_access() {
    let _serial = serial().await;
    let home = home();
    let cfg = create_home(&home);
    let store = Store::open(&cfg.db()).unwrap();
    let task = store
        .create_task("Malformed managed lifecycle history", "fast")
        .unwrap();
    store.finish_task(&task, "completed", "fixture").unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'model_residency','human-cli','{',unixepoch())",[&task]).unwrap();
    drop(db);
    drop(store);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = base(&listener);
    let server = tokio::spawn(async move {
        no_request(&listener).await;
    });
    let result = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(!result.status.success());
    server.await.unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let tasks: i64 = db
        .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        tasks, 1,
        "malformed history must block before creating a new operation task"
    );
    drop(db);
    std::fs::remove_dir_all(home).unwrap();
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
async fn unknown_release_quarantines_receipt_and_uuid_retry_is_refused_before_network() {
    let _serial = serial().await;
    let home = home();
    let cfg = create_home(&home);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = base(&listener);
    let server_db = cfg.db();
    let listener = Arc::new(listener);
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        let mut loaded = BTreeMap::new();
        for step in 0..5 {
            let (mut stream, _) = server_listener.accept().await.unwrap();
            let (head, request) = read_request(&mut stream).await;
            if step == 0 || step == 3 {
                assert!(head.starts_with("GET /api/v1/models "));
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else if step == 1 {
                assert!(head.starts_with("POST /api/v1/models/load "));
                loaded.insert(
                    "alpha".to_owned(),
                    vec![("managed-instance".to_owned(), 2048)],
                );
                let body = json!({"status":"loaded","type":"llm","instance_id":"managed-instance","load_time_seconds":0.1}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else if step == 2 {
                assert!(head.starts_with("GET /api/v1/models "));
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else {
                assert!(head.starts_with("POST /api/v1/models/unload "));
                let row = receipts(&server_db)
                    .into_iter()
                    .find(|row| row.state == "release_requested")
                    .unwrap();
                audit_post(&server_db, &request, Some(&row.receipt_id));
                drop(stream); // Unload may have happened; no result is safe to retry.
            }
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(2), server_listener.accept())
                .await
                .is_err(),
            "quarantined receipt must reject a release retry before HTTP"
        );
    });
    let loaded = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(
        loaded.status.success(),
        "{}",
        String::from_utf8_lossy(&loaded.stderr)
    );
    let loaded: Value = serde_json::from_slice(&loaded.stdout).unwrap();
    let receipt_id = loaded["receipt"]["receipt_id"].as_str().unwrap().to_owned();
    let release = cli(&home, managed_command("release", &api_base, &receipt_id)).await;
    assert!(!release.status.success());
    let rows = receipts(&cfg.db());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, "quarantined");
    assert!(rows[0].release_task_id.is_some());
    let retry = cli(&home, managed_command("release", &api_base, &receipt_id)).await;
    assert!(!retry.status.success());
    server.await.unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let release_tasks: i64 = db
        .query_row(
            "SELECT count(*) FROM tasks WHERE query LIKE '%release%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        release_tasks, 1,
        "quarantined release must not create a retry task"
    );
    drop(db);
    let task = rows[0].release_task_id.as_ref().unwrap();
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        exchange(
            &server_listener,
            "GET /api/v1/models ",
            "200 OK",
            models(&BTreeMap::new()),
        )
        .await;
    });
    let resolved = cli(
        &home,
        args(&[
            "residency-managed-resolve",
            "--api-base",
            &api_base,
            "--task-id",
            task,
            "--human-approve",
            "--provider-stop-acknowledged",
        ]),
    )
    .await;
    assert!(
        resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    server.await.unwrap();
    assert_eq!(receipts(&cfg.db())[0].state, "retired");
    let archived = cli(
        &home,
        args(&[
            "residency-managed-archive",
            "--receipt-id",
            &receipt_id,
            "--human-approve",
        ]),
    )
    .await;
    assert!(
        archived.status.success(),
        "{}",
        String::from_utf8_lossy(&archived.stderr)
    );
    let status = cli(
        &home,
        args(&["residency-managed-status", "--api-base", &api_base]),
    )
    .await;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["unresolved_provider_operation"], false);
    no_request(&listener).await;
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn audit_failure_before_post_rolls_back_task_and_after_post_records_unknown_without_receipt()
{
    let _serial = serial().await;
    for after_post in [false, true] {
        let home = home();
        let cfg = create_home(&home);
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        if after_post {
            db.execute_batch("CREATE TRIGGER refuse_managed_confirmation BEFORE INSERT ON events WHEN NEW.kind='model_residency' AND json_extract(NEW.payload,'$.state')='server_confirmed' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        } else {
            db.execute_batch("CREATE TRIGGER refuse_managed_intent BEFORE INSERT ON events WHEN NEW.kind='model_residency' AND json_extract(NEW.payload,'$.managed')=1 AND json_extract(NEW.payload,'$.state')='requested' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        }
        drop(db);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api_base = base(&listener);
        let server = tokio::spawn(async move {
            let initial = models(&BTreeMap::new());
            exchange(&listener, "GET /api/v1/models ", "200 OK", initial.clone()).await;
            if after_post {
                let post = exchange(&listener,"POST /api/v1/models/load ","200 OK",json!({"status":"loaded","type":"llm","instance_id":"after-audit-instance","load_time_seconds":0.1})).await;
                assert_eq!(post["model"], "alpha");
                let after = json!({"models":[{"key":"alpha","type":"llm","loaded_instances":[{"id":"after-audit-instance","config":{"context_length":2048}}]},{"key":"beta","type":"llm","loaded_instances":[]}]});
                exchange(&listener, "GET /api/v1/models ", "200 OK", after).await;
            }
            no_request(&listener).await;
        });
        let result = cli(&home, managed_command("load", &api_base, "alpha")).await;
        assert!(!result.status.success());
        server.await.unwrap();
        assert!(
            receipts(&cfg.db()).is_empty(),
            "failed completion transaction must not leave a receipt"
        );
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        let (tasks, interrupted, unknown): (i64,i64,i64) = db.query_row(
            "SELECT count(*),sum(status='interrupted'),(SELECT count(*) FROM events WHERE json_extract(payload,'$.state')='outcome_unknown') FROM tasks",
            [], |row| Ok((row.get(0)?,row.get(1).unwrap_or(0),row.get(2)?)),
        ).unwrap();
        if after_post {
            assert_eq!(tasks, 1);
            assert_eq!(interrupted, 1);
            assert_eq!(unknown, 1);
        } else {
            assert_eq!(
                tasks, 0,
                "pre-POST audit refusal must atomically roll back the task"
            );
            assert_eq!(interrupted, 0);
            assert_eq!(unknown, 0);
        }
        drop(db);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[tokio::test]
async fn managed_mutations_refuse_runtime_and_idle_resource_pool_leases_before_http() {
    let _serial = serial().await;
    let home = home();
    let mut cfg = create_home(&home);
    cfg.resource_admission = Some(
        serde_json::from_value(json!({
            "settings":{"label":"gpu.main","capacity":{"compute_slots":1,"host_memory_bytes":100,"device_memory_bytes":100},"max_pending":4,"max_wait_ms":1000,"max_execution_ms":1000},
            "allocation":{"compute_slots":1,"host_memory_bytes":20,"device_memory_bytes":20}
        }))
        .unwrap(),
    );
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let engine = Engine::new(cfg.clone(), store.clone());
    let pool = engine.configured_resource_pool().unwrap().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = base(&listener);
    let server = tokio::spawn(async move {
        no_request(&listener).await;
    });
    let idle_pool = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(
        !idle_pool.status.success(),
        "configured idle resource ownership blocks residency mutation"
    );
    drop(pool);
    drop(engine);

    let mut runtime_path = cfg.db().as_os_str().to_os_string();
    runtime_path.push(".runtime.lock");
    let runtime_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(PathBuf::from(runtime_path))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(runtime_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0,
        "test must hold the same kernel runtime lock used by active execution"
    );
    let runtime = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(
        !runtime.status.success(),
        "live runtime execution blocks residency mutation"
    );
    drop(runtime_file);
    server.await.unwrap();
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[derive(Clone, Copy)]
enum ManualAction {
    Load,
    Unload,
    Plan,
}

#[tokio::test]
async fn legacy_manual_load_unload_and_reviewed_plan_quarantine_managed_cohorts_before_post() {
    let _serial = serial().await;
    for action in [ManualAction::Load, ManualAction::Unload, ManualAction::Plan] {
        let home = home();
        let cfg = create_home(&home);
        let listener = Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
        let api_base = base(&listener);
        let server_listener = listener.clone();
        let server_db = cfg.db();
        let server = tokio::spawn(async move {
            let mut loaded = BTreeMap::<String, Vec<(String, u64)>>::new();
            for step in 0..3 {
                let (mut stream, _) = server_listener.accept().await.unwrap();
                let (head, request) = read_request(&mut stream).await;
                match step {
                    0 => {
                        assert!(head.starts_with("GET /api/v1/models "));
                        let bytes = models(&loaded).to_string();
                        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).as_bytes()).await.unwrap();
                    }
                    1 => {
                        assert!(head.starts_with("POST /api/v1/models/load "));
                        assert_eq!(request["model"], "alpha");
                        loaded.insert("alpha".into(), vec![("alpha-managed".into(), 2048)]);
                        let bytes=json!({"status":"loaded","type":"llm","instance_id":"alpha-managed","load_time_seconds":0.1}).to_string();
                        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).as_bytes()).await.unwrap();
                    }
                    _ => {
                        assert!(head.starts_with("GET /api/v1/models "));
                        let bytes = models(&loaded).to_string();
                        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).as_bytes()).await.unwrap();
                    }
                }
            }
            if matches!(action, ManualAction::Plan) {
                let (mut stream, _) = server_listener.accept().await.unwrap();
                let (head, _) = read_request(&mut stream).await;
                assert!(
                    head.starts_with("GET /api/v1/models "),
                    "plan preview is read-only"
                );
                let bytes = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).as_bytes()).await.unwrap();
                // Apply validates the reviewed starting state, then rechecks it
                // immediately before the mutation.
                for _ in 0..2 {
                    let (mut stream, _) = server_listener.accept().await.unwrap();
                    let (head, _) = read_request(&mut stream).await;
                    assert!(head.starts_with("GET /api/v1/models "));
                    let bytes = models(&loaded).to_string();
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).as_bytes()).await.unwrap();
                }
            } else {
                let (mut stream, _) = server_listener.accept().await.unwrap();
                let (head, _) = read_request(&mut stream).await;
                assert!(head.starts_with("GET /api/v1/models "));
                let bytes = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).as_bytes()).await.unwrap();
            }
            let (mut stream, _) = server_listener.accept().await.unwrap();
            let (head, request) = read_request(&mut stream).await;
            let target = if matches!(action, ManualAction::Unload) {
                "POST /api/v1/models/unload "
            } else {
                "POST /api/v1/models/load "
            };
            assert!(
                head.starts_with(target),
                "unexpected provider operation: {head}"
            );
            let db = rusqlite::Connection::open(&server_db).unwrap();
            let quarantined: i64 = db
                .query_row(
                    "SELECT count(*) FROM managed_residency_receipts WHERE state='quarantined'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let invalidation:i64=db.query_row("SELECT count(*) FROM events WHERE kind='managed_residency_audit' AND json_extract(payload,'$.state')='managed_receipts_quarantined'",[],|row|row.get(0)).unwrap();
            assert_eq!(
                quarantined, 1,
                "manual mutation must quarantine the cohort before POST"
            );
            assert_eq!(invalidation, 1, "quarantine must be audited before POST");
            drop(db);
            if matches!(action, ManualAction::Unload) {
                let id = request["instance_id"].as_str().unwrap();
                assert_eq!(id, "alpha-managed");
                loaded.get_mut("alpha").unwrap().clear();
                let bytes = json!({"instance_id":id}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).as_bytes()).await.unwrap();
            } else {
                assert_eq!(request["model"], "beta");
                let instance = if matches!(action, ManualAction::Plan) {
                    "plan-beta"
                } else {
                    "manual-beta"
                };
                loaded
                    .entry("beta".into())
                    .or_default()
                    .push((instance.into(), request["context_length"].as_u64().unwrap()));
                let bytes=json!({"status":"loaded","type":"llm","instance_id":instance,"load_time_seconds":0.1}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len()).as_bytes()).await.unwrap();
            }
            let (mut stream, _) = server_listener.accept().await.unwrap();
            let (head, _) = read_request(&mut stream).await;
            assert!(head.starts_with("GET /api/v1/models "));
            let bytes = models(&loaded).to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",
                        bytes.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            no_request(&server_listener).await;
        });

        let seed = cli(&home, managed_command("load", &api_base, "alpha")).await;
        assert!(
            seed.status.success(),
            "{}",
            String::from_utf8_lossy(&seed.stderr)
        );
        let result = match action {
            ManualAction::Load => {
                cli(
                    &home,
                    args(&["residency-load", "--api-base"]).into_iter().chain([
                        api_base.clone(),
                        "--model".into(),
                        "beta".into(),
                        "--context-length".into(),
                        "2048".into(),
                        "--human-approve".into(),
                    ]),
                )
                .await
            }
            ManualAction::Unload => {
                cli(
                    &home,
                    args(&["residency-unload", "--api-base"])
                        .into_iter()
                        .chain([
                            api_base.clone(),
                            "--instance-id".into(),
                            "alpha-managed".into(),
                            "--human-approve".into(),
                        ]),
                )
                .await
            }
            ManualAction::Plan => {
                let request_path = home.join("residency-request.json");
                let plan_path = home.join("residency-plan.json");
                std::fs::write(&request_path,br#"{"ensure_loaded":[{"model":"beta","context_length":2048}],"release_instances":[]}"#).unwrap();
                let preview = cli(
                    &home,
                    args(&["residency-plan"]).into_iter().chain([
                        request_path.to_string_lossy().into_owned(),
                        "--api-base".into(),
                        api_base.clone(),
                    ]),
                )
                .await;
                assert!(
                    preview.status.success(),
                    "{}",
                    String::from_utf8_lossy(&preview.stderr)
                );
                std::fs::write(&plan_path, preview.stdout).unwrap();
                cli(
                    &home,
                    args(&["residency-apply"]).into_iter().chain([
                        plan_path.to_string_lossy().into_owned(),
                        "--api-base".into(),
                        api_base.clone(),
                        "--human-approve".into(),
                    ]),
                )
                .await
            }
        };
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        server.await.unwrap();
        assert_eq!(receipts(&cfg.db())[0].state, "quarantined");
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[tokio::test]
async fn denied_manual_load_unload_and_plan_apply_do_not_contact_provider() {
    let _serial = serial().await;
    let home = home();
    let _cfg = create_home(&home);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = base(&listener);
    let missing_plan = home.join("missing-plan.json");
    let commands = [
        vec![
            "residency-load".into(),
            "--api-base".into(),
            api_base.clone(),
            "--model".into(),
            "alpha".into(),
        ],
        vec![
            "residency-unload".into(),
            "--api-base".into(),
            api_base.clone(),
            "--instance-id".into(),
            "alpha-instance".into(),
        ],
        vec![
            "residency-apply".into(),
            missing_plan.to_string_lossy().into_owned(),
            "--api-base".into(),
            api_base.clone(),
        ],
    ];
    for command in commands {
        assert!(!cli(&home, command).await.status.success());
    }
    no_request(&listener).await;
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn receipt_capacity_rejects_load_before_task_or_provider_but_allows_release() {
    let _serial = serial().await;
    let home = home();
    let cfg = create_home(&home);
    let listener = Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    let api_base = base(&listener);
    let provider = listener.clone();
    let (allow_release_tx, allow_release_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut loaded = BTreeMap::new();
        for step in 0..3 {
            let (mut stream, _) = provider.accept().await.unwrap();
            let (head, request) = read_request(&mut stream).await;
            if step == 0 {
                assert!(head.starts_with("GET /api/v1/models "));
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else if step == 1 {
                assert!(head.starts_with("POST /api/v1/models/load "));
                assert_eq!(request["model"], "alpha");
                loaded.insert(
                    "alpha".to_owned(),
                    vec![("capacity-instance".to_owned(), 2048)],
                );
                let body=json!({"status":"loaded","type":"llm","instance_id":"capacity-instance","load_time_seconds":0.1}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else {
                assert!(head.starts_with("GET /api/v1/models "));
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        }
        // Capacity is checked after a fresh inventory read, but before task
        // creation and before the mutating POST.
        let (mut stream, _) = provider.accept().await.unwrap();
        let (head, _) = read_request(&mut stream).await;
        assert!(head.starts_with("GET /api/v1/models "));
        let body = models(&loaded).to_string();
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        allow_release_rx
            .await
            .expect("test allows release after capacity assertions");
        for step in 0..3 {
            let (mut stream, _) = provider.accept().await.unwrap();
            let (head, request) = read_request(&mut stream).await;
            if step == 0 {
                assert!(head.starts_with("GET /api/v1/models "));
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else if step == 1 {
                assert!(head.starts_with("POST /api/v1/models/unload "));
                let id = request["instance_id"].as_str().unwrap();
                assert_eq!(id, "capacity-instance");
                loaded.get_mut("alpha").unwrap().clear();
                let body = json!({"instance_id":id}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            } else {
                assert!(head.starts_with("GET /api/v1/models "));
                let body = models(&loaded).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        }
        no_request(&provider).await;
    });

    let seed = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(
        seed.status.success(),
        "{}",
        String::from_utf8_lossy(&seed.stderr)
    );
    let seed: Value = serde_json::from_slice(&seed.stdout).unwrap();
    let receipt_id = seed["receipt"]["receipt_id"].as_str().unwrap().to_owned();
    let load_task = seed["receipt"]["load_task_id"].as_str().unwrap().to_owned();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    for index in 0..1023 {
        let receipt_id = uuid::Uuid::new_v4().to_string();
        let instance_id = format!("retired-{index}");
        let initial = "0".repeat(64);
        let receipt = json!({"format_version":1,"receipt_id":receipt_id,"adapter":"lmstudio-native-v1","endpoint":"http://127.0.0.1:1/api/v1","model_key":"alpha","model_type":"llm","context_length":2048,"instance_id":instance_id,"load_task_id":load_task,"initial_inventory_sha256":initial});
        let raw = receipt.to_string();
        let checksum = cogfabric::store::sha256(raw.as_bytes());
        db.execute("INSERT INTO managed_residency_receipts(receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,current_inventory_sha256,state,load_task_id,release_task_id,created,updated) VALUES (?1,'http://127.0.0.1:1/api/v1',?2,?3,?4,?5,'released',?6,NULL,0,0)",rusqlite::params![receipt["receipt_id"].as_str().unwrap(),instance_id,raw,checksum,initial,load_task]).unwrap();
    }
    let before: i64 = db
        .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    drop(db);
    let refused = cli(&home, managed_command("load", &api_base, "beta")).await;
    assert!(!refused.status.success());
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let total: i64 = db
        .query_row(
            "SELECT count(*) FROM managed_residency_receipts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let after: i64 = db
        .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(total, 1024);
    assert_eq!(
        after, before,
        "receipt capacity refusal must occur before task creation"
    );
    drop(db);
    allow_release_tx.send(()).unwrap();
    let released = cli(&home, managed_command("release", &api_base, &receipt_id)).await;
    assert!(
        released.status.success(),
        "releasing remains allowed at receipt capacity: {}",
        String::from_utf8_lossy(&released.stderr)
    );
    server.await.unwrap();
    assert_eq!(
        receipts(&cfg.db())
            .iter()
            .find(|row| row.receipt_id == receipt_id)
            .unwrap()
            .state,
        "released"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn unknown_load_resolution_requires_testimony_preserves_unknown_and_never_posts() {
    let _serial = serial().await;
    let home = home();
    let cfg = create_home(&home);
    let listener = Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    let api_base = base(&listener);
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        exchange(
            &server_listener,
            "GET /api/v1/models ",
            "200 OK",
            models(&BTreeMap::new()),
        )
        .await;
        let (mut stream, _) = server_listener.accept().await.unwrap();
        assert!(read_request(&mut stream)
            .await
            .0
            .starts_with("POST /api/v1/models/load "));
        drop(stream);
    });
    assert!(!cli(&home, managed_command("load", &api_base, "alpha"))
        .await
        .status
        .success());
    server.await.unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let task: String = db
        .query_row("SELECT id FROM tasks LIMIT 1", [], |r| r.get(0))
        .unwrap();
    drop(db);
    let mut command = args(&[
        "residency-managed-resolve",
        "--api-base",
        &api_base,
        "--task-id",
        &task,
        "--human-approve",
    ]);
    assert!(!cli(&home, command.clone()).await.status.success());
    no_request(&listener).await;
    command.push("--provider-stop-acknowledged".into());
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        exchange(
            &server_listener,
            "GET /api/v1/models ",
            "200 OK",
            models(&BTreeMap::new()),
        )
        .await;
    });
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.execute("UPDATE tasks SET status='running' WHERE id=?1", [&task])
        .unwrap();
    for _ in 0..7 {
        db.execute("INSERT INTO tasks(id,query,mode,status,created,updated) VALUES(?1,'other dangling task','fast','running',1,1)", [uuid::Uuid::new_v4().to_string()]).unwrap();
    }
    db.execute_batch("CREATE TRIGGER reject_resolution_audit BEFORE INSERT ON events WHEN NEW.kind='managed_residency_resolution' BEGIN SELECT RAISE(ABORT,'fixture audit denial'); END;").unwrap();
    drop(db);
    let refused = cli(&home, command.clone()).await;
    assert!(!refused.status.success());
    server.await.unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM managed_residency_resolutions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        8
    );
    assert_eq!(
        db.query_row("SELECT status FROM tasks WHERE id=?1", [&task], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "running",
        "audit failure must roll back the local interruption"
    );
    db.execute_batch("DROP TRIGGER reject_resolution_audit;")
        .unwrap();
    drop(db);
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        exchange(
            &server_listener,
            "GET /api/v1/models ",
            "200 OK",
            models(&BTreeMap::new()),
        )
        .await;
    });
    let resolved = cli(&home, command.clone()).await;
    assert!(
        resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    server.await.unwrap();
    let report: Value = serde_json::from_slice(&resolved.stdout).unwrap();
    assert_eq!(report["original_outcome"], "unknown");
    assert_eq!(report["provider_stop_proven"], false);
    assert_eq!(report["retired_receipts"], 0);
    assert!(!cli(&home, command).await.status.success());
    no_request(&listener).await;
    let status = cli(
        &home,
        args(&["residency-managed-status", "--api-base", &api_base]),
    )
    .await;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let report: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(report["unresolved_provider_operation"], false);
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let state: String = db
        .query_row("SELECT status FROM tasks WHERE id=?1", [&task], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(state, "interrupted");
    assert!(db
        .execute(
            "UPDATE managed_residency_resolutions SET operator_approved=1",
            []
        )
        .is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM managed_residency_receipts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(db);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn terminal_receipt_archive_preserves_history_without_restoring_release_authority() {
    let _serial = serial().await;
    let home = home();
    let cfg = create_home(&home);
    let listener = Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    let api_base = base(&listener);
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        let empty = models(&BTreeMap::new());
        let loaded = models(&BTreeMap::from([(
            "alpha".into(),
            vec![("instance-a".into(), 2048)],
        )]));
        exchange(
            &server_listener,
            "GET /api/v1/models ",
            "200 OK",
            empty.clone(),
        )
        .await;
        exchange(&server_listener, "POST /api/v1/models/load ", "200 OK", json!({"status":"loaded","type":"llm","instance_id":"instance-a","load_time_seconds":0.1})).await;
        exchange(
            &server_listener,
            "GET /api/v1/models ",
            "200 OK",
            loaded.clone(),
        )
        .await;
        exchange(&server_listener, "GET /api/v1/models ", "200 OK", loaded).await;
        exchange(
            &server_listener,
            "POST /api/v1/models/unload ",
            "200 OK",
            json!({"instance_id":"instance-a"}),
        )
        .await;
        exchange(&server_listener, "GET /api/v1/models ", "200 OK", empty).await;
    });
    let loaded = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(
        loaded.status.success(),
        "{}",
        String::from_utf8_lossy(&loaded.stderr)
    );
    let loaded: Value = serde_json::from_slice(&loaded.stdout).unwrap();
    let id = loaded["receipt"]["receipt_id"].as_str().unwrap();
    let archive = args(&[
        "residency-managed-archive",
        "--receipt-id",
        id,
        "--human-approve",
    ]);
    assert!(!cli(&home, archive.clone()).await.status.success());
    let released = cli(&home, managed_command("release", &api_base, id)).await;
    assert!(
        released.status.success(),
        "{}",
        String::from_utf8_lossy(&released.stderr)
    );
    server.await.unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.execute_batch("CREATE TRIGGER reject_archive_audit BEFORE INSERT ON events WHEN NEW.kind='managed_residency_audit' AND json_extract(NEW.payload,'$.state')='receipt_archived' BEGIN SELECT RAISE(ABORT,'fixture audit denial'); END;").unwrap();
    drop(db);
    assert!(!cli(&home, archive.clone()).await.status.success());
    assert_eq!(receipts(&cfg.db())[0].state, "released");
    let inspected = cli(
        &home,
        args(&["residency-managed-inspect", "--receipt-id", id]),
    )
    .await;
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspected: Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(inspected["state"], "released");
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM managed_residency_receipt_archive",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER reject_archive_audit;")
        .unwrap();
    drop(db);
    let archived = cli(&home, archive.clone()).await;
    assert!(
        archived.status.success(),
        "{}",
        String::from_utf8_lossy(&archived.stderr)
    );
    assert!(!cli(&home, archive).await.status.success());
    assert!(!cli(&home, managed_command("release", &api_base, id))
        .await
        .status
        .success());
    no_request(&listener).await;
    let status = cli(
        &home,
        args(&["residency-managed-status", "--api-base", &api_base]),
    )
    .await;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM managed_residency_receipts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM managed_residency_receipt_archive",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert!(db
        .execute("DELETE FROM managed_residency_receipt_archive", [])
        .is_err());
    db.execute("UPDATE events SET payload='{}' WHERE kind='managed_residency_audit' AND json_extract(payload,'$.state')='receipt_archived'", []).unwrap();
    drop(db);
    let invalid = cli(
        &home,
        args(&["residency-managed-status", "--api-base", &api_base]),
    )
    .await;
    assert!(
        invalid.status.success(),
        "status reports blocked state without authorizing mutation"
    );
    let invalid: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(invalid["managed_lifecycle_blocked"], true);
    let inspect = cli(
        &home,
        args(&["residency-managed-inspect", "--receipt-id", id]),
    )
    .await;
    assert!(
        !inspect.status.success(),
        "corrupted archive audit must reject evidence inspection"
    );
    assert!(!cli(&home, managed_command("load", &api_base, "beta"))
        .await
        .status
        .success());
    no_request(&listener).await;
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn stale_cohort_requires_explicit_authority_retirement_and_retains_member_proof() {
    let _serial = serial().await;
    let home = home();
    let cfg = create_home(&home);
    let listener = Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    let api_base = base(&listener);
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        let loaded = models(&BTreeMap::from([(
            "alpha".into(),
            vec![("instance-a".into(), 2048)],
        )]));
        exchange(
            &server_listener,
            "GET /api/v1/models ",
            "200 OK",
            models(&BTreeMap::new()),
        )
        .await;
        exchange(&server_listener, "POST /api/v1/models/load ", "200 OK", json!({"status":"loaded","type":"llm","instance_id":"instance-a","load_time_seconds":0.1})).await;
        exchange(
            &server_listener,
            "GET /api/v1/models ",
            "200 OK",
            loaded.clone(),
        )
        .await;
        exchange(&server_listener, "GET /api/v1/models ", "200 OK", loaded).await;
        let (mut stream, _) = server_listener.accept().await.unwrap();
        let (head, body) = read_request(&mut stream).await;
        assert!(head.starts_with("POST /api/v1/models/load "));
        assert_eq!(body["model"], "beta");
        drop(stream);
    });
    let loaded = cli(&home, managed_command("load", &api_base, "alpha")).await;
    assert!(
        loaded.status.success(),
        "{}",
        String::from_utf8_lossy(&loaded.stderr)
    );
    assert!(!cli(&home, managed_command("load", &api_base, "beta"))
        .await
        .status
        .success());
    server.await.unwrap();
    let row = receipts(&cfg.db()).remove(0);
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let task: String = db
        .query_row("SELECT id FROM tasks WHERE status='interrupted'", [], |r| {
            r.get(0)
        })
        .unwrap();
    drop(db);
    let mut command = args(&[
        "residency-managed-resolve",
        "--api-base",
        &api_base,
        "--task-id",
        &task,
        "--human-approve",
        "--provider-stop-acknowledged",
    ]);
    for approved in [false, true] {
        if approved {
            command.push("--retire-stale-cohort".into());
        }
        let server_listener = listener.clone();
        let server = tokio::spawn(async move {
            let changed = models(&BTreeMap::from([(
                "alpha".into(),
                vec![("instance-a".into(), 4096)],
            )]));
            exchange(&server_listener, "GET /api/v1/models ", "200 OK", changed).await;
        });
        let result = cli(&home, command.clone()).await;
        server.await.unwrap();
        assert_eq!(
            result.status.success(),
            approved,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if !approved {
            assert_eq!(receipts(&cfg.db())[0].state, "active");
        }
    }
    let after = receipts(&cfg.db()).remove(0);
    assert_eq!(after.state, "retired");
    assert_eq!(
        after.current_digest, row.current_digest,
        "retirement must not rebase stale authority"
    );
    assert!(!cli(
        &home,
        managed_command("release", &api_base, &row.receipt_id)
    )
    .await
    .status
    .success());
    no_request(&listener).await;
    let inspected = cli(
        &home,
        args(&["residency-managed-inspect", "--receipt-id", &row.receipt_id]),
    )
    .await;
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM managed_residency_resolution_members",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert!(db
        .execute("DELETE FROM managed_residency_resolution_members", [])
        .is_err());
    db.execute("UPDATE events SET payload='{}' WHERE kind='managed_residency_resolution' AND json_extract(payload,'$.state')='operator_resolved'", []).unwrap();
    drop(db);
    assert!(!cli(
        &home,
        args(&[
            "residency-managed-archive",
            "--receipt-id",
            &row.receipt_id,
            "--human-approve"
        ])
    )
    .await
    .status
    .success());
    no_request(&listener).await;
    std::fs::remove_dir_all(home).unwrap();
}
