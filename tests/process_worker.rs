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
fn registered_expert_uses_its_own_managed_backend_and_both_children_are_reaped() {
    use std::{
        io::{Read, Write},
        time::Duration,
    };
    let home = std::env::temp_dir().join(format!("fabric-process-expert-{}", uuid::Uuid::new_v4()));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut raw = Vec::new();
        let mut buffer = [0u8; 4096];
        let request = loop {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            raw.extend_from_slice(&buffer[..count]);
            assert!(raw.len() < 64000);
            if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&raw[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if raw.len() >= end + 4 + length {
                    break serde_json::from_slice::<serde_json::Value>(
                        &raw[end + 4..end + 4 + length],
                    )
                    .unwrap();
                }
            }
        };
        let body=serde_json::json!({"model":"domain-provider","choices":[{"message":{"content":"Independent domain advice remains unverified"}}],"usage":{"prompt_tokens":5,"completion_tokens":3}}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        request
    });
    let cfg = Config {
        home: home.clone(),
        inference_process: true,
        interpret_goals: false,
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let model = home.join("reviewed-model.json");
    std::fs::write(&model,serde_json::json!({"id":"domain.local","version":"1.0.0","model":"domain-model","api_base":endpoint,"source":"fixture","license":"CC0","context_tokens":4096,"max_output_tokens":512,"weights_sha256":null}).to_string()).unwrap();
    store.register_model(&model).unwrap();
    let manifest:cogfabric::registry::Manifest=serde_json::from_value(serde_json::json!({"id":"rust.model.s1","version":"1.0.0","kind":"workflow","description":"Reviewed model-backed advice","tags":["ownership"],"source":"fixture","license":"CC0","workflow":"Inspect domain evidence without claiming verification","model":{"id":"domain.local","version":"1.0.0"}})).unwrap();
    cogfabric::registry::register(&store, &manifest).unwrap();
    cogfabric::registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    let output = command(&home, &["ask", "Explain ownership", "--mode", "deep"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["status"], "demo");
    let request = server.join().unwrap();
    assert_eq!(request["model"], "domain-model");
    assert_eq!(request["max_tokens"], 192);
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "worker_process")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "worker_process_stopped" && e["payload"]["reaped"] == true)
            .count(),
        2
    );
    assert!(store.recover_workers().unwrap().is_empty());
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
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
    terminate_pending_inference(false);
}
#[test]
fn crashed_worker_is_reconciled_without_replaying_or_changing_task_result() {
    terminate_pending_inference(true);
}
fn terminate_pending_inference(crash_worker: bool) {
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
    // Target only the current fixture's owned child or its newly launched worker.
    let (pid, signal) = if crash_worker {
        let db = rusqlite::Connection::open_with_flags(
            cfg.db(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let pid: i32 = db
            .query_row(
                "SELECT json_extract(payload,'$.pid') FROM events WHERE kind='worker_process'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        (pid, libc::SIGKILL)
    } else {
        (child.id() as i32, libc::SIGINT)
    };
    assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
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
    assert_eq!(
        outcome["status"],
        if crash_worker { "failed" } else { "cancelled" }
    );
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(outcome["task_id"].as_str().unwrap()).unwrap();
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "worker_process_stopped" && e["payload"]["reaped"] == true));
    server.join().unwrap();
    let recovered = command(&home, &["recover-workers"]);
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    let recovered: serde_json::Value = serde_json::from_slice(&recovered.stdout).unwrap();
    assert_eq!(
        recovered["interrupted_worker_sessions"]
            .as_array()
            .unwrap()
            .len(),
        usize::from(crash_worker)
    );
    assert!(store.recover_workers().unwrap().is_empty());
    assert_eq!(
        store.task(outcome["task_id"].as_str().unwrap()).unwrap()["task"]["status"],
        outcome["status"]
    );
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
