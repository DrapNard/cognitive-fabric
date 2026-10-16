use cogfabric::{config::Config, store::Store};
use serde_json::{json, Value};
use std::{
    path::Path,
    process::{Command, Output},
};
fn cli(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(home)
        .arg("--json")
        .args(args)
        .env_clear()
        .output()
        .unwrap()
}
fn configuration() -> Value {
    json!({"settings":{"label":"gpu.main","capacity":{"compute_slots":1,"host_memory_bytes":100,"device_memory_bytes":100},"max_pending":4,"max_wait_ms":1000,"max_execution_ms":1000},"allocation":{"compute_slots":1,"host_memory_bytes":40,"device_memory_bytes":60}})
}
#[test]
fn cli_configures_runs_checks_and_reconciles_resources_without_silent_restart_reset() {
    let root = std::env::temp_dir().join(format!("fabric-cli-resources-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let home = root.join("state");
    let file = root.join("resources.json");
    std::fs::write(&file, configuration().to_string()).unwrap();
    let result = cli(&home, &["configure-resources", file.to_str().unwrap()]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!home.join("fabric.sqlite3").exists());
    let mut cfg = Config::load(Some(home.clone()), None).unwrap();
    cfg.interpret_goals = false;
    cfg.save().unwrap();
    for _ in 0..2 {
        let result = cli(
            &home,
            &["ask", "Describe a generic objective", "--mode", "fast"],
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let store = Store::open(&cfg.db()).unwrap();
    let task = store.create_task("Interrupted fixture", "fast").unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let pool = uuid::Uuid::new_v4().to_string();
    store.event(&task,"resource_dispatch","jev-plus",json!({"format_version":1,"pool_id":pool,"reservation_id":id,"provider_stop_proven":false})).unwrap();
    let result = cli(&home, &["ask", "Must not start fresh", "--mode", "fast"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Unresolved prior resource dispatch"));
    let result = cli(&home, &["resource-status"]);
    assert!(result.status.success());
    let status: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(status["unresolved"][0]["reservation_id"], id);
    assert!(!cli(&home, &["resource-release", &id]).status.success());
    let result = cli(&home, &["resource-release", &id, "--human-approve"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(cli(
        &home,
        &["ask", "Describe the next objective", "--mode", "fast"]
    )
    .status
    .success());
    assert!(cli(&home, &["configure-resources", "--disable"])
        .status
        .success());
    assert!(Config::load(Some(home), None)
        .unwrap()
        .resource_admission
        .is_none());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_resource_configuration_and_missing_approval_do_not_initialize_state() {
    let root =
        std::env::temp_dir().join(format!("fabric-invalid-resources-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let home = root.join("state");
    let file = root.join("resources.json");
    for variant in 0..5 {
        let mut value = configuration();
        match variant {
            0 => value["allocation"]["compute_slots"] = json!(0),
            1 => value["allocation"]["host_memory_bytes"] = json!(101),
            2 => value["settings"]["max_wait_ms"] = json!(0),
            3 => value["settings"]["unknown"] = json!(true),
            _ => value["allocation"]["unknown"] = json!(true),
        };
        std::fs::write(&file, value.to_string()).unwrap();
        assert!(
            !cli(&home, &["configure-resources", file.to_str().unwrap()])
                .status
                .success()
        );
        assert!(!home.exists());
    }
    std::fs::write(&file, r#"{"settings":{},"settings":{},"allocation":{}}"#).unwrap();
    assert!(
        !cli(&home, &["configure-resources", file.to_str().unwrap()])
            .status
            .success()
    );
    assert!(!cli(&home, &["configure-resources"]).status.success());
    assert!(!cli(&home, &["resource-release", "invalid"])
        .status
        .success());
    assert!(!cli(&home, &["resource-status"]).status.success());
    assert!(!home.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn configured_cli_managed_inference_preserves_resource_and_worker_audits() {
    let root = std::env::temp_dir().join(format!(
        "fabric-configured-managed-{}",
        uuid::Uuid::new_v4()
    ));
    let cfg = Config {
        home: root.clone(),
        inference_process: true,
        interpret_goals: false,
        resource_admission: Some(serde_json::from_value(configuration()).unwrap()),
        ..Config::default()
    };
    cfg.save().unwrap();
    let result = cli(
        &root,
        &[
            "ask",
            "Describe a managed generic objective",
            "--mode",
            "fast",
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output: Value = serde_json::from_slice(&result.stdout).unwrap();
    let id = output["task_id"].as_str().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(id).unwrap();
    let events = trace["events"].as_array().unwrap();
    for kind in [
        "resource_dispatch",
        "resource_completed",
        "worker_process_launch",
        "worker_process_stopped",
    ] {
        assert_eq!(
            events.iter().filter(|e| e["kind"] == kind).count(),
            1,
            "{kind}"
        );
    }
    let resource = events
        .iter()
        .position(|e| e["kind"] == "resource_dispatch")
        .unwrap();
    let worker = events
        .iter()
        .position(|e| e["kind"] == "worker_process_launch")
        .unwrap();
    assert!(resource < worker);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_resident_catalog_reserves_capacity_and_rejects_impossible_working_budget() {
    let root = std::env::temp_dir().join(format!("fabric-cli-resident-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let home = root.join("state");
    let file = root.join("resources.json");
    let mut value = configuration();
    value["residents"] = json!([{"id":"backbone.fixture","version":"1.0.0","weights_sha256":"a".repeat(64),"host_memory_bytes":40,"device_memory_bytes":41,"consumers":[{"api_base":"http://localhost:1234/v1/","model":"local-model"}]}]);
    std::fs::write(&file, value.to_string()).unwrap();
    assert!(
        !cli(&home, &["configure-resources", file.to_str().unwrap()])
            .status
            .success()
    );
    assert!(!home.exists());
    value["residents"][0]["device_memory_bytes"] = json!(40);
    std::fs::write(&file, value.to_string()).unwrap();
    let configured = cli(&home, &["configure-resources", file.to_str().unwrap()]);
    assert!(
        configured.status.success(),
        "{}",
        String::from_utf8_lossy(&configured.stderr)
    );
    let mut cfg = Config::load(Some(home.clone()), None).unwrap();
    cfg.interpret_goals = false;
    cfg.save().unwrap();
    let output = cli(
        &home,
        &["ask", "Describe a generic objective", "--mode", "fast"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let trace = store.task(result["task_id"].as_str().unwrap()).unwrap();
    let intent = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "resource_intent")
        .unwrap();
    assert_eq!(
        intent["payload"]["resident_allocated"],
        json!({"compute_slots":0,"host_memory_bytes":40,"device_memory_bytes":40})
    );
    assert_eq!(intent["payload"]["residency_attested"], false);
    assert_eq!(
        intent["payload"]["resident_catalog_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(cli(
        &home,
        &["ask", "Describe the next objective", "--mode", "fast"]
    )
    .status
    .success());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unlisted_role_targets_fail_before_http_or_managed_worker_launch_and_keep_charges() {
    for managed in [false, true] {
        let root =
            std::env::temp_dir().join(format!("fabric-unlisted-target-{}", uuid::Uuid::new_v4()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let mut resources = configuration();
        resources["residents"] = json!([{"id":"backbone.listed","version":"1.0.0","weights_sha256":"a".repeat(64),"host_memory_bytes":40,"device_memory_bytes":40,"consumers":[{"api_base":base,"model":"listed"}]}]);
        let mut cfg = Config {
            home: root.clone(),
            backend: "openai-local".into(),
            model: "listed".into(),
            api_base: base,
            inference_process: managed,
            interpret_goals: false,
            resource_admission: Some(serde_json::from_value(resources).unwrap()),
            ..Config::default()
        };
        cfg.role_profiles.insert(
            "solver".into(),
            cogfabric::config::RoleProfile {
                model: "unlisted".into(),
                temperature: 0.0,
            },
        );
        cfg.save().unwrap();
        let output = cli(
            &root,
            &["ask", "Describe a generic objective", "--mode", "fast"],
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Task ended with status: failed"));
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["status"], "failed");
        assert_eq!(result["calls"], 1);
        assert!(
            result["resources"]["reserved_output_tokens"]
                .as_u64()
                .unwrap()
                > 0
        );
        let store = Store::open(&cfg.db()).unwrap();
        let trace = store.task(result["task_id"].as_str().unwrap()).unwrap();
        let events = trace["events"].as_array().unwrap();
        assert!(events
            .iter()
            .any(|e| e["kind"] == "resource_target" && e["payload"]["allowed"] == false));
        assert!(!events
            .iter()
            .any(|e| e["kind"] == "resource_dispatch" || e["kind"] == "worker_process_launch"));
        assert!(
            matches!(listener.accept(),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock)
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
