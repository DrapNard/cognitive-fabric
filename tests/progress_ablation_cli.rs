//! Actual CLI deployment of the paired runner, using the deterministic demo adapter.
use serde_json::{json, Value};
use std::{path::Path, process::Command};

const QUERY: &str = r#"csp:{"variables":[{"id":"x","domain":[1,2]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":2}]}"#;
fn run(home: &Path, dataset: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(home)
        .args(["--backend", "demo", "--json", "compare-progress"])
        .arg(dataset)
        .args([
            "--mode",
            "research",
            "--plateau-cycles",
            "2",
            "--source",
            "Public CLI regression fixture",
            "--license",
            "CC0-1.0",
            "--workflow-check",
            "--code-revision",
            &"a".repeat(40),
            "--max-run-seconds",
            "30",
        ])
        .output()
        .unwrap()
}
#[test]
fn cli_reports_paired_scope_without_installing_a_persistent_policy() {
    let root = std::env::temp_dir().join(format!("fabric-progress-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let home = root.join("state");
    let file = root.join("cases.jsonl");
    std::fs::write(
        &file,
        format!(
            "{}\n",
            json!({"input":QUERY,"verification":"finite_domain_v1"})
        ),
    )
    .unwrap();
    let output = run(&home, &file);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["experiment"], "finite_domain_progress_stopping/v1");
    assert_eq!(report["complete"], true);
    assert_eq!(report["complete_cases"], 1);
    assert_eq!(report["arms"].as_array().unwrap().len(), 2);
    assert_eq!(report["automatic_policy_installation"], false);
    assert_eq!(report["permissions_granted_by_experiment"], false);
    assert_eq!(report["held_out_operator_attested"], false);
    let mut baseline = report["baseline_configuration"].clone();
    let mut candidate = report["progress_configuration"].clone();
    assert!(baseline.get("progress_stopping").is_none());
    assert_eq!(candidate["progress_stopping"]["plateau_cycles"], 2);
    baseline
        .as_object_mut()
        .unwrap()
        .remove("progress_stopping");
    candidate
        .as_object_mut()
        .unwrap()
        .remove("progress_stopping");
    assert_eq!(baseline, candidate);
    let cfg = home.join("config.json");
    if cfg.exists() {
        let original: Value = serde_json::from_slice(&std::fs::read(cfg).unwrap()).unwrap();
        assert!(original.get("progress_stopping").is_none());
    }
    // Every input is preflighted before either arm starts.
    let denied_home = root.join("denied");
    std::fs::write(
        &file,
        format!(
            "{}\n",
            json!({"input":QUERY,"verification":"finite_domain_v1","expected":"{\"x\":2}"})
        ),
    )
    .unwrap();
    let denied = run(&denied_home, &file);
    assert!(!denied.status.success());
    let db = rusqlite::Connection::open(denied_home.join("fabric.sqlite3")).unwrap();
    let tasks: u64 = db
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tasks, 0);
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}
