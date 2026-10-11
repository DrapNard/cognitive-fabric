use cogfabric::{config::Config, store::Store};
use std::{path::PathBuf, process::Command};

fn command(home: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(home)
        .arg("--backend")
        .arg("demo")
        .arg("--json")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn managed_worker_runs_and_audits_one_task_without_claiming_real_inference() {
    let home = std::env::temp_dir().join(format!("fabric-process-e2e-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: home.clone(),
        ..Config::default()
    };
    cfg.save().unwrap();
    let configured = command(&home, &["configure-worker", "--enabled", "true"]);
    assert!(
        configured.status.success(),
        "{}",
        String::from_utf8_lossy(&configured.stderr)
    );
    let output = command(
        &home,
        &["ask", "Explain bounded reasoning", "--mode", "standard"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["status"], "demo");
    assert!(outcome["calls"].as_u64().unwrap() > 0);
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "worker_process")
            .count(),
        1
    );
    assert!(events
        .iter()
        .any(|e| e["kind"] == "worker.audit" && e["payload"]["state"] == "authorized"));
    assert!(events
        .iter()
        .any(|e| e["kind"] == "worker.audit" && e["payload"]["state"] == "inference_completed"));
    assert!(events
        .iter()
        .any(|e| e["kind"] == "worker_process_stopped" && e["payload"]["reaped"] == true));
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn deterministic_tools_do_not_launch_a_worker() {
    let home = std::env::temp_dir().join(format!("fabric-process-tool-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: home.clone(),
        inference_process: true,
        ..Config::default()
    };
    cfg.save().unwrap();
    let output = command(&home, &["ask", "calc: 6*7", "--mode", "fast"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["answer"], "42");
    assert_eq!(outcome["calls"], 0);
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "worker_process"));
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cancellation_reaps_worker_during_pending_http_inference() {
    use std::{
        io::Read,
        time::{Duration, Instant},
    };
    let home = std::env::temp_dir().join(format!("fabric-process-cancel-{}", uuid::Uuid::new_v4()));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let cfg = Config {
        home: home.clone(),
        backend: "openai-local".into(),
        inference_process: true,
        interpret_goals: false,
        api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
        max_wall_seconds: 10,
        ..Config::default()
    };
    cfg.save().unwrap();
    let (entered, received) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = [0u8; 4096];
        assert!(stream.read(&mut bytes).unwrap() > 0);
        entered.send(()).unwrap();
        loop {
            match stream.read(&mut bytes) {
                Ok(0) => break,
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                Err(error) => panic!("HTTP request did not disconnect: {error}"),
            }
        }
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args([
            "--json",
            "ask",
            "Explain bounded reasoning",
            "--mode",
            "fast",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    received.recv_timeout(Duration::from_secs(10)).unwrap();
    // Signal only the child created and still owned by this test.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("CLI cancellation did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["status"], "cancelled");
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "worker_process_stopped" && e["payload"]["reaped"] == true));
    server.join().unwrap();
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
