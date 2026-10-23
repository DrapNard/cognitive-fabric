use cogfabric::{config::Config, store::Store};
use std::{
    path::Path,
    process::{Command, Output},
};

fn command(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(home)
        .arg("--backend")
        .arg("demo")
        .arg("--json")
        .arg("--typed-module-worker")
        .args(args)
        .output()
        .unwrap()
}

fn assert_module_lifecycle(home: &Path, output: &Output, answer: &str) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["answer"], answer);
    let store = Store::open(&home.join("fabric.sqlite3")).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert!(events
        .iter()
        .any(|event| event["kind"] == "module_process_launch"));
    assert!(events
        .iter()
        .any(|event| event["kind"] == "module_process_owned"));
    assert!(events.iter().any(|event| {
        event["kind"] == "module_process_stopped" && event["payload"]["reaped"] == true
    }));
    assert!(events
        .iter()
        .any(|event| event["kind"] == "module_dispatch_intent"));
    assert!(events.iter().any(|event| {
        event["kind"] == "module_dispatch_outcome" && event["payload"]["status"] == "returned"
    }));
    assert!(events
        .iter()
        .any(|event| { event["kind"] == "verification" && event["payload"]["verified"] == true }));
}

#[test]
fn cli_runs_typed_arithmetic_and_sql_in_the_same_image_worker() {
    let home =
        std::env::temp_dir().join(format!("cogfabric-typed-module-{}", uuid::Uuid::new_v4()));
    Config {
        home: home.clone(),
        ..Config::default()
    }
    .save()
    .unwrap();

    let arithmetic = command(&home, &["ask", "calc: 6*7", "--mode", "fast"]);
    assert_module_lifecycle(&home, &arithmetic, "42");

    let sql = command(
        &home,
        &[
            "ask",
            r#"sql:{"tables":[],"query":"SELECT 42 AS answer"}"#,
            "--mode",
            "fast",
        ],
    );
    assert_module_lifecycle(&home, &sql, "{\"columns\":[\"answer\"],\"rows\":[[42]]}");
    drop(Store::open(&home.join("fabric.sqlite3")).unwrap());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn forged_hidden_worker_fails_before_creating_or_loading_a_home() {
    let home = std::env::temp_dir().join(format!(
        "cogfabric-forged-module-worker-{}",
        uuid::Uuid::new_v4()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("--json")
        .arg("module-worker")
        .arg("--socket")
        .arg("/tmp/cogfabric-forged-module-worker.sock")
        .arg("--host-pid")
        .arg("1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        !home.exists(),
        "hidden worker initialized ordinary runtime configuration"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn cancelling_during_a_running_sql_worker_reaps_the_owned_child() {
    use std::{
        process::{Child, Stdio},
        time::{Duration, Instant},
    };

    struct ChildGuard(Option<Child>);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if let Some(child) = self.0.as_mut() {
                if child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }

    let home =
        std::env::temp_dir().join(format!("cogfabric-typed-cancel-{}", uuid::Uuid::new_v4()));
    let config = Config {
        home: home.clone(),
        ..Config::default()
    };
    config.save().unwrap();
    let query = r#"sql:{"tables":[],"query":"WITH RECURSIVE cnt(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM cnt WHERE x<999999999) SELECT sum(x) AS total FROM cnt"}"#;
    let mut child = ChildGuard(Some(
        Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&home)
            .args([
                "--backend",
                "demo",
                "--json",
                "--typed-module-worker",
                "ask",
                query,
                "--mode",
                "fast",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    ));

    let db_path = config.db();
    let deadline = Instant::now() + Duration::from_secs(10);
    let (task_id, worker_pid) = loop {
        assert!(
            Instant::now() < deadline,
            "typed worker did not reach its owned state"
        );
        if db_path.exists() {
            let connection = rusqlite::Connection::open(&db_path).unwrap();
            connection.busy_timeout(Duration::from_millis(50)).unwrap();
            let owned = connection.query_row(
                "SELECT task_id, payload FROM events WHERE kind='module_process_owned' ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            );
            if let Ok((task, payload)) = owned {
                let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
                break (task, payload["pid"].as_u64().unwrap() as libc::pid_t);
            }
        }
        assert!(
            child.0.as_mut().unwrap().try_wait().unwrap().is_none(),
            "CLI exited before owning a typed worker"
        );
        std::thread::sleep(Duration::from_millis(2));
    };

    // SIGINT follows the operator cancellation path. A long recursive SELECT
    // keeps the worker active while ownership is dropped; no test-only pause
    // hook is needed.
    assert_eq!(
        unsafe { libc::kill(child.0.as_ref().unwrap().id() as libc::pid_t, libc::SIGINT) },
        0
    );
    let stop_deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if child.0.as_mut().unwrap().try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            Instant::now() < stop_deadline,
            "CLI did not finish cancellation"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.0.take().unwrap().wait_with_output().unwrap();
    assert!(!output.status.success());

    let stopped_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let connection = rusqlite::Connection::open(&db_path).unwrap();
        connection.busy_timeout(Duration::from_millis(50)).unwrap();
        let reaped = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='module_process_stopped' AND json_extract(payload,'$.reaped')=1 AND json_extract(payload,'$.dropped_owner')=1)",
            [&task_id],
            |row| row.get::<_, bool>(0),
        ).unwrap();
        if reaped {
            break;
        }
        assert!(
            Instant::now() < stopped_deadline,
            "cancelled typed child was not audited as reaped"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(unsafe { libc::kill(worker_pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    drop(child);
    std::fs::remove_dir_all(home).unwrap();
}
