use axum::{extract::State, routing::post, Router};
use cogfabric::{config::Config, store::Store};
use serde_json::json;
use std::{
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
const KEY: &str = "collective-cleanup-fixture-key-1234567890";
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn pending(State(entered): State<Arc<tokio::sync::Notify>>) -> &'static str {
    entered.notify_one();
    std::future::pending().await
}
#[tokio::test]
async fn collective_disconnect_and_immediate_shutdown_reap_owned_inference() {
    for (shutdown_immediately, audit_failure) in [(false, false), (true, false), (false, true)] {
        let entered = Arc::new(tokio::sync::Notify::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream = listener.local_addr().unwrap();
        let provider = tokio::spawn(
            axum::serve(
                listener,
                Router::new()
                    .route("/v1/chat/completions", post(pending))
                    .with_state(entered.clone()),
            )
            .into_future(),
        );
        let home = std::env::temp_dir().join(format!(
            "fabric-collective-cleanup-{}-{shutdown_immediately}-{audit_failure}",
            std::process::id()
        ));
        let cfg = Config {
            home: home.clone(),
            backend: "openai-local".into(),
            model: "cleanup-fixture".into(),
            api_base: format!("http://{upstream}/v1"),
            inference_process: true,
            max_wall_seconds: 30,
            timeout_seconds: 25,
            ..Config::default()
        };
        cfg.save().unwrap();
        let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reserve.local_addr().unwrap();
        drop(reserve);
        let mut server = Owned(
            Command::new(env!("CARGO_BIN_EXE_cogfabric-server"))
                .arg("--home")
                .arg(&home)
                .arg("--bind")
                .arg(address.to_string())
                .env_clear()
                .env("COGFABRIC_SERVER_API_KEY", KEY)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
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
                assert!(server.0.try_wait().unwrap().is_none());
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let mut connection = tokio::net::TcpStream::connect(address).await.unwrap();
        let body = json!({"model":"cogfabric-fast","messages":[{"role":"user","content":"Describe a tree"}]}).to_string();
        connection.write_all(format!("POST /v1/chat/completions HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {KEY}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .unwrap();
        let store = Store::open(&cfg.db()).unwrap();
        assert!(store.recover_workers().is_err());
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        let owned: String = db
            .query_row(
                "SELECT payload FROM events WHERE kind='worker_process_owned'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let pid = serde_json::from_str::<serde_json::Value>(&owned).unwrap()["pid"]
            .as_i64()
            .unwrap();
        if audit_failure {
            db.execute_batch("CREATE TRIGGER fail_collective_stop BEFORE INSERT ON events WHEN NEW.kind='worker_process_stopped' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        }
        drop(connection);
        if shutdown_immediately {
            assert_eq!(unsafe { libc::kill(server.0.id() as i32, libc::SIGINT) }, 0);
        }
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let reaped: u64 = db.query_row("SELECT count(*) FROM events WHERE kind='worker_process_stopped' AND json_extract(payload,'$.reaped')=1", [], |row| row.get(0)).unwrap();
                if reaped == 1 || (audit_failure && unsafe { libc::kill(pid as i32, 0) } == -1) { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        if audit_failure {
            let retry = client.post(format!("http://{address}/v1/chat/completions")).bearer_auth(KEY)
                .json(&json!({"model":"cogfabric-fast","messages":[{"role":"user","content":"Try again"}]})).send().await.unwrap();
            assert_eq!(retry.status(), 502);
            let launched: u64 = db
                .query_row(
                    "SELECT count(*) FROM events WHERE kind='worker_process_launch'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(launched, 1);
        }
        if !shutdown_immediately {
            assert_eq!(unsafe { libc::kill(server.0.id() as i32, libc::SIGINT) }, 0);
        }
        let status = tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if let Some(status) = server.0.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(status.success(), !audit_failure);
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        store.recover_workers().unwrap();
        drop(db);
        drop(store);
        drop(server);
        provider.abort();
        let _ = provider.await;
        std::fs::remove_dir_all(home).unwrap();
    }
}
use std::future::IntoFuture;
