use cogfabric::{config::Config, scheduling::Policy, store::Store};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const PLAN: &str = r#"{"steps":[{"id":"zz","objective":"Detached work","depends_on":[]},{"id":"a","objective":"Start chain","depends_on":[]},{"id":"b","objective":"Middle chain","depends_on":["a"]},{"id":"c","objective":"End chain","depends_on":["b"]}]}"#;
async fn cli(home: &std::path::Path, args: &[&str]) -> Value {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(home)
            .arg("--json")
            .args(args)
            .env_clear()
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test]
async fn configured_cli_orders_dependencies_and_resume_keeps_original_decision_after_config_change()
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut cfg = Config {
        home: std::env::temp_dir().join(format!("fabric-critical-path-{}", uuid::Uuid::new_v4())),
        backend: "openai-local".into(),
        api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
        interpret_goals: false,
        subtask_scheduling: Some(Policy::CriticalPath),
        max_tokens: 2400,
        ..Config::default()
    };
    cfg.save().unwrap();
    let server = tokio::spawn(async move {
        // Six original calls, then only three outstanding subtasks and final solver.
        for (expected, answer, dependency) in [
            ("Objective:", PLAN, ""),
            ("Start chain", "start-result", ""),
            ("Middle chain", "middle-result", "start-result"),
            ("End chain", "end-result", "middle-result"),
            ("Detached work", "detached-result", ""),
            ("Previous unverified answer:", "[1,2,3]", ""),
            ("Middle chain", "middle-result", "start-result"),
            ("End chain", "end-result", "middle-result"),
            ("Detached work", "detached-result", ""),
            ("Previous unverified answer:", "[1,2,3]", ""),
        ] {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(15), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
            let mut bytes = Vec::new();
            let request = loop {
                let mut chunk = [0u8; 4096];
                let n = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    stream.read(&mut chunk),
                )
                .await
                .unwrap()
                .unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 64000);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                            .unwrap();
                    }
                }
            };
            let prompt = request["messages"][1]["content"].as_str().unwrap();
            assert!(prompt.contains(expected), "{expected}: {prompt}");
            assert!(prompt.contains(dependency));
            let body=json!({"choices":[{"message":{"content":answer}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}).to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        }
    });
    let original = cli(
        &cfg.home,
        &["ask", r#"sort:{"values":[3,2,1]}"#, "--mode", "deep"],
    )
    .await;
    assert_eq!(original["status"], "completed");
    assert_eq!(original["calls"], 6);
    let task = original["task_id"].as_str().unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let cutoff:u64=db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='subtask_resources' AND json_extract(payload,'$.node_id')='a' AND json_extract(payload,'$.state')='unverified'",[task],|r|r.get(0)).unwrap();
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, cutoff],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [task],
    )
    .unwrap();
    drop(db);
    cfg.subtask_scheduling = Some(Policy::Stable);
    cfg.save().unwrap();
    let resumed = cli(&cfg.home, &["resume-task", task, "--human-approve"]).await;
    assert_eq!(resumed["status"], "completed");
    assert_eq!(resumed["new_calls"], 4);
    assert_eq!(resumed["charged_resources"]["calls"], 6);
    server.await.unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    assert!(store.graph_audit(task).is_ok());
    let trace = store.task(task).unwrap();
    let events = trace["events"].as_array().unwrap();
    let decisions = events
        .iter()
        .filter(|e| e["kind"] == "subtask_schedule")
        .collect::<Vec<_>>();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["payload"]["order"], json!([1, 2, 3, 0]));
    assert_eq!(
        decisions[0]["payload"]["scheduler"]["id"],
        "jev-plus.critical-path"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "execution_settings")
            .count(),
        1
    );
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
