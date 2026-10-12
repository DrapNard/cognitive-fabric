#![cfg(target_os = "linux")]
use cogfabric::{config::Config, store::Store};
use std::{
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::Path,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

struct Service(Child);
impl Drop for Service {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn command(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(home)
        .args(["--backend", "demo", "--json"])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn running_service_keeps_its_worker_image_after_path_replacement() {
    let root =
        std::env::temp_dir().join(format!("cogfabric-worker-image-{}", uuid::Uuid::new_v4()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let home = root.join("state");
    let cfg = Config {
        home: home.clone(),
        inference_process: true,
        arithmetic_process: true,
        sql_process: true,
        interpret_goals: false,
        ..Config::default()
    };
    cfg.save().unwrap();
    let image = root.join("service-image");
    std::fs::copy(env!("CARGO_BIN_EXE_cogfabric"), &image).unwrap();
    let mut service = Service(
        Command::new(&image)
            .arg("--home")
            .arg(&home)
            .args(["--backend", "demo", "serve"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cfg.socket().exists() {
        assert!(
            service.0.try_wait().unwrap().is_none(),
            "Service exited during startup"
        );
        assert!(Instant::now() < deadline, "Service startup timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(command(&home, &["service-status"]).status.success());
    // Atomic replacement models an upgrade; never execute the replacement file.
    let replacement = root.join("replacement");
    std::fs::write(&replacement, b"This is not the running worker image.\n").unwrap();
    std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::rename(&replacement, &image).unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    for (query, status, kind) in [
        ("calc: 6*7", "completed", "tool_worker_process"),
        (
            r#"sql:{"tables":[],"query":"SELECT 2 + 3 AS total"}"#,
            "completed",
            "tool_worker_process",
        ),
        ("Explain Rust ownership", "demo", "worker_process"),
    ] {
        let output = command(&home, &["ask", query, "--mode", "fast", "--socket"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["status"], status);
        if query.starts_with("calc:") {
            assert_eq!(result["answer"], "42");
        }
        if query.starts_with("sql:") {
            let answer: serde_json::Value =
                serde_json::from_str(result["answer"].as_str().unwrap()).unwrap();
            assert_eq!(answer["rows"], serde_json::json!([[5]]));
        }
        let trace = store.task(result["task_id"].as_str().unwrap()).unwrap();
        let events = trace["events"].as_array().unwrap();
        assert_eq!(events.iter().filter(|e| e["kind"] == kind).count(), 1);
        let stopped = format!("{kind}_stopped");
        let stops = events
            .iter()
            .filter(|e| e["kind"] == stopped)
            .collect::<Vec<_>>();
        assert_eq!(stops.len(), 1);
        assert_eq!(stops[0]["payload"]["reaped"], true);
        assert_eq!(stops[0]["payload"]["forced"], false);
        assert_eq!(stops[0]["payload"]["exit_code"], 0);
    }
    assert!(command(&home, &["stop"]).status.success());
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "Service shutdown timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!cfg.socket().exists());
    drop(service);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
