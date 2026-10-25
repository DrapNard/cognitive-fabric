use cogfabric::{
    benchmarking::{progress::compare_progress_until, Review},
    config::Config,
    reasoning::progress::Settings,
    store::Store,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const FIRST_QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1,2,3]},{"id":"b","domain":[0,1,2,3]},{"id":"c","domain":[0,1,2,3]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"b","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"c","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const SECOND_QUERY: &str = r#"csp:{"variables":[{"id":"x","domain":[0,1,2,3]},{"id":"y","domain":[0,1,2,3]},{"id":"z","domain":[0,1,2,3]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"y","coefficient":1}],"relation":"eq","rhs":1},{"kind":"linear","terms":[{"variable":"z","coefficient":1}],"relation":"eq","rhs":1}]}"#;

fn review() -> Review {
    Review {
        source: "public synthetic loopback fixture".into(),
        license: "Apache-2.0".into(),
        code_revision: "a".repeat(40),
        held_out: false,
        workflow_check: true,
        expected_dataset_sha256: None,
        max_run_seconds: 40,
    }
}

fn task_count(db: &std::path::Path) -> u64 {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
        .unwrap()
}

fn response(text: &'static str) -> String {
    json!({"choices":[{"message":{"content":text}}],"usage":{"prompt_tokens":7,"completion_tokens":2}}).to_string()
}

async fn serve(listener: tokio::net::TcpListener) -> Vec<Value> {
    let first_bad = [
        r#"{"a":1,"b":0,"c":1}"#,
        r#"{"a":1,"b":2,"c":1}"#,
        r#"{"a":1,"b":3,"c":1}"#,
        r#"{"a":2,"b":1,"c":1}"#,
    ];
    let second_bad = [
        r#"{"x":1,"y":0,"z":1}"#,
        r#"{"x":1,"y":2,"z":1}"#,
        r#"{"x":1,"y":3,"z":1}"#,
    ];
    // Calls are in deterministic case order with arm order rotating per case:
    // baseline(A), treatment(A), treatment(B), baseline(B). Advice stages are
    // returned first; each arm then gets four or three solver candidates.
    let mut replies = Vec::new();
    for candidates in [first_bad.as_slice(), first_bad[..3].as_ref()] {
        replies.push("invalid graph");
        replies.push("Try another assignment");
        replies.extend(candidates.iter().copied());
    }
    replies.push("invalid graph");
    replies.push("Try another assignment");
    replies.extend(second_bad);
    replies.push("invalid graph");
    replies.push("Try another assignment");
    replies.extend(second_bad);
    replies.push(r#"{"x":1,"y":1,"z":1}"#);

    let mut seen = Vec::new();
    for (request_index, answer) in replies.into_iter().enumerate() {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
            .await
            .expect("timed out awaiting expected inference request")
            .unwrap();
        let mut bytes = Vec::new();
        let request = loop {
            let mut chunk = [0; 4096];
            let count = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk))
                .await
                .expect("timed out reading inference request")
                .unwrap();
            assert!(count > 0, "provider closed before request completed");
            bytes.extend_from_slice(&chunk[..count]);
            assert!(bytes.len() < 64_000, "fixture request exceeded bound");
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .expect("missing content-length")
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                        .unwrap();
                }
            }
        };
        assert_eq!(request["messages"][0]["role"], "system");
        assert_eq!(request["messages"][1]["role"], "user");
        let prompt = request["messages"][1]["content"].as_str().unwrap();
        let expected_query = if request_index < 11 {
            FIRST_QUERY
        } else {
            SECOND_QUERY
        };
        assert!(prompt.contains(expected_query));
        seen.push(request.clone());
        let body = response(answer);
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(250), listener.accept())
            .await
            .is_err()
    );
    seen
}

