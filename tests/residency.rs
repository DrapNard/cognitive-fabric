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
