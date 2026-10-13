use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn native_residency_cli_scopes_auth_and_rejects_untrusted_responses() {
    for scenario in 0..6 {
        let home = std::env::temp_dir().join(format!("fabric-residency-{}", uuid::Uuid::new_v4()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let token = "residency-fixture-token";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            while !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let n = stream.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 8192);
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.starts_with("GET /api/v1/models HTTP/1.1"));
            assert_eq!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer residency-fixture-token"),
                scenario != 0
            );
            assert!(!request.contains("primary-inference-fixture-token"));
            let body = match scenario {
                2 => r#"{"models":[],"echo":"\u0072esidency-fixture-token"}"#.into(),
                4 => "x".repeat(262145),
                5 => "{malformed".into(),
                _ => json!({"models":[{"key":"installed","type":"llm","loaded_instances":[{"id":"running-instance","config":{"context_length":2048}}]}]}).to_string(),
            };
            let status = if scenario == 3 {
                "302 Found\r\nLocation: http://127.0.0.1:1/steal"
            } else {
                "200 OK"
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            // The client may close before reading an oversized or redirect body.
            let _ = stream.write_all(response.as_bytes()).await;
        });
        let task_home = home.clone();
        let output = tokio::task::spawn_blocking(move || {
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"));
            command.arg("--home").arg(task_home).args([
                "--json",
                "residency-status",
                "--api-base",
                &base,
            ]);
            command.env(
                "COGFABRIC_INFERENCE_API_KEY",
                "primary-inference-fixture-token",
            );
            command.env("COGFABRIC_RESIDENCY_API_KEY", token);
            if scenario != 0 {
                command.arg("--authenticate");
            }
            command.output().unwrap()
        })
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(
            output.status.success(),
            scenario < 2,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for bytes in [&output.stdout, &output.stderr] {
            assert!(!String::from_utf8_lossy(bytes).contains(token));
        }
        if scenario < 2 {
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["loaded_instances"], 1);
            assert_eq!(
                report["models"][0]["loaded_instances"][0]["id"],
                "running-instance"
            );
            assert_eq!(report["ownership_established"], false);
            assert_eq!(report["generation_tested"], false);
        }
        std::fs::remove_dir_all(home).unwrap();
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length: usize = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .map(|v| v.parse().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
        let mut chunk = [0; 2048];
        let n = stream.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() < 16384);
    }
}

