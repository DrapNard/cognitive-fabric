use cogfabric::{
    config::Config,
    residency::{
        planning::{self, Plan, Request},
        Mutation,
    },
    store::Store,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn inventory() -> Value {
    json!({"models":[{"key":"a","type":"llm","loaded_instances":[{"id":"a-existing","config":{"context_length":2048}}]},{"key":"b","type":"llm","loaded_instances":[]},{"key":"c","type":"llm","loaded_instances":[{"id":"c-old","config":{"context_length":1024}}]},{"key":"d","type":"llm","loaded_instances":[]}]})
}
fn request() -> Request {
    serde_json::from_value(json!({"ensure_loaded":[{"model":"a","context_length":2048},{"model":"b","context_length":4096}],"release_instances":["c-old"]})).unwrap()
}
fn persistent() -> (Config, Arc<Store>) {
    let cfg = Config {
        home: std::env::temp_dir().join(format!("fabric-residency-plan-{}", uuid::Uuid::new_v4())),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    (cfg, store)
}
async fn exchange(listener: &tokio::net::TcpListener, expected: &str, body: Value) -> Value {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut bytes = Vec::new();
    let (end, length) = loop {
        let mut chunk = [0u8; 2048];
        let n = stream.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() < 16000);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            let length = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .map(|v| v.trim().parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                assert!(String::from_utf8_lossy(&bytes).starts_with(expected));
                break (end, length);
            }
        }
    };
    let request = if length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap()
    };
    let body = body.to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    request
}
#[tokio::test]
async fn plan_reuses_then_applies_only_explicit_release_and_missing_load_and_denies_replay() {
    let (cfg, store) = persistent();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let initial = inventory();
        let mut released = initial.clone();
        released["models"][2]["loaded_instances"] = json!([]);
        let mut loaded = released.clone();
        loaded["models"][1]["loaded_instances"] =
            json!([{"id":"b-new","config":{"context_length":4096}}]);
        for _ in 0..3 {
            exchange(&listener, "GET /api/v1/models ", initial.clone()).await;
        }
        let request = exchange(
            &listener,
            "POST /api/v1/models/unload ",
            json!({"instance_id":"c-old","status":"unloaded"}),
        )
        .await;
        assert_eq!(request, json!({"instance_id":"c-old"}));
        for _ in 0..2 {
            exchange(&listener, "GET /api/v1/models ", released.clone()).await;
        }
        let request = exchange(
            &listener,
            "POST /api/v1/models/load ",
            json!({"instance_id":"b-new","status":"loaded","type":"llm","load_time_seconds":0.01}),
        )
        .await;
        assert_eq!(request, json!({"model":"b","context_length":4096}));
        exchange(&listener, "GET /api/v1/models ", loaded).await;
        // Simulate an external reset to the original inventory. The used receipt,
        // rather than only the inventory hash, must still prevent replay.
        exchange(&listener, "GET /api/v1/models ", initial).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
    });
    let plan = planning::preview(&base, false, &request()).await.unwrap();
    assert_eq!(plan.reused[0].instance_id, "a-existing");
    assert_eq!(
        plan.actions,
        vec![
            Mutation::Unload {
                instance_id: "c-old".into()
            },
            Mutation::Load {
                model: "b".into(),
                context_length: 4096,
                offload_kv_cache_to_gpu: None,
            }
        ]
    );
    let report = planning::apply(&store, &base, false, &plan, true)
        .await
        .unwrap();
    assert_eq!(report["state"], "completed");
    assert_eq!(report["confirmed_steps"].as_array().unwrap().len(), 2);
    assert_eq!(report["ownership_established"], false);
    assert!(planning::apply(&store, &base, false, &plan, true)
        .await
        .is_err());
    server.await.unwrap();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.execute("UPDATE tasks SET updated=0", []).unwrap();
    drop(db);
    assert!(store.retention_preview(1, 100).unwrap().task_ids.is_empty());
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
#[tokio::test]
async fn unrelated_inventory_change_after_a_post_stops_remaining_operations_and_preserves_unknown_outcome(
) {
    let (cfg, store) = persistent();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let initial = inventory();
        for _ in 0..3 {
            exchange(&listener, "GET /api/v1/models ", initial.clone()).await;
        }
        exchange(
            &listener,
            "POST /api/v1/models/load ",
            json!({"instance_id":"b-new","status":"loaded","type":"llm","load_time_seconds":0.01}),
        )
        .await;
        let mut changed = initial;
        changed["models"][1]["loaded_instances"] =
            json!([{"id":"b-new","config":{"context_length":4096}}]);
        changed["models"][0]["loaded_instances"][0]["config"]["context_length"] = json!(8192);
        exchange(&listener, "GET /api/v1/models ", changed).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
    });
    let request:Request=serde_json::from_value(json!({"ensure_loaded":[{"model":"b","context_length":4096},{"model":"d","context_length":2048}]})).unwrap();
    let plan = planning::preview(&base, false, &request).await.unwrap();
    let report = planning::apply(&store, &base, false, &plan, true)
        .await
        .unwrap();
    assert_eq!(report["state"], "interrupted");
    assert_eq!(report["confirmed_steps"], json!([]));
    let trace = store.task(report["task_id"].as_str().unwrap()).unwrap();
    let child = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "residency_plan_step")
        .unwrap()["payload"]["operation_task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let operation = store.task(&child).unwrap();
    assert!(operation["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "model_residency" && e["payload"]["state"] == "outcome_unknown"));
    server.await.unwrap();
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
#[tokio::test]
async fn stale_inventory_and_failed_claim_audits_prevent_all_posts_and_task_initialization() {
    for audit_failure in [false, true] {
        let (cfg, store) = persistent();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
        if audit_failure {
            let db = rusqlite::Connection::open(cfg.db()).unwrap();
            db.execute_batch("CREATE TRIGGER refuse BEFORE INSERT ON events WHEN NEW.kind='residency_plan' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        }
        let server = tokio::spawn(async move {
            let initial = inventory();
            exchange(&listener, "GET /api/v1/models ", initial.clone()).await;
            let mut observed = initial;
            if !audit_failure {
                observed["models"][0]["loaded_instances"][0]["config"]["context_length"] =
                    json!(8192);
            }
            exchange(&listener, "GET /api/v1/models ", observed).await;
            assert!(
                tokio::time::timeout(Duration::from_millis(20), listener.accept())
                    .await
                    .is_err()
            );
        });
        let plan = planning::preview(&base, false, &request()).await.unwrap();
        assert!(planning::apply(&store, &base, false, &plan, true)
            .await
            .is_err());
        server.await.unwrap();
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        let tasks: u64 = db
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tasks, 0);
        drop(db);
        drop(store);
        std::fs::remove_dir_all(cfg.home).unwrap();
    }
}
#[tokio::test]
async fn configured_pool_ownership_blocks_idle_residency_changes_and_batch_application() {
    let (mut cfg, store) = persistent();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        exchange(&listener, "GET /api/v1/models ", inventory()).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(80), listener.accept())
                .await
                .is_err()
        );
    });
    let plan = planning::preview(&base, false, &request()).await.unwrap();
    cfg.resource_admission=Some(serde_json::from_value(json!({"settings":{"label":"gpu.main","capacity":{"compute_slots":1,"host_memory_bytes":100,"device_memory_bytes":100},"max_pending":4,"max_wait_ms":1000,"max_execution_ms":1000},"allocation":{"compute_slots":1,"host_memory_bytes":20,"device_memory_bytes":20}})).unwrap());
    let engine = cogfabric::engine::Engine::new(cfg.clone(), store.clone());
    let pool = engine.configured_resource_pool().unwrap().unwrap();
    assert!(planning::apply(&store, &base, false, &plan, true)
        .await
        .is_err());
    assert!(cogfabric::residency::mutate(
        &store,
        &base,
        false,
        Mutation::Unload {
            instance_id: "c-old".into()
        },
        true
    )
    .await
    .is_err());
    server.await.unwrap();
    drop(pool);
    drop(engine);
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
#[tokio::test]
async fn cli_preview_is_read_only_and_apply_requires_approval_before_file_or_state_access() {
    let root = std::env::temp_dir().join(format!("fabric-plan-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let home = root.join("state");
    let file = root.join("request.json");
    std::fs::write(&file, serde_json::to_vec(&request()).unwrap()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        exchange(&listener, "GET /api/v1/models ", inventory()).await;
    });
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("--json")
        .arg("residency-plan")
        .arg(&file)
        .arg("--api-base")
        .arg(&base)
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let _: Plan = serde_json::from_slice(&result.stdout).unwrap();
    server.await.unwrap();
    assert!(!home.exists());
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("residency-apply")
        .arg(root.join("missing.json"))
        .arg("--api-base")
        .arg(&base)
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("explicit operator approval"));
    assert!(!home.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn inventory_drift_between_steps_stops_before_the_next_post() {
    let (cfg, store) = persistent();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let initial = inventory();
        for _ in 0..3 {
            exchange(&listener, "GET /api/v1/models ", initial.clone()).await;
        }
        exchange(
            &listener,
            "POST /api/v1/models/unload ",
            json!({"instance_id":"c-old","status":"unloaded"}),
        )
        .await;
        let mut released = initial;
        released["models"][2]["loaded_instances"] = json!([]);
        exchange(&listener, "GET /api/v1/models ", released.clone()).await;
        released["models"][0]["loaded_instances"][0]["config"]["context_length"] = json!(8192);
        exchange(&listener, "GET /api/v1/models ", released).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
    });
    let plan = planning::preview(&base, false, &request()).await.unwrap();
    let report = planning::apply(&store, &base, false, &plan, true)
        .await
        .unwrap();
    assert_eq!(report["state"], "interrupted");
    assert_eq!(report["confirmed_steps"].as_array().unwrap().len(), 1);
    let trace = store.task(report["task_id"].as_str().unwrap()).unwrap();
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "residency_plan_step")
            .count(),
        1
    );
    server.await.unwrap();
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
#[tokio::test]
async fn actual_cli_applies_reviewed_reuse_plan_without_loading_another_model() {
    let root = std::env::temp_dir().join(format!("fabric-reuse-plan-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let home = root.join("state");
    let request_file = root.join("request.json");
    let plan_file = root.join("plan.json");
    std::fs::write(
        &request_file,
        json!({"ensure_loaded":[{"model":"a","context_length":2048}]}).to_string(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            exchange(&listener, "GET /api/v1/models ", inventory()).await;
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
    });
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("--json")
        .arg("residency-plan")
        .arg(&request_file)
        .arg("--api-base")
        .arg(&base)
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(result.status.success());
    std::fs::write(&plan_file, &result.stdout).unwrap();
    let cfg = Config {
        home: home.clone(),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("--json")
        .arg("residency-apply")
        .arg(&plan_file)
        .arg("--api-base")
        .arg(&base)
        .arg("--human-approve")
        .env_clear()
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["state"], "completed");
    assert_eq!(report["planned_operations"], 0);
    assert_eq!(report["reused"][0]["instance_id"], "a-existing");
    server.await.unwrap();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
