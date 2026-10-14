use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn native_cycle_script_inspects_by_default_and_preserves_existing_instances() {
    for scenario in 0..4 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        let count = match scenario {
            0 | 1 => 1,
            2 => 9,
            _ => 5,
        };
        let server = tokio::spawn(async move {
            let mut loaded = scenario == 1;
            let mut drifted = false;
            let mut mutations = Vec::new();
            for _ in 0..count {
                let (mut stream, _) =
                    tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut bytes = Vec::new();
                let (head, body) = loop {
                    let mut chunk = [0; 2048];
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    assert!(bytes.len() < 16384);
                    if let Some(pos) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        let head = String::from_utf8(bytes[..pos].to_vec()).unwrap();
                        let length = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= pos + 4 + length {
                            break (head, bytes[pos + 4..pos + 4 + length].to_vec());
                        }
                    }
                };
                let response = if head.starts_with("GET /api/v1/models HTTP/1.1") {
                    json!({"models":[{"key":"existing.qwen","type":"llm","loaded_instances":if drifted {vec![]} else {vec![json!({"id":"existing","config":{"context_length":4096}})]}},{"key":"installed.embedding","type":"embedding","loaded_instances":if loaded {vec![json!({"id":"fresh","config":{"context_length":2048}})]} else {vec![]}}]})
                } else if head.starts_with("POST /api/v1/models/load HTTP/1.1") {
                    assert!(!loaded);
                    let request: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(
                        request,
                        json!({"model":"installed.embedding","context_length":2048})
                    );
                    loaded = true;
                    drifted = scenario == 3;
                    mutations.push("load");
                    json!({"instance_id":"fresh","status":"loaded","type":"embedding","load_time_seconds":0.1})
                } else if head.starts_with("POST /api/v1/models/unload HTTP/1.1") {
                    assert_eq!(scenario, 2);
                    assert!(loaded);
                    assert_eq!(
                        serde_json::from_slice::<Value>(&body).unwrap(),
                        json!({"instance_id":"fresh"})
                    );
                    loaded = false;
                    mutations.push("unload");
                    json!({"instance_id":"fresh"})
                } else {
                    panic!("Unexpected model API request: {head}")
                };
                let body = response.to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
            mutations
        });
        let output = tokio::task::spawn_blocking(move || {
            let mut cmd = std::process::Command::new("bash");
            cmd.arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/native-residency-smoke.sh"
            ))
            .args([
                env!("CARGO_BIN_EXE_cogfabric"),
                &base,
                "installed.embedding",
            ]);
            if scenario > 0 {
                cmd.arg("--human-approve");
            }
            cmd.output().unwrap()
        })
        .await
        .unwrap();
        let mutations = server.await.unwrap();
        assert_eq!(
            output.status.success(),
            scenario == 0 || scenario == 2,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            mutations,
            match scenario {
                2 => vec!["load", "unload"],
                3 => vec!["load"],
                _ => vec![],
            }
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let evidence = stdout
            .lines()
            .find_map(|l| l.strip_prefix("Residency evidence: "))
            .unwrap();
        if scenario == 2 {
            let report: Value = serde_json::from_slice(
                &std::fs::read(std::path::Path::new(evidence).join("result.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(report["server_confirmed_cycle"], true);
            assert_eq!(report["ownership_established"], false);
        }
        if scenario == 3 {
            assert!(!std::path::Path::new(evidence).join("unload.json").exists());
        }
        std::fs::remove_dir_all(evidence).unwrap();
    }
}
