use cogfabric::{
    benchmarking::{progress::compare_progress_until, Review},
    config::Config,
    reasoning::progress::Settings,
    store::Store,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const FIRST_QUERY: &str = r#"path:{"nodes":["s","a","g"],"edges":[["s","a"],["a","g"],["s","g"]],"start":"s","goal":"g"}"#;
const SECOND_QUERY: &str = r#"path:{"nodes":["x","y","z"],"edges":[["x","y"],["y","z"],["x","z"]],"start":"x","goal":"z"}"#;

fn review() -> Review {
    Review {
        source: "synthetic shortest-path loopback fixture".into(),
        license: "CC0-1.0".into(),
        code_revision: "a".repeat(40),
        held_out: false,
        workflow_check: true,
        expected_dataset_sha256: None,
        max_run_seconds: 30,
    }
}

fn path_answer(prompt: &str) -> &'static str {
    if prompt.contains(FIRST_QUERY) {
        r#"["s","g"]"#
    } else if prompt.contains(SECOND_QUERY) {
        r#"["x","z"]"#
    } else {
        panic!("request omitted both reviewed path objectives")
    }
}

async fn serve(listener: tokio::net::TcpListener) -> Vec<Value> {
    let mut requests = Vec::new();
    loop {
        let accepted = tokio::time::timeout(Duration::from_millis(300), listener.accept()).await;
        let Ok(Ok((mut stream, _))) = accepted else {
            break;
        };
        let mut bytes = Vec::new();
        let request = loop {
            let mut chunk = [0; 4096];
            let count = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk))
                .await
                .expect("timed out reading provider request")
                .unwrap();
            assert!(count > 0, "provider closed before request completed");
            bytes.extend_from_slice(&chunk[..count]);
            assert!(bytes.len() <= 64_000, "fixture request exceeded bound");
            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]).to_ascii_lowercase();
            let body_length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .expect("missing content-length")
                .trim()
                .parse::<usize>()
                .unwrap();
            if bytes.len() >= header_end + 4 + body_length {
                break serde_json::from_slice::<Value>(
                    &bytes[header_end + 4..header_end + 4 + body_length],
                )
                .unwrap();
            }
        };
        let prompt = request["messages"][1]["content"]
            .as_str()
            .expect("missing user prompt");
        let answer = path_answer(prompt);
        let body = json!({
            "model": "path-comparison-fixture/v1",
            "choices": [{"message": {"content": answer}}],
            "usage": {"prompt_tokens": 11, "completion_tokens": 3}
        })
        .to_string();
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
        requests.push(request);
    }
    requests
}

#[tokio::test]
async fn path_corpus_comparison_reports_structured_criterion_and_independent_witness_checks() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = std::env::temp_dir().join(format!(
        "fabric-path-progress-comparison-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let mut cfg = Config {
        backend: "openai-local".into(),
        api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
        interpret_goals: false,
        max_calls: 12,
        max_cycles: 4,
        max_tokens: 5_000,
        max_tool_calls: 8,
        timeout_seconds: 10,
        max_wall_seconds: 30,
        ..Config::default()
    };
    cfg.home = root.join("state");
    cfg.save().unwrap();
    let dataset = root.join("paths.jsonl");
    std::fs::write(
        &dataset,
        format!(
            "{{\"input\":{},\"verification\":\"shortest_path_v1\"}}\n{{\"input\":{},\"verification\":\"shortest_path_v1\"}}\n",
            serde_json::to_string(FIRST_QUERY).unwrap(),
            serde_json::to_string(SECOND_QUERY).unwrap()
        ),
    )
    .unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let server = tokio::spawn(serve(listener));

    let report = compare_progress_until(
        &cfg,
        store,
        &dataset,
        "research",
        Settings { plateau_cycles: 2 },
        &review(),
        std::future::pending(),
    )
    .await
    .unwrap();
    let requests = server.await.unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(report["complete"], true);
    assert_eq!(report["complete_cases"], 2);
    assert_eq!(report["experiment"], "structured_progress_stopping/v2");
    assert_eq!(report["criterion"], "shortest_path/v1");
    assert_eq!(
        report["progress_stopping_default_configuration_changed"],
        false
    );
    assert_eq!(
        report["arms"][0]["configuration"]["progress_stopping"],
        Value::Null
    );
    assert_eq!(
        report["arms"][1]["configuration"]["progress_stopping"]["plateau_cycles"],
        2
    );
    for key in [
        "max_calls",
        "max_cycles",
        "max_tokens",
        "max_tool_calls",
        "timeout_seconds",
        "max_wall_seconds",
    ] {
        assert_eq!(
            report["arms"][0]["configuration"][key], report["arms"][1]["configuration"][key],
            "comparison changed shared budget {key}"
        );
    }
    assert_eq!(
        requests.len() as u64,
        report["arms"][0]["resources"]["model_calls"]
            .as_u64()
            .unwrap()
            + report["arms"][1]["resources"]["model_calls"]
                .as_u64()
                .unwrap()
    );
    for (index, expected) in [r#"["s","g"]"#, r#"["x","z"]"#].iter().enumerate() {
        let case = &report["cases"][index];
        assert_eq!(case["verification"], "shortest_path_v1");
        for arm in ["baseline", "progress_stopping"] {
            let result = &case[arm];
            assert_eq!(result["status"], "completed");
            assert_eq!(result["answer"], *expected);
            assert_eq!(result["passed_independent_criterion_check"], true);
            assert_eq!(
                result["passed_independent_finite_domain_check"],
                Value::Null
            );
        }
        assert_eq!(case["baseline_passed"], true);
        assert_eq!(case["progress_stopping_passed"], true);
    }
}
