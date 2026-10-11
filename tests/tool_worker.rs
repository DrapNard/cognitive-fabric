use cogfabric::{config::Config, store::Store};
use std::{
    path::Path,
    process::{Command, Output},
    time::{Duration, Instant},
};

fn command(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(home)
        .arg("--json")
        .args(args)
        .output()
        .unwrap()
}
fn verify(home: &Path, output: Output, expected: &str) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["answer"], expected);
    assert_eq!(outcome["status"], "completed");
    assert_eq!(outcome["calls"], 0);
    assert_eq!(outcome["resources"]["tool_calls"], 2);
    let store = Store::open(&home.join("fabric.sqlite3")).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "tool_worker_process")
            .count(),
        1
    );
    assert_eq!(events.iter().filter(|e| e["kind"] == "tool_worker_process_stopped" && e["payload"]["reaped"] == true).count(), 1);
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "tool_worker"
                && e["payload"]["state"] == "authorized"
                && e["payload"]["charged_calls"] == 1)
            .count(),
        1
    );
    assert!(!events.iter().any(|e| e["kind"] == "worker_process"));
    assert!(events
        .iter()
        .any(|e| e["kind"] == "verification" && e["payload"]["verified"] == true));
}
#[test]
fn cli_launches_one_scoped_arithmetic_worker_and_reaps_it() {
    let home = std::env::temp_dir().join(format!("cogfabric-tool-cli-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: home.clone(),
        inference_process: true,
        ..Config::default()
    };
    cfg.save().unwrap();
    assert!(
        command(&home, &["configure-tool-worker", "--enabled", "true"])
            .status
            .success()
    );
    verify(
        &home,
        command(&home, &["ask", "calc: 6*7", "--mode", "fast"]),
        "42",
    );
    let rejected = command(&home, &["ask", "calc: 1/0", "--mode", "fast"]);
    assert!(!rejected.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(outcome["status"], "rejected");
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "tool_worker_process_stopped" && e["payload"]["reaped"] == true));
    drop(store);
    let mut denied = Config::load(Some(home.clone()), None).unwrap();
    denied.max_tool_calls = 1;
    denied.save().unwrap();
    let denied = command(&home, &["ask", "calc: 6*7", "--mode", "fast"]);
    assert!(!denied.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&denied.stdout).unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "tool_worker_process"));
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
struct Service(std::process::Child);
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn socket_service_launches_and_reaps_arithmetic_workers() {
    let home =
        std::env::temp_dir().join(format!("cogfabric-tool-service-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: home.clone(),
        arithmetic_process: true,
        sql_process: true,
        ..Config::default()
    };
    cfg.save().unwrap();
    let mut service = Service(
        Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&home)
            .arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cfg.socket().exists() {
        assert!(service.0.try_wait().unwrap().is_none(), "Service exited");
        assert!(Instant::now() < deadline, "Service did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    verify(
        &home,
        command(&home, &["ask", "calc: 8*9", "--mode", "fast", "--socket"]),
        "72",
    );
    verify_sql(
        &home,
        command(
            &home,
            &[
                "ask",
                r#"sql:{"tables":[],"query":"SELECT 72 AS answer"}"#,
                "--mode",
                "fast",
                "--socket",
            ],
        ),
        72,
    );
    assert!(command(&home, &["stop"]).status.success());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = service.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "Service did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!cfg.socket().exists());
    drop(service);
    std::fs::remove_dir_all(home).unwrap();
}

fn verify_sql(home: &Path, output: Output, expected: i64) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let answer = outcome["answer"].as_str().unwrap();
    let result: serde_json::Value = serde_json::from_str(answer).unwrap();
    assert_eq!(result["rows"][0][0], expected);
    let store = Store::open(&home.join("fabric.sqlite3")).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "tool_worker"
            && e["payload"]["capability"] == "sql.query"
            && e["payload"]["state"] == "authorized"));
    drop(store);
    verify(home, output, answer);
}

#[test]
fn cli_launches_sql_worker_and_reaps_it_on_success_and_rejection() {
    let home = std::env::temp_dir().join(format!("cogfabric-sql-worker-{}", uuid::Uuid::new_v4()));
    let mut cfg = Config {
        home: home.clone(),
        arithmetic_process: true,
        inference_process: true,
        ..Config::default()
    };
    cfg.save().unwrap();
    assert!(
        command(&home, &["configure-sql-worker", "--enabled", "true"])
            .status
            .success()
    );
    verify_sql(
        &home,
        command(
            &home,
            &[
                "ask",
                r#"sql:{"tables":[{"name":"items","columns":["value"],"rows":[[20],[22]]}],"query":"SELECT sum(value) AS answer FROM items"}"#,
                "--mode",
                "fast",
            ],
        ),
        42,
    );
    let rejected = command(
        &home,
        &[
            "ask",
            r#"sql:{"tables":[],"query":"SELECT random() AS value"}"#,
            "--mode",
            "fast",
        ],
    );
    assert!(!rejected.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_ne!(outcome["status"], "completed");
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "tool_worker_process_stopped" && e["payload"]["reaped"] == true));
    drop(store);
    cfg.sql_process = true;
    cfg.max_tool_calls = 1;
    cfg.save().unwrap();
    let denied = command(
        &home,
        &[
            "ask",
            r#"sql:{"tables":[],"query":"SELECT 42"}"#,
            "--mode",
            "fast",
        ],
    );
    assert!(!denied.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&denied.stdout).unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "tool_worker_process"));
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn sql_routes_by_capability_without_launching_arithmetic_or_inference_workers() {
    let home = std::env::temp_dir().join(format!("cogfabric-sql-routing-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: home.clone(),
        arithmetic_process: true,
        inference_process: true,
        ..Config::default()
    };
    cfg.save().unwrap();
    let output = command(
        &home,
        &[
            "ask",
            r#"sql:{"tables":[],"query":"SELECT 42 AS answer"}"#,
            "--mode",
            "fast",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["status"], "completed");
    assert_eq!(outcome["calls"], 0);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(outcome["answer"].as_str().unwrap()).unwrap()
            ["rows"][0][0],
        42
    );
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["kind"] == "tool_dispatch"
        && e["payload"]["module_id"] == "sql.local"
        && e["payload"]["capability"] == "sql.query"));
    assert!(!events
        .iter()
        .any(|e| e["kind"] == "tool_worker_process" || e["kind"] == "worker_process"));
    assert!(events.iter().any(|e| e["kind"] == "verification"
        && e["payload"]["checker"] == "sqlite_inline/v1"
        && e["payload"]["verified"] == true));
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
