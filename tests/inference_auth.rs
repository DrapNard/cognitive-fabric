use cogfabric::{config::Config, store::Store};
use std::{
    io::{Read, Write},
    process::Command,
    time::Duration,
};

const KEY: &str = "fixture-auth-key-123456789";

#[test]
fn scoped_authentication_works_in_direct_and_managed_inference_and_rejects_echoes() {
    for managed in [false, true] {
        for echo in [false, true] {
            let home = std::env::temp_dir().join(format!("fabric-auth-{}", uuid::Uuid::new_v4()));
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(started.elapsed() < Duration::from_secs(15));
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut raw = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    raw.extend_from_slice(&buffer[..count]);
                    assert!(raw.len() < 64000);
                    if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
                        assert!(headers.contains(&format!("authorization: bearer {KEY}")));
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("content-length:")
                                    .map(|n| n.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        if raw.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let content = if echo { KEY } else { "An advisory answer" };
                let body =
                    serde_json::json!({"choices":[{"message":{"content":content}}]}).to_string();
                // Escaped provider echoes must also be rejected after JSON decoding.
                let body = if echo {
                    body.replace(
                        KEY,
                        &KEY.bytes()
                            .map(|b| format!("\\u{b:04x}"))
                            .collect::<String>(),
                    )
                } else {
                    body
                };
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            });
            let cfg = Config {
                home: home.clone(),
                backend: "openai-local".into(),
                api_base: endpoint.clone(),
                inference_auth_scope: Some(endpoint),
                inference_process: managed,
                interpret_goals: false,
                ..Config::default()
            };
            cfg.save().unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
                .arg("--home")
                .arg(&home)
                .arg("--json")
                .args(["ask", "Explain ownership", "--mode", "fast"])
                .env(cogfabric::credentials::ENVIRONMENT_VARIABLE, KEY)
                .output()
                .unwrap();
            server.join().unwrap();
            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(!stdout.contains(KEY));
            assert!(!stderr.contains(KEY));
            let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
            if !echo {
                assert_eq!(result["status"], "unverified");
            } else {
                assert_ne!(result["status"], "unverified");
            }
            let store = Store::open(&cfg.db()).unwrap();
            let trace = store.task(result["task_id"].as_str().unwrap()).unwrap();
            assert!(!trace.to_string().contains(KEY));
            assert!(!std::fs::read_to_string(home.join("config.json"))
                .unwrap()
                .contains(KEY));
            drop(store);
            std::fs::remove_dir_all(home).unwrap();
        }
    }
}