#[tokio::test]
async fn cli_residency_mutations_audit_confirm_and_preserve_unknown_outcomes() {
    for scenario in 0..5 {
        let home =
            std::env::temp_dir().join(format!("fabric-residency-change-{}", uuid::Uuid::new_v4()));
        let cfg = cogfabric::config::Config {
            home: home.clone(),
            ..Default::default()
        };
        cfg.prepare().unwrap();
        let store = cogfabric::store::Store::open(&cfg.db()).unwrap();
        if scenario == 4 {
            rusqlite::Connection::open(cfg.db()).unwrap().execute_batch("CREATE TRIGGER refuse_residency_intent BEFORE INSERT ON events WHEN NEW.kind='model_residency' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let server_store = cfg.db();
        let server = tokio::spawn(async move {
            let count = if scenario >= 3 {
                1
            } else if scenario == 2 {
                2
            } else {
                3
            };
            for call in 0..count {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                let body = if call == 1 {
                    let db = rusqlite::Connection::open(&server_store).unwrap();
                    let recorded: i64 = db.query_row("SELECT COUNT(*) FROM events WHERE kind='model_residency' AND json_extract(payload,'$.state')='requested'", [], |r| r.get(0)).unwrap();
                    assert_eq!(recorded, 1, "Intent must exist before POST");
                    if scenario == 1 {
                        assert!(request.starts_with("POST /api/v1/models/unload "));
                        assert!(request.contains(r#""instance_id":"instance""#));
                        json!({"instance_id":"instance"})
                    } else {
                        assert!(request.starts_with("POST /api/v1/models/load "));
                        assert!(request.contains(r#""context_length":2048"#));
                        if scenario == 2 { json!({"status":"loaded","instance_id":"unexpected"}) }
                        else { json!({"type":"llm","status":"loaded","instance_id":"instance","load_time_seconds":0.25}) }
                    }
                } else {
                    assert!(request.starts_with("GET /api/v1/models "));
                    let loaded = if call == 0 { scenario == 1 || scenario == 3 } else { scenario == 0 };
                    json!({"models":[{"key":"installed","type":"llm","loaded_instances":if loaded { json!([{"id":"instance","config":{"context_length":2048}}]) } else { json!([]) }}]})
                }.to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let task_home = home.clone();
        let output = tokio::task::spawn_blocking(move || {
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"));
            command.arg("--home").arg(task_home).args([
                "--json",
                if scenario == 1 {
                    "residency-unload"
                } else {
                    "residency-load"
                },
                "--api-base",
                &base,
                "--human-approve",
            ]);
            if scenario == 1 {
                command.args(["--instance-id", "instance"]);
            } else {
                command.args(["--model", "installed"]);
            }
            command.output().unwrap()
        })
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(
            output.status.success(),
            scenario < 2,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        if scenario < 2 {
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["server_confirmed"], true);
            assert_eq!(report["ownership_established"], false);
            assert_eq!(
                store.task(report["task_id"].as_str().unwrap()).unwrap()["task"]["status"],
                "completed"
            );
        } else if scenario == 2 {
            let state: String = db
                .query_row("SELECT status FROM tasks", [], |r| r.get(0))
                .unwrap();
            assert_eq!(state, "interrupted");
            let unknown: i64 = db.query_row("SELECT COUNT(*) FROM events WHERE json_extract(payload,'$.state')='outcome_unknown'",[],|r|r.get(0)).unwrap();
            assert_eq!(unknown, 1);
        } else {
            let posts: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE kind='model_residency'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(posts, 0);
        }
        drop(db);
        drop(store);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[tokio::test]
async fn operation_inspection_observes_inventory_without_resolving_or_mutating_tasks() {
    for (load, present) in [(true, true), (true, false), (false, true), (false, false)] {
        let home =
            std::env::temp_dir().join(format!("fabric-residency-inspect-{}", uuid::Uuid::new_v4()));
        let cfg = cogfabric::config::Config {
            home: home.clone(),
            ..Default::default()
        };
        cfg.save().unwrap();
        let store = cogfabric::store::Store::open(&cfg.db()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let action = if load {
            json!({"operation":"load","model":"installed","context_length":2048})
        } else {
            json!({"operation":"unload","instance_id":"instance"})
        };
        let task = store
            .create_task("Operator residency unknown outcome", "fast")
            .unwrap();
        store.event(&task, "model_residency", "human-cli", json!({"state":"requested","adapter":"lmstudio-native-v1","endpoint":format!("{base}/models"),"action":action,"external_outcome":"unknown"})).unwrap();
        store
            .finish_task(&task, "interrupted", "Unknown outcome")
            .unwrap();
        let before = store.task(&task).unwrap();
        let config_before = std::fs::read(home.join("config.json")).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            assert!(request.starts_with("GET /api/v1/models "));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            let body = json!({"models":[{"key":"installed","type":"llm","loaded_instances":if present { json!([{"id":"instance","config":{"context_length":2048}}]) } else { json!([]) }}]}).to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                    .await
                    .is_err(),
                "Inspection must issue exactly one GET, never a mutation/retry"
            );
        });
        let command_home = home.clone();
        let id = task.clone();
        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
                .env_clear()
                .arg("--home")
                .arg(command_home)
                .args(["--json", "residency-inspect", &id, "--api-base", &base])
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        server.await.unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["desired_state_observed"], load == present);
        assert_eq!(report["operation_outcome"], "not_inferred");
        assert_eq!(report["ownership_established"], false);
        assert_eq!(report["model_mutation_performed"], false);
        assert_eq!(report["task_updated"], false);
        assert_eq!(report["task_state_at_read"], "interrupted");
        assert_eq!(store.task(&task).unwrap(), before);
        assert_eq!(
            std::fs::read(home.join("config.json")).unwrap(),
            config_before
        );
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        let skills: u64 = db
            .query_row("SELECT count(*) FROM skills", [], |r| r.get(0))
            .unwrap();
        assert_eq!(skills, 0, "Read-only startup must not bootstrap skills");
        drop(db);
        drop(store);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[tokio::test]
async fn operation_inspection_rejects_ambiguous_corrupt_or_wrong_endpoint_intent_before_network() {
    for scenario in 0..7 {
        let home = std::env::temp_dir().join(format!(
            "fabric-residency-inspect-deny-{}",
            uuid::Uuid::new_v4()
        ));
        let cfg = cogfabric::config::Config {
            home: home.clone(),
            ..Default::default()
        };
        cfg.save().unwrap();
        let store = cogfabric::store::Store::open(&cfg.db()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let task = store
            .create_task("Invalid residency intent", "fast")
            .unwrap();
        let mut intent = json!({"state":"requested","adapter":"lmstudio-native-v1","endpoint":format!("{base}/models"),"action":{"operation":"load","model":"installed","context_length":2048},"external_outcome":"unknown"});
        match scenario {
            0 => intent["endpoint"] = json!("http://127.0.0.1:1/api/v1/models"),
            1 => intent["action"]["context_length"] = json!(32769),
            2 => intent["action"]["permissions"] = json!(["shell"]),
            3 => intent["adapter"] = json!("arbitrary-provider"),
            4 => intent["action"]["model"] = json!("x".repeat(5000)),
            _ => {}
        }
        if scenario != 6 {
            store
                .event(&task, "model_residency", "human-cli", intent.clone())
                .unwrap();
            if scenario == 5 {
                store
                    .event(&task, "model_residency", "human-cli", intent)
                    .unwrap();
            }
        }
        let before = store.task(&task).unwrap();
        let command_home = home.clone();
        let id = task.clone();
        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
                .env_clear()
                .arg("--home")
                .arg(command_home)
                .args(["--json", "residency-inspect", &id, "--api-base", &base])
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert!(!result.status.success());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), listener.accept())
                .await
                .is_err(),
            "Invalid intent must fail before contacting any provider"
        );
        assert_eq!(store.task(&task).unwrap(), before);
        drop(store);
        std::fs::remove_dir_all(home).unwrap();
    }
    let missing = std::env::temp_dir().join(format!(
        "fabric-residency-inspect-missing-{}",
        uuid::Uuid::new_v4()
    ));
    let command_home = missing.clone();
    let result = tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .env_clear()
            .arg("--home")
            .arg(command_home)
            .args([
                "--json",
                "residency-inspect",
                &uuid::Uuid::new_v4().to_string(),
                "--api-base",
                "http://127.0.0.1:1/api/v1",
            ])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(!result.status.success());
    assert!(
        !missing.exists(),
        "Inspection must not initialize a missing workspace"
    );
}
