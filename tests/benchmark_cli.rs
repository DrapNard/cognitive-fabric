//! Verify actual CLI partial output and durable cancellation with a stalled provider.
use cogfabric::{config::Config, store::Store};
use serde_json::{json, Value};
use std::{process::Stdio, sync::Arc, time::Duration};
#[tokio::test]
async fn benchmark_cli_deadline_retains_partial_report_and_exits_unsuccessfully() {
    run(false).await;
}
#[tokio::test]
async fn benchmark_cli_ctrl_c_retains_partial_report_and_exits_unsuccessfully() {
    run(true).await;
}
async fn run(interrupt: bool) {
    let root = std::env::temp_dir().join(format!("fabric-benchmark-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = Config {
        home: root.join("state"),
        backend: "openai-local".into(),
        api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
        max_calls: 1,
        max_cycles: 1,
        max_tokens: 512,
        max_tool_calls: 0,
        timeout_seconds: 30,
        max_wall_seconds: 60,
        interpret_goals: false,
        synthesize_results: false,
        ..Default::default()
    };
    cfg.save().unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let provider_entered = entered.clone();
    let provider = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        provider_entered.notify_one();
        std::future::pending::<()>().await;
        drop(stream);
    });
    let dataset = root.join("cases.jsonl");
    std::fs::write(
        &dataset,
        format!(
            "{}\n{}\n",
            json!({"input":"Explain ownership briefly","expected":"answer"}),
            json!({"input":"Another question","expected":"answer"})
        ),
    )
    .unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .env_clear()
        .arg("--home")
        .arg(&cfg.home)
        .args(["--json", "bench"])
        .arg(&dataset)
        .args([
            "--mode",
            "fast",
            "--max-run-seconds",
            if interrupt { "10" } else { "1" },
        ])
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(root.join("report.json")).unwrap())
        .stderr(std::fs::File::create(root.join("stderr.log")).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    if interrupt {
        assert_eq!(
            unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGINT) },
            0
        );
    }
    let status = tokio::time::timeout(Duration::from_secs(6), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(!status.success());
    let report: Value =
        serde_json::from_slice(&std::fs::read(root.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(
        report["stop_reason"],
        if interrupt {
            "cancelled"
        } else {
            "run_deadline"
        }
    );
    assert_eq!(report["total"], 2);
    assert_eq!(report["executed"], 1);
    assert_eq!(report["unexecuted"], 1);
    assert_eq!(report["model_calls"], 1);
    assert_eq!(report["passed"], 0);
    assert_eq!(report["runtime_status_counts"]["cancelled"], 1);
    let store = Store::open(&cfg.db()).unwrap();
    let task = report["details"][0]["task_id"].as_str().unwrap();
    assert_eq!(store.task(task).unwrap()["task"]["status"], "cancelled");
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let count: u64 = db
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    provider.abort();
    let _ = provider.await;
    drop(db);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
