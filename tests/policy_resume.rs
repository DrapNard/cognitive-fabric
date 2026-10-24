use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    escalation::{
        empirical::{EmpiricalPolicy, Thresholds},
        EscalationPolicy,
    },
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self.0.lock().unwrap().pop_front().unwrap();
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "fixture/v1".into(),
                input_tokens: Some(10),
                output_tokens: Some(10),
                elapsed_ms: 1.0,
                queue_wait_ms: None,
                provider_metrics: None,
            })
        })
    }
}
fn script() -> Arc<Script> {
    Arc::new(Script(Mutex::new(
        vec![
            ("planner", "invalid plan"),
            ("solver", "[3,2,1]"),
            ("solver", "[1,2,3]"),
        ]
        .into(),
    )))
}
async fn cli(
    home: &std::path::Path,
    task: &str,
    action: &str,
    study: &std::path::Path,
    thresholds: &std::path::Path,
    approve: bool,
) -> std::process::Output {
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"));
    cmd.arg("--home")
        .arg(home)
        .arg("--json")
        .arg(action)
        .arg(task)
        .arg("--retry-study")
        .arg(study)
        .arg("--retry-thresholds")
        .arg(thresholds);
    if approve {
        cmd.arg("--human-approve");
    }
    tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn cli_restores_original_empirical_policy_and_resumes_only_missing_solver() {
    let home = std::env::temp_dir().join(format!("fabric-empirical-cli-{}", uuid::Uuid::new_v4()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = Config {
        home: home.clone(),
        backend: "openai-local".into(),
        api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
        interpret_goals: false,
        max_tokens: 3000,
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    const QUERY: &str = r#"sort:{"values":[3,1,2]}"#;
    // Obtain real scope metadata without exposing internal prompt functions.
    let baseline = Engine::with_backend(cfg.clone(), store.clone(), script())
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();
    let trace = store.task(&baseline.task_id).unwrap();
    let scope = &trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "escalation_intent")
        .unwrap()["payload"]["request"];
    // Invented observations exercise identity and CLI flow, never claim quality.
    let data = json!({"format_version":1,"mode":"deep","criterion":"integer_sort/v1","cycle":1,
        "configuration_sha256":scope["configuration_sha256"],"role_prompts_sha256":scope["role_prompts_sha256"],"runtime_sources_sha256":scope["runtime_sources_sha256"],
        "dataset_sha256":"a".repeat(64),"source":"Synthetic CLI recovery fixture, not a measured study","license":"CC0-1.0","reviewer":"fixture.operator",
        "held_out_operator_attested":true,"independent_outcomes_operator_attested":true,"includes_failed_attempts_operator_attested":true,
        "observations":[{"task_id":uuid::Uuid::new_v4().to_string(),"verified":true,"model_calls":1,"reserved_output_tokens":384,"tool_calls":1,"prompt_bytes":32000,"elapsed_ms":1}]
    });
    let bytes = serde_json::to_vec(&data).unwrap();
    let thresholds = Thresholds {
        min_observations: 1,
        min_success_basis_points: 10_000,
        max_output_tokens_per_success: 384,
        max_prompt_bytes_per_success: 32000,
        max_milliseconds_per_success: 100,
    };
    let policy = Arc::new(EmpiricalPolicy::from_json(&bytes, thresholds.clone()).unwrap());
    let descriptor = policy.descriptor();
    let original = Engine::with_backend(cfg.clone(), store.clone(), script())
        .with_escalation_policy(policy)
        .unwrap()
        .run_profile(QUERY, "deep", ExecutionProfile::Collective)
        .await
        .unwrap();
    assert_eq!(original.status, "completed");
    let task = original.task_id;
    {
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        let boundary: u64 = db
            .query_row(
                "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary'",
                [&task],
                |r| r.get(0),
            )
            .unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            rusqlite::params![task, boundary],
        )
        .unwrap();
        db.execute(
            "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
            [&task],
        )
        .unwrap();
    }
    let study = home.join("reviewed.json");
    let limits = home.join("thresholds.json");
    std::fs::write(&study, &bytes).unwrap();
    std::fs::write(&limits, serde_json::to_vec(&thresholds).unwrap()).unwrap();
    let absent_home = home.join("must-not-initialize");
    assert!(
        !cli(&absent_home, &task, "resume-task", &study, &limits, true)
            .await
            .status
            .success()
    );
    assert!(!absent_home.exists());
    let before = store.task(&task).unwrap();
    let preview = cli(&home, &task, "resume-preview", &study, &limits, false).await;
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let p: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(p["checkpoint_eligible"], true);
    assert_eq!(p["resume_authorized"], false);
    assert_eq!(
        p["escalation_policy"],
        serde_json::to_value(&descriptor).unwrap()
    );
    assert_eq!(store.task(&task).unwrap(), before);
    let mut changed = thresholds.clone();
    changed.max_milliseconds_per_success = 101;
    std::fs::write(&limits, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(!cli(&home, &task, "resume-preview", &study, &limits, false)
        .await
        .status
        .success());
    assert!(!cli(&home, &task, "resume-task", &study, &limits, true)
        .await
        .status
        .success());
    assert_eq!(store.task(&task).unwrap(), before);
    std::fs::write(&limits, serde_json::to_vec(&thresholds).unwrap()).unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 131072);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|v| v.parse::<usize>().ok())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    let body: Value =
                        serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                    assert_eq!(body["model"], "local-model");
                    break;
                }
            }
        }
        let body = r#"{"choices":[{"message":{"content":"[1,2,3]"}}],"model":"fixture/v1","usage":{"prompt_tokens":10,"completion_tokens":10}}"#;
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
    });
    let output = cli(&home, &task, "resume-task", &study, &limits, true).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["verified"], true);
    assert_eq!(report["new_calls"], 1);
    assert_eq!(
        report["escalation_policy"],
        serde_json::to_value(&descriptor).unwrap()
    );
    assert_eq!(store.inference_audit(&task).unwrap()["reserved_calls"], 3);
    assert_eq!(
        store.inference_audit(&task).unwrap()["reserved_output_tokens"],
        1024
    );
    let trace = store.task(&task).unwrap();
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "escalation_decision")
            .count(),
        1
    );
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn cli_policy_resume_denies_missing_approval_and_bad_input_before_state_creation() {
    let home =
        std::env::temp_dir().join(format!("fabric-no-policy-state-{}", uuid::Uuid::new_v4()));
    let missing = home.join("absent.json");
    let task = uuid::Uuid::new_v4().to_string();
    let output = cli(&home, &task, "resume-task", &missing, &missing, false).await;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("explicit operator approval"));
    assert!(!home.exists());
    assert!(!cli(&home, &task, "resume-task", &missing, &missing, true)
        .await
        .status
        .success());
    assert!(!home.exists());
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("resume-preview")
        .arg(&task)
        .arg("--retry-study")
        .arg(&missing)
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(!home.exists());
}

