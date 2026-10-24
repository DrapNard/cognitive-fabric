//! Operator inspection must not bootstrap or mutate runtime state.
use cogfabric::{
    config::Config,
    store::{sha256, Store},
};
use serde_json::{json, Value};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("fabric-inference-audit-{}", uuid::Uuid::new_v4())))
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn inspect(home: &std::path::Path, task: &str) -> std::process::Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"));
    command.args(["--json", "--home"]).arg(home).args([
        "--verifier-wasm",
        "/definitely-missing-cogfabric-audit-fixture.wasm",
        "--verifier-wasm-sha256",
        &"a".repeat(64),
        "inference-audit",
        task,
    ]);
    command
        .env_remove("COGFABRIC_BACKEND")
        .env_remove("COGFABRIC_MODEL")
        .env_remove("COGFABRIC_API_BASE")
        .kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn audit_reports_correlated_metrics_without_runtime_bootstrap_or_answers() {
    let root = Root::new();
    std::fs::create_dir(&root.0).unwrap();
    let cfg = Config {
        home: root.0.clone(),
        ..Config::default()
    };
    let store = Store::open(&cfg.db()).unwrap();
    let task = store.create_task("private original goal", "fast").unwrap();
    store
        .event(
            &task,
            "model_dispatch",
            "jev-r",
            json!({"call":1,"role":"solver","allocation":384,"prompt_bytes":123}),
        )
        .unwrap();
    store.event(&task,"reasoning","jev-r",json!({"call":1,"node":{"role":"solver","model":"fixture","content_hash":sha256(b"private answer")},"text":"private answer","input_tokens":7,"output_tokens":3})).unwrap();
    store.event(&task,"inference_provider_metrics","runtime",json!({"format_version":1,"call":1,"role":"solver","answer_sha256":sha256(b"private answer"),"provider_reported":true,"physical_resource_measurement":false,"result_verified":false,"metrics":{"format_version":1,"source":"lm_studio_native","model_load_ms":100.0}})).unwrap();
    drop(store);
    {
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        db.execute_batch("CREATE TRIGGER refuse_audit_bootstrap BEFORE INSERT ON skills BEGIN SELECT RAISE(ABORT, 'inspection bootstrapped runtime'); END;").unwrap();
    }
    std::fs::set_permissions(&root.0, std::fs::Permissions::from_mode(0o750)).unwrap();
    let output = inspect(&root.0, &task).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = String::from_utf8(output.stdout).unwrap();
    assert!(!raw.contains("private"));
    let report: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        report["calls"][0]["provider_metrics"]["model_load_ms"],
        100.0
    );
    assert_eq!(report["provider_metrics_coverage"]["reported_calls"], 1);
    assert_eq!(report["resume_authorized"], false);
    assert_eq!(
        std::fs::metadata(&root.0).unwrap().permissions().mode() & 0o777,
        0o750
    );
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM skills", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn audit_of_missing_database_does_not_create_state_directory() {
    let root = Root::new();
    let output = inspect(&root.0, &uuid::Uuid::new_v4().to_string()).await;
    assert!(!output.status.success());
    assert!(!root.0.exists());
}
