use cogfabric::{config::Config, store::Store};
use std::process::Command;
#[test]
fn cli_pages_full_audit_history_with_stable_cursors() {
    let home = std::env::temp_dir().join(format!("fabric-cli-audit-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: home.clone(),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let task = store.create_task("page fixture", "fast").unwrap();
    for index in 0..35 {
        store
            .event(
                &task,
                "audit",
                "runtime",
                serde_json::json!({"index":index}),
            )
            .unwrap();
    }
    let first = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args(["--json", "task-events", &task])
        .output()
        .unwrap();
    assert!(first.status.success());
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["events"].as_array().unwrap().len(), 32);
    assert_eq!(first["more"], true);
    let second = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args([
            "--json",
            "task-events",
            &task,
            "--after",
            &first["cursor"].to_string(),
        ])
        .output()
        .unwrap();
    assert!(second.status.success());
    let second: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second["events"].as_array().unwrap().len(), 3);
    assert_eq!(second["events"][0]["payload"]["index"], 32);
    assert_eq!(second["more"], false);
    let outcome = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args([
            "--backend",
            "demo",
            "--json",
            "ask",
            "Explain ownership",
            "--mode",
            "fast",
        ])
        .output()
        .unwrap();
    assert!(outcome.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&outcome.stdout).unwrap();
    let audit = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args([
            "--json",
            "inference-audit",
            outcome["task_id"].as_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(audit.status.success());
    let audit: serde_json::Value = serde_json::from_slice(&audit.stdout).unwrap();
    assert_eq!(audit["reserved_calls"], outcome["calls"]);
    assert_eq!(audit["recorded_results"], outcome["calls"]);
    assert_eq!(audit["resume_authorized"], false);
    assert!(audit["input_tokens"].is_null());
    let graph = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args([
            "--json",
            "graph-audit",
            outcome["task_id"].as_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(graph.status.success());
    let graph: serde_json::Value = serde_json::from_slice(&graph.stdout).unwrap();
    assert_eq!(graph["graph_recorded"], false);
    assert_eq!(graph["resume_authorized"], false);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
