use axum::{extract::State, routing::post as route_post, Json, Router};
use cogfabric::{
    config::Config,
    registry::Manifest,
    routing::{
        empirical::{EvidenceRouter, Thresholds},
        WorkflowRequest,
    },
    store::Store,
};
use serde_json::{json, Value};
use std::{
    future::IntoFuture,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

const KEY: &str = "workflow-policy-server-fixture-key-1234567890";
const QUERY: &str = r#"sort:{"values":[3,1,2]}"#;
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

fn task_id() -> String {
    format!(
        "00000000-0000-4000-8000-{:06x}{:06x}",
        std::process::id() & 0x00ff_ffff,
        NEXT_ID.fetch_add(1, Ordering::Relaxed) & 0x00ff_ffff
    )
}

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn inference(State(calls): State<Arc<AtomicUsize>>, Json(_): Json<Value>) -> Json<Value> {
    let index = calls.fetch_add(1, Ordering::SeqCst);
    let content = match index {
        0 | 3 | 6 => "Advice",
        1 | 4 | 7 | 9 => "invalid plan",
        2 | 5 | 8 | 10 => "[1,2,3]",
        _ => "Native response",
    };
    Json(
        json!({"model":"fixture/v1","choices":[{"message":{"role":"assistant","content":content},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}),
    )
}

async fn start_server(home: &std::path::Path, args: &[(&str, &std::path::Path)]) -> Owned {
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserve.local_addr().unwrap();
    drop(reserve);
    let mut command = Command::new(env!("CARGO_BIN_EXE_cogfabric-server"));
    command
        .arg("--home")
        .arg(home)
        .arg("--bind")
        .arg(address.to_string());
    for (flag, path) in args {
        command.arg(flag).arg(path);
    }
    let mut child = Owned(
        command
            .env_clear()
            .env("COGFABRIC_SERVER_API_KEY", KEY)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if client
                .get(format!("http://{address}/v1/models"))
                .bearer_auth(KEY)
                .send()
                .await
                .is_ok()
            {
                break;
            }
            if let Some(status) = child.0.try_wait().unwrap() {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.0.stderr.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                panic!("HTTP server exited ({status}): {stderr}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    // Keep the selected ephemeral address discoverable from the child's bound
    // port by reserving it in a small marker file owned by this test fixture.
    std::fs::write(home.join(".workflow-policy-bind"), address.to_string()).unwrap();
    child
}

fn address(home: &std::path::Path) -> String {
    std::fs::read_to_string(home.join(".workflow-policy-bind")).unwrap()
}

async fn send_chat(client: &reqwest::Client, base: &str, model: &str, content: &str) -> Value {
    client
        .post(format!("{base}/v1/chat/completions"))
        .bearer_auth(KEY)
        .json(&json!({"model":model,"messages":[{"role":"user","content":content}]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn workflow_study(request: &WorkflowRequest) -> Value {
    let candidate = &request.candidates[0];
    let measurement = |verified: bool, expert: bool| {
        json!({
            "task_id": task_id(),
            "verified": verified,
            "model_calls": 1 + u32::from(expert),
            "reserved_output_tokens": 384 + if expert { 192 } else { 0 },
            "prompt_bytes": 100 + if expert { 100 } else { 0 },
            "tool_calls": 0,
            "elapsed_ms": 10 + if expert { 10 } else { 0 },
        })
    };
    let pairs = (0..4)
        .map(|index| {
            json!({
            "case_sha256":cogfabric::store::sha256(format!("case-{index}").as_bytes()),
                "baseline":measurement(index == 3, false),
                "expert":measurement(index < 3, true)
            })
        })
        .collect::<Vec<_>>();
    json!({
        "format_version":1,
        "mode":request.mode,
        "criterion":request.criterion,
        "configuration_sha256":request.configuration_sha256,
        "role_prompts_sha256":request.role_prompts_sha256,
        "runtime_sources_sha256":request.runtime_sources_sha256,
        "pinned_evidence_sha256":request.evidence_sha256,
        "dataset_sha256":"d".repeat(64),
        "source":"Synthetic fixture only",
        "license":"CC0-1.0",
        "reviewer":"fixture.reviewer",
        "held_out_operator_attested":true,
        "independent_outcomes_operator_attested":true,
        "includes_failed_attempts_operator_attested":true,
        "studies":[{
            "workflow_id":candidate.id,
            "workflow_version":candidate.version,
            "workflow_sha256":candidate.snapshot_sha256,
            "pairs":pairs
        }]
    })
}

fn thresholds(max_cost: u64) -> Value {
    serde_json::to_value(Thresholds {
        format_version: 1,
        min_pairs: 4,
        min_net_successes: 2,
        min_net_gain_per_mille: 500,
        max_extra_output_tokens_per_net_success: max_cost,
        main_solver_reserve_ms: 100,
    })
    .unwrap()
}

#[tokio::test]
async fn http_server_pins_workflow_policy_for_collective_requests_only() {
    let directory = std::env::temp_dir().join(format!("fabric-http-workflow-policy-{}", task_id()));
    std::fs::create_dir(&directory).unwrap();
    let home = directory.join("state");
    std::fs::create_dir(&home).unwrap();

    let provider_calls = Arc::new(AtomicUsize::new(0));
    let provider_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_address = provider_listener.local_addr().unwrap();
    let provider = tokio::spawn(
        axum::serve(
            provider_listener,
            Router::new()
                .route("/v1/chat/completions", route_post(inference))
                .with_state(provider_calls.clone()),
        )
        .into_future(),
    );
    let config = Config {
        home: home.clone(),
        backend: "openai-local".into(),
        model: "workflow-policy-fixture".into(),
        api_base: format!("http://{provider_address}/v1"),
        interpret_goals: false,
        max_calls: 6,
        timeout_seconds: 5,
        max_wall_seconds: 30,
        ..Config::default()
    };
    config.save().unwrap();
    let store = Store::open(&config.db()).unwrap();
    cogfabric::store::bootstrap(&store).unwrap();
    let manifest: Manifest = serde_json::from_value(json!({
        "id":"sort.advice",
        "version":"1.0.0",
        "kind":"workflow",
        "description":"Sorting workflow fixture",
        "tags":["sort"],
        "source":"Synthetic fixture",
        "license":"CC0-1.0",
        "workflow":"Suggest an ascending sorting strategy"
    }))
    .unwrap();
    cogfabric::registry::register(&store, &manifest).unwrap();
    cogfabric::registry::activate(&store, &manifest.id, &manifest.version).unwrap();
    drop(store);

    // First run captures the exact pinned request scope and demonstrates the
    // default router admits the matching workflow.
    let default_server = start_server(&home, &[]).await;
    let base = format!("http://{}", address(&home));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let baseline = send_chat(&client, &base, "cogfabric-deep", QUERY).await;
    assert!(baseline["choices"][0]["message"]["content"].is_string());
    drop(default_server);
    let store = Store::open(&config.db()).unwrap();
    let baseline_id = baseline["id"]
        .as_str()
        .unwrap()
        .strip_prefix("chatcmpl-")
        .unwrap();
    let trace = store.task(baseline_id).unwrap();
    let events = trace["events"].as_array().unwrap();
    let routing = events
        .iter()
        .find(|e| e["kind"] == "workflow_routing")
        .unwrap();
    assert_eq!(routing["payload"]["order"], json!(["sort.advice"]));
    let intent = events
        .iter()
        .find(|e| e["kind"] == "workflow_routing_intent")
        .unwrap();
    let request: WorkflowRequest =
        serde_json::from_value(intent["payload"]["request"].clone()).unwrap();
    drop(store);

    let study = directory.join("study.json");
    let limits = directory.join("thresholds.json");
    let study_value = workflow_study(&request);
    let study_bytes = serde_json::to_vec(&study_value).unwrap();
    let threshold_value = thresholds(384);
    let threshold_bytes = serde_json::to_vec(&threshold_value).unwrap();
    let expected_router = EvidenceRouter::from_json(
        &study_bytes,
        serde_json::from_value(threshold_value).unwrap(),
    )
    .unwrap()
    .summary()["policy"]
        .clone();
    std::fs::write(&study, &study_bytes).unwrap();
    std::fs::write(&limits, &threshold_bytes).unwrap();

    let policy_server = start_server(
        &home,
        &[
            ("--workflow-study", study.as_path()),
            ("--workflow-thresholds", limits.as_path()),
        ],
    )
    .await;
    let base = format!("http://{}", address(&home));
    let admitted = send_chat(&client, &base, "cogfabric-deep", QUERY).await;
    assert!(admitted["choices"][0]["message"]["content"].is_string());
    // Replacement after startup cannot replace the already loaded policy.
    std::fs::write(&study, b"{}").unwrap();
    std::fs::write(&limits, b"{}").unwrap();
    let still_admitted = send_chat(&client, &base, "cogfabric-deep", QUERY).await;
    assert!(still_admitted["choices"][0]["message"]["content"].is_string());
    drop(policy_server);

    // A new process explicitly pins a stricter version of the same policy.
    std::fs::write(&study, &study_bytes).unwrap();
    let declined_thresholds = thresholds(1);
    let declined_router = EvidenceRouter::from_json(
        &study_bytes,
        serde_json::from_value(declined_thresholds.clone()).unwrap(),
    )
    .unwrap()
    .summary()["policy"]
        .clone();
    std::fs::write(&limits, serde_json::to_vec(&declined_thresholds).unwrap()).unwrap();
    let declined_server = start_server(
        &home,
        &[
            ("--workflow-study", study.as_path()),
            ("--workflow-thresholds", limits.as_path()),
        ],
    )
    .await;
    let base = format!("http://{}", address(&home));
    let collective = send_chat(&client, &base, "cogfabric-deep", QUERY).await;
    assert!(collective["choices"][0]["message"]["content"].is_string());
    std::fs::write(&study, b"{}").unwrap();
    std::fs::write(&limits, b"{}").unwrap();
    let native = send_chat(&client, &base, "cogfabric-agent", "Say hello").await;
    assert_eq!(
        native["choices"][0]["message"]["content"],
        "Native response"
    );
    assert_eq!(provider_calls.load(Ordering::SeqCst), 12);
    let override_response = client
        .post(format!("{base}/v1/chat/completions"))
        .bearer_auth(KEY)
        .json(&json!({
            "model":"cogfabric-deep",
            "messages":[{"role":"user","content":QUERY}],
            "workflow_study":"/tmp/untrusted-study.json",
            "workflow_thresholds":"/tmp/untrusted-thresholds.json"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(override_response.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 12);
    drop(declined_server);

    let store = Store::open(&config.db()).unwrap();
    let admitted_id = admitted["id"]
        .as_str()
        .unwrap()
        .strip_prefix("chatcmpl-")
        .unwrap();
    let admitted_trace = store.task(admitted_id).unwrap();
    let admitted_routing = admitted_trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "workflow_routing")
        .unwrap();
    assert_eq!(admitted_routing["payload"]["order"], json!(["sort.advice"]));
    assert_eq!(admitted_routing["payload"]["router"], expected_router);
    let still_admitted_id = still_admitted["id"]
        .as_str()
        .unwrap()
        .strip_prefix("chatcmpl-")
        .unwrap();
    let still_admitted_trace = store.task(still_admitted_id).unwrap();
    let still_admitted_routing = still_admitted_trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "workflow_routing")
        .unwrap();
    assert_eq!(
        still_admitted_routing["payload"]["order"],
        json!(["sort.advice"])
    );
    assert_eq!(still_admitted_routing["payload"]["router"], expected_router);
    let id = collective["id"]
        .as_str()
        .unwrap()
        .strip_prefix("chatcmpl-")
        .unwrap();
    let trace = store.task(id).unwrap();
    let events = trace["events"].as_array().unwrap();
    let routing = events
        .iter()
        .find(|e| e["kind"] == "workflow_routing")
        .unwrap();
    assert_eq!(routing["payload"]["order"], json!([]));
    assert_eq!(routing["payload"]["router"], declined_router);
    assert!(trace["task"]["status"] == "completed");
    let native_id = native["id"]
        .as_str()
        .unwrap()
        .strip_prefix("chatcmpl-")
        .unwrap();
    let native_trace = store.task(native_id).unwrap();
    assert!(!native_trace["events"].as_array().unwrap().iter().any(|e| {
        matches!(
            e["kind"].as_str(),
            Some("workflow_routing" | "workflow_routing_intent")
        )
    }));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 12);

    // Invalid/partial policies and hidden workers must fail before state setup.
    for (args, worker) in [
        (vec!["--workflow-study", study.to_str().unwrap()], false),
        (
            vec![
                "--workflow-study",
                study.to_str().unwrap(),
                "--workflow-thresholds",
                limits.to_str().unwrap(),
            ],
            false,
        ),
        (
            vec![
                "--workflow-study",
                study.to_str().unwrap(),
                "--workflow-thresholds",
                limits.to_str().unwrap(),
            ],
            true,
        ),
    ] {
        let absent = directory.join(format!("absent-{}", task_id()));
        let mut command = Command::new(env!("CARGO_BIN_EXE_cogfabric-server"));
        command.arg("--home").arg(&absent);
        for arg in args {
            command.arg(arg);
        }
        if worker {
            command.args([
                "agent-worker",
                "--socket",
                "/tmp/no-worker.sock",
                "--host-pid",
                "1",
            ]);
        } else {
            command.args(["--bind", "127.0.0.1:0"]);
        }
        assert!(!command.env_clear().output().unwrap().status.success());
        assert!(!absent.exists());
    }

    provider.abort();
    let _ = provider.await;
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