#[test]
fn continuation_database_open_never_creates_or_migrates_and_denies_final_symlinks() {
    let root =
        std::env::temp_dir().join(format!("fabric-existing-resume-{}", uuid::Uuid::new_v4()));
    let missing = root.join("absent").join("state.db");
    assert!(
        Store::open_existing_with_limit(&missing, cogfabric::storage::DEFAULT_DATABASE_BYTES)
            .is_err()
    );
    assert!(!root.exists());
    std::fs::create_dir(&root).unwrap();
    let old = root.join("old.db");
    let db = rusqlite::Connection::open(&old).unwrap();
    db.pragma_update(None, "user_version", 5).unwrap();
    drop(db);
    assert!(
        Store::open_existing_with_limit(&old, cogfabric::storage::DEFAULT_DATABASE_BYTES).is_err()
    );
    let db = rusqlite::Connection::open(&old).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    drop(db);
    let current = root.join("state.db");
    let store = Store::open(&current).unwrap();
    drop(store);
    let existing =
        Store::open_existing_with_limit(&current, cogfabric::storage::DEFAULT_DATABASE_BYTES)
            .unwrap();
    let task = existing.create_task("calc: 1+1", "fast").unwrap();
    assert!(existing.task(&task).is_ok());
    drop(existing);
    let link = root.join("link.db");
    std::os::unix::fs::symlink(&current, &link).unwrap();
    assert!(
        Store::open_existing_with_limit(&link, cogfabric::storage::DEFAULT_DATABASE_BYTES).is_err()
    );
    std::fs::remove_dir_all(root).unwrap();
}
