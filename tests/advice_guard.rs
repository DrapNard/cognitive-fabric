#![cfg(target_os = "linux")]
use cogfabric::{config::Config, reasoning::advice_guard::Settings, store::Store};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[tokio::test]
async fn separate_cli_tasks_reuse_timeout_history_and_preserve_the_first_solver() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = Config {
        home: std::env::temp_dir().join(format!("fabric-advice-cli-{}", uuid::Uuid::new_v4())),
        backend: "openai-local".into(),
        api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
        interpret_goals: false,
        timeout_seconds: 1,
        max_tokens: 2000,
        advice_guard: Some(Settings {
            min_observations: 2,
            sample_size: 4,
            window_seconds: 60,
            trip_failure_basis_points: 10000,
        }),
        ..Config::default()
    };
    cfg.save().unwrap();
    let planners = Arc::new(AtomicUsize::new(0));
    let solvers = Arc::new(AtomicUsize::new(0));
    let p = planners.clone();
    let s = solvers.clone();
    let server = tokio::spawn(async move {
        let mut handlers = tokio::task::JoinSet::new();
        for _ in 0..5 {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(10), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
            let p = p.clone();
            let s = s.clone();
            handlers.spawn(async move {
                let mut bytes=Vec::new();let body=loop {
                    let mut chunk=[0u8;4096];let n=tokio::time::timeout(std::time::Duration::from_secs(3),stream.read(&mut chunk)).await.unwrap().unwrap();assert!(n>0);bytes.extend_from_slice(&chunk[..n]);assert!(bytes.len()<64000);
                    if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {let headers=String::from_utf8_lossy(&bytes[..end]).to_lowercase();let length=headers.lines().find_map(|l|l.strip_prefix("content-length:")).unwrap().trim().parse::<usize>().unwrap();if bytes.len()>=end+4+length {break serde_json::from_slice::<Value>(&bytes[end+4..end+4+length]).unwrap()}}
                };
                let system=body["messages"][0]["content"].as_str().unwrap();
                if system.starts_with("Decompose") {p.fetch_add(1,Ordering::SeqCst);tokio::time::sleep(std::time::Duration::from_secs(2)).await;return;}
                assert!(system.starts_with("Solve"));s.fetch_add(1,Ordering::SeqCst);
                let response=json!({"choices":[{"message":{"content":"[1,2,3]"}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).as_bytes()).await.unwrap();
            });
        }
        while let Some(result) = handlers.join_next().await {
            result.unwrap();
        }
    });
    for attempt in 0..3 {
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
                .arg("--home")
                .arg(&cfg.home)
                .args([
                    "--json",
                    "ask",
                    r#"sort:{"values":[3,2,1]}"#,
                    "--mode",
                    "deep",
                ])
                .env_clear()
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["status"], "completed");
        assert_eq!(result["calls"], if attempt < 2 { 2 } else { 1 });
        let store = Store::open(&cfg.db()).unwrap();
        let trace = store.task(result["task_id"].as_str().unwrap()).unwrap();
        let events = trace["events"].as_array().unwrap();
        let decision = events
            .iter()
            .find(|e| e["kind"] == "advice_guard_decision")
            .unwrap();
        assert_eq!(decision["payload"]["allow"], attempt < 2);
        if attempt == 2 {
            assert!(!events.iter().any(|e| e["kind"] == "planning_phase_budget"));
            assert_eq!(result["resources"]["reserved_output_tokens"], 384);
            assert_eq!(decision["payload"]["history"]["failures"], 2);
        }
    }
    server.await.unwrap();
    assert_eq!(planners.load(Ordering::SeqCst), 2);
    assert_eq!(solvers.load(Ordering::SeqCst), 3);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