async fn stall_one_request(listener: tokio::net::TcpListener) {
    let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("comparison did not dispatch its first inference")
        .unwrap();
    let mut bytes = [0; 4096];
    let count = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut bytes))
        .await
        .expect("comparison did not write its provider request")
        .unwrap();
    assert!(count > 0);
    tokio::time::sleep(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn workflow_loopback_ablation_measures_saved_calls_and_an_observed_regression() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut cfg = Config {
        backend: "openai-local".into(),
        api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
        interpret_goals: false,
        max_calls: 16,
        max_cycles: 4,
        max_tokens: 5000,
        max_tool_calls: 8,
        max_wall_seconds: 45,
        resource_admission: Some(cogfabric::resource_scheduler::Configuration {
            settings: cogfabric::resource_scheduler::Settings {
                label: "benchmark_pool".into(),
                capacity: cogfabric::resource_scheduler::Allocation {
                    compute_slots: 1,
                    host_memory_bytes: 0,
                    device_memory_bytes: 0,
                },
                max_pending: 8,
                max_wait_ms: 1000,
                max_execution_ms: 60_000,
                scheduling: None,
            },
            allocation: cogfabric::resource_scheduler::Allocation {
                compute_slots: 1,
                host_memory_bytes: 0,
                device_memory_bytes: 0,
            },
            residents: Vec::new(),
            working_profiles: Vec::new(),
            gpu_observation: None,
        }),
        ..Config::default()
    };
    let root =
        std::env::temp_dir().join(format!("fabric-progress-ablation-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    cfg.home = root.join("state");
    cfg.save().unwrap();
    let dataset = root.join("cases.jsonl");
    std::fs::write(
        &dataset,
        format!(
            "{{\"input\":{},\"verification\":\"finite_domain_v1\"}}\n{{\"input\":{},\"verification\":\"finite_domain_v1\"}}\n",
            serde_json::to_string(FIRST_QUERY).unwrap(),
            serde_json::to_string(SECOND_QUERY).unwrap()
        ),
    )
    .unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let review = review();
    let server = tokio::spawn(serve(listener));
    let report = compare_progress_until(
        &cfg,
        store,
        &dataset,
        "research",
        Settings { plateau_cycles: 2 },
        &review,
        std::future::pending(),
    )
    .await
    .unwrap();
    let requests = server.await.unwrap();
    std::fs::remove_dir_all(root).unwrap();

    assert_eq!(requests.len(), 22);
    assert_eq!(report["complete"], true);
    assert_eq!(report["complete_cases"], 2);
    assert_eq!(report["corpus_snapshot"]["frozen"], true);
    assert_eq!(
        report["baseline_configuration"]["progress_stopping"],
        Value::Null
    );
    assert_eq!(
        report["progress_configuration"]["progress_stopping"]["plateau_cycles"],
        2
    );
    assert_eq!(
        report["progress_stopping_default_configuration_changed"],
        false
    );
    assert!(report["baseline_configuration_sha256"].as_str().is_some());
    assert!(report["progress_configuration_sha256"].as_str().is_some());
    assert_eq!(report["arms"][0]["executed"], 2);
    assert_eq!(report["arms"][1]["executed"], 2);
    assert_eq!(report["arms"][0]["resources"]["model_calls"], 12);
    assert_eq!(report["arms"][1]["resources"]["model_calls"], 10);
    assert_eq!(report["configured_resource_pool"], true);
    assert_eq!(report["configured_resource_pool_shared_across_arms"], true);
    assert_eq!(
        report["paired_resource_totals"]["delta_progress_minus_baseline"]["model_calls"],
        -2
    );
    assert_eq!(
        report["paired_outcomes"]["progress_stopping_wins"],
        json!([])
    );
    assert_eq!(
        report["paired_outcomes"]["progress_stopping_regressions"],
        json!([1])
    );
    assert_eq!(
        report["paired_outcomes"]["observed_regressions_with_baseline_witness"],
        1
    );
    assert_eq!(report["cases"][0]["baseline_passed"], false);
    assert_eq!(report["cases"][0]["progress_stopping_passed"], false);
    assert_eq!(report["cases"][1]["baseline_passed"], true);
    assert_eq!(report["cases"][1]["progress_stopping_passed"], false);
    assert_eq!(report["automatic_policy_installation"], false);
    assert_eq!(report["candidate_promotion"], false);
}

#[tokio::test]
async fn rejects_non_csp_expected_labels_and_adaptive_history_before_task_creation() {
    let root = std::env::temp_dir().join(format!(
        "fabric-progress-ablation-invalid-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let dataset = root.join("cases.jsonl");
    std::fs::write(
        &dataset,
        format!(
            "{{\"input\":{},\"verification\":\"finite_domain_v1\",\"expected\":\"answer\"}}\n",
            serde_json::to_string(FIRST_QUERY).unwrap()
        ),
    )
    .unwrap();
    let cfg = Config {
        home: root.join("state"),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let settings = Settings { plateau_cycles: 2 };
    assert!(compare_progress_until(
        &cfg,
        store.clone(),
        &dataset,
        "research",
        settings.clone(),
        &review(),
        std::future::pending(),
    )
    .await
    .is_err());
    assert_eq!(task_count(&cfg.db()), 0);

    std::fs::write(
        &dataset,
        format!(
            "{{\"input\":{},\"verification\":\"finite_domain_v1\",\"input\":{}}}\n",
            serde_json::to_string(FIRST_QUERY).unwrap(),
            serde_json::to_string(SECOND_QUERY).unwrap()
        ),
    )
    .unwrap();
    assert!(compare_progress_until(
        &cfg,
        store.clone(),
        &dataset,
        "research",
        Settings { plateau_cycles: 2 },
        &review(),
        std::future::pending(),
    )
    .await
    .is_err());
    assert_eq!(task_count(&cfg.db()), 0);

    std::fs::write(
        &dataset,
        format!(
            "{{\"input\":{},\"verification\":\"finite_domain_v1\"}}\n",
            serde_json::to_string(FIRST_QUERY).unwrap()
        ),
    )
    .unwrap();
    let search_cfg = Config {
        finite_search: Some(cogfabric::reasoning::finite_search::Settings { max_nodes: 4096 }),
        ..cfg.clone()
    };
    let error = compare_progress_until(
        &search_cfg,
        store.clone(),
        &dataset,
        "research",
        settings.clone(),
        &review(),
        std::future::pending(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("pre-model finite search"));
    assert_eq!(task_count(&cfg.db()), 0);

    let cfg = Config {
        advice_guard: Some(cogfabric::reasoning::advice_guard::Settings {
            min_observations: 1,
            sample_size: 1,
            window_seconds: 60,
            trip_failure_basis_points: 10_000,
        }),
        ..cfg.clone()
    };
    assert!(compare_progress_until(
        &cfg,
        store.clone(),
        &dataset,
        "research",
        settings,
        &review(),
        std::future::pending(),
    )
    .await
    .is_err());
    assert_eq!(task_count(&cfg.db()), 0);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn held_out_report_omits_objectives_and_answers() {
    let root = std::env::temp_dir().join(format!(
        "fabric-progress-ablation-heldout-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let dataset = root.join("cases.jsonl");
    std::fs::write(
        &dataset,
        format!(
            "{{\"input\":{},\"verification\":\"finite_domain_v1\"}}\n",
            serde_json::to_string(FIRST_QUERY).unwrap()
        ),
    )
    .unwrap();
    let mut held_out_review = review();
    held_out_review.held_out = true;
    held_out_review.workflow_check = false;
    let report = compare_progress_until(
        &Config::default(),
        Arc::new(Store::in_memory().unwrap()),
        &dataset,
        "deep",
        Settings { plateau_cycles: 2 },
        &held_out_review,
        std::future::pending(),
    )
    .await
    .unwrap();
    let serialized = serde_json::to_string(&report).unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["content_included"], false);
    assert_eq!(report["cases"][0]["input"], Value::Null);
    assert_eq!(report["cases"][0]["baseline"]["answer"], Value::Null);
    assert_eq!(
        report["cases"][0]["progress_stopping"]["answer"],
        Value::Null
    );
    assert!(!serialized.contains(FIRST_QUERY));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn cancellation_returns_an_explicit_partial_comparison_without_dispatching() {
    let root = std::env::temp_dir().join(format!(
        "fabric-progress-ablation-cancel-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let dataset = root.join("cases.jsonl");
    std::fs::write(
        &dataset,
        format!(
            "{{\"input\":{},\"verification\":\"finite_domain_v1\"}}\n",
            serde_json::to_string(FIRST_QUERY).unwrap()
        ),
    )
    .unwrap();
    let cfg = Config {
        home: root.join("state"),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let report = compare_progress_until(
        &cfg,
        store.clone(),
        &dataset,
        "research",
        Settings { plateau_cycles: 2 },
        &review(),
        std::future::ready(()),
    )
    .await
    .unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(report["stop_reason"], "cancelled");
    assert_eq!(report["corpus_snapshot"]["frozen"], true);
    assert_eq!(report["complete_cases"], 0);
    assert_eq!(report["paired_outcomes"]["unmatched_cases"], json!([0]));
    assert_eq!(report["arms"][0]["executed"], 0);
    assert_eq!(report["arms"][0]["resources"]["model_calls"], 0);
    assert_eq!(report["arms"][1]["executed"], 0);
    assert_eq!(
        report["arms"][0]["configuration"]["progress_stopping"],
        Value::Null
    );
    assert_eq!(
        report["arms"][1]["configuration"]["progress_stopping"]["plateau_cycles"],
        2
    );
    assert_eq!(task_count(&cfg.db()), 0);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn overall_deadline_is_reported_separately_from_cancellation_and_keeps_partial_costs() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = Config {
        backend: "openai-local".into(),
        api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
        interpret_goals: false,
        max_wall_seconds: 30,
        ..Config::default()
    };
    let root = std::env::temp_dir().join(format!(
        "fabric-progress-ablation-deadline-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let dataset = root.join("cases.jsonl");
    std::fs::write(
        &dataset,
        format!(
            "{{\"input\":{},\"verification\":\"finite_domain_v1\"}}\n",
            serde_json::to_string(FIRST_QUERY).unwrap()
        ),
    )
    .unwrap();
    let mut one_second_review = review();
    one_second_review.max_run_seconds = 1;
    let server = tokio::spawn(stall_one_request(listener));
    let report = compare_progress_until(
        &cfg,
        Arc::new(Store::in_memory().unwrap()),
        &dataset,
        "research",
        Settings { plateau_cycles: 2 },
        &one_second_review,
        std::future::pending(),
    )
    .await
    .unwrap();
    server.await.unwrap();
    std::fs::remove_dir_all(root).unwrap();

    assert_eq!(report["complete"], false);
    assert_eq!(report["stop_reason"], "run_deadline");
    assert_eq!(report["complete_cases"], 0);
    assert_eq!(report["paired_outcomes"]["unmatched_cases"], json!([0]));
    assert_eq!(report["arms"][0]["executed"], 1);
    assert_eq!(report["arms"][0]["resources"]["model_calls"], 1);
    assert_eq!(report["arms"][1]["executed"], 0);
    assert_eq!(
        report["paired_resource_totals"]["baseline"]["model_calls"],
        0
    );
    assert_eq!(
        report["paired_resource_totals"]["progress_stopping"]["model_calls"],
        0
    );
}
