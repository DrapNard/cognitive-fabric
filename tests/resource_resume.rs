use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend, InferenceTarget, LocalBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    resource_scheduler::Configuration,
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
// A fork can temporarily inherit another thread's CLOEXEC lease until exec.
// Keep in-process recovery fixtures parallel, but exclude child launch while
// they assert immediate exclusive acquisition. Production locking stays strict.
static PROCESS_LAUNCH: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());
struct Script {
    cfg: Config,
    answers: Mutex<VecDeque<&'static str>>,
}
impl InferenceBackend for Script {
    fn inference_target(&self, role: &str) -> Result<Option<InferenceTarget>> {
        LocalBackend(self.cfg.clone()).inference_target(role)
    }
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let text = self.answers.lock().unwrap().pop_front().unwrap();
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "fixture-model".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: None,
                provider_metrics: None,
            })
        })
    }
}
async fn fixture(base: &str, managed: bool) -> (Config, Arc<Store>, String, String) {
    fixture_mode(base, managed, "deep", true).await
}
async fn fixture_mode(
    base: &str,
    managed: bool,
    mode: &str,
    with_residents: bool,
) -> (Config, Arc<Store>, String, String) {
    fixture_mode_policy(base, managed, mode, with_residents, false).await
}
async fn fixture_mode_policy(
    base: &str,
    managed: bool,
    mode: &str,
    with_residents: bool,
    priority: bool,
) -> (Config, Arc<Store>, String, String) {
    let root =
        std::env::temp_dir().join(format!("fabric-resource-resume-{}", uuid::Uuid::new_v4()));
    let mut resources:Configuration=serde_json::from_value(json!({"settings":{"label":"gpu.original","capacity":{"compute_slots":1,"host_memory_bytes":100,"device_memory_bytes":100},"max_pending":4,"max_wait_ms":5000,"max_execution_ms":5000},"allocation":{"compute_slots":1,"host_memory_bytes":20,"device_memory_bytes":20},"residents":[{"id":"backbone.original","version":"1.0.0","weights_sha256":"a".repeat(64),"host_memory_bytes":30,"device_memory_bytes":30,"consumers":[{"api_base":base,"model":"fixture-model"}]}]})).unwrap();
    if !with_residents {
        resources.residents.clear();
    }
    if priority {
        resources.settings.scheduling = Some(cogfabric::resource_scheduler::PriorityScheduling {
            role_priorities: std::collections::BTreeMap::from([
                ("solver".into(), 15),
                ("planner".into(), 2),
            ]),
            max_bypasses: 2,
        });
    }
    let cfg = Config {
        home: root,
        backend: "openai-local".into(),
        model: "fixture-model".into(),
        api_base: base.into(),
        interpret_goals: false,
        inference_process: managed,
        max_calls: 4,
        resource_admission: Some(resources),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend = Arc::new(Script {
        cfg: cfg.clone(),
        answers: Mutex::new(
            if mode == "fast" {
                vec!["[1,2,3]"]
            } else if mode == "graph" {
                vec![r#"{"steps":[{"id":"a","objective":"Inspect input","depends_on":[]},{"id":"b","objective":"Combine advice","depends_on":["a"]}]}"#, "original A advice", "original B advice", "[1,2,3]"]
            } else {
                vec!["invalid plan", "[1,2,3]"]
            }
            .into(),
        ),
    });
    let engine = Engine::with_backend(cfg.clone(), store.clone(), backend);
    let original_pool = engine
        .configured_resource_pool()
        .unwrap()
        .unwrap()
        .snapshot()
        .unwrap()
        .pool_id;
    let original = engine
        .run_profile(
            r#"sort:{"values":[3,1,2]}"#,
            if mode == "graph" { "deep" } else { mode },
            ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(original.status, "completed");
    let task = original.task_id;
    // Keep the real audited prefix before the first whole-goal solver dispatch.
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let boundary: u64 = db
        .query_row(
            if mode=="graph" { "SELECT min(id)-1 FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='b' AND json_extract(payload,'$.node.state')='running'" } else { "SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary'" },
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
    drop(db);
    drop(engine);
    (cfg, store, task, original_pool)
}
async fn http_answers(listener: tokio::net::TcpListener, answers: Vec<(u64, &'static str)>) {
    for (tokens, answer) in answers {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0u8; 2048];
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 64000);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    let value: Value =
                        serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                    assert_eq!(value["model"], "fixture-model");
                    assert_eq!(value["max_tokens"], tokens);
                    break;
                }
            }
        }
        let body=json!({"model":"fixture-model","choices":[{"message":{"content":answer}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}).to_string();
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(reply.as_bytes()).await.unwrap();
    }
}
async fn http_once(listener: tokio::net::TcpListener) {
    http_answers(listener, vec![(384, "[1,2,3]")]).await;
}

async fn cli(cfg: &Config, task: &str, action: &str, approve: bool) -> std::process::Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"));
    command
        .arg("--home")
        .arg(&cfg.home)
        .arg("--json")
        .arg(action)
        .arg(task)
        .env_clear();
    if approve {
        command.arg("--human-approve");
    }
    command.output().await.unwrap()
}
#[tokio::test]
async fn configured_pool_preview_and_resume_restore_original_admission_without_replay() {
    let _launch = PROCESS_LAUNCH.read().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (cfg, store, task, original_pool) = fixture(&base, false).await;
    let before = store.task(&task).unwrap();
    let preview = cogfabric::recovery::continuation::preview(&store, &task).unwrap();
    assert_eq!(preview["resource_history"]["completed_calls"], 1);
    assert_eq!(preview["consumed"]["calls"], 1);
    assert_eq!(store.task(&task).unwrap(), before);
    let provider = tokio::spawn(http_once(listener));
    let report = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    provider.await.unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["new_calls"], 1);
    assert_eq!(report["charged_resources"]["calls"], 2);
    assert_eq!(report["charged_resources"]["reserved_output_tokens"], 640);
    assert_ne!(report["restored_resource_pool"]["pool_id"], original_pool);
    assert_eq!(
        report["restored_resource_pool"]["resident_allocated"],
        json!({"compute_slots":0,"host_memory_bytes":30,"device_memory_bytes":30})
    );
    let trace = store.task(&task).unwrap();
    let events = trace["events"].as_array().unwrap();
    for kind in ["execution_settings", "resource_completed"] {
        assert_eq!(
            events.iter().filter(|e| e["kind"] == kind).count(),
            if kind == "execution_settings" { 1 } else { 2 }
        );
    }
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
#[tokio::test]
async fn actual_cli_restores_original_pool_for_direct_and_managed_inference() {
    let _launch = PROCESS_LAUNCH.write().await;
    for managed in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let (cfg, store, task, _) = fixture(&base, managed).await;
        let mut changed = cfg.clone();
        changed
            .resource_admission
            .as_mut()
            .unwrap()
            .settings
            .capacity
            .compute_slots = 8;
        changed
            .resource_admission
            .as_mut()
            .unwrap()
            .allocation
            .host_memory_bytes = 70;
        changed.save().unwrap();
        let preview = cli(&cfg, &task, "resume-preview", false).await;
        assert!(
            preview.status.success(),
            "{}",
            String::from_utf8_lossy(&preview.stderr)
        );
        let provider = tokio::spawn(http_once(listener));
        let output = cli(&cfg, &task, "resume-task", true).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        provider.await.unwrap();
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["status"], "completed");
        assert_eq!(report["new_calls"], 1);
        assert_eq!(report["charged_resources"]["calls"], 2);
        let trace = store.task(&task).unwrap();
        let events = trace["events"].as_array().unwrap();
        let last = events
            .iter()
            .rev()
            .find(|e| e["kind"] == "resource_intent")
            .unwrap();
        assert_eq!(last["payload"]["settings"]["capacity"]["compute_slots"], 1);
        assert_eq!(last["payload"]["allocation"]["host_memory_bytes"], 20);
        assert_eq!(
            events
                .iter()
                .filter(|e| e["kind"] == "worker_process_launch")
                .count(),
            usize::from(managed)
        );
        drop(store);
        std::fs::remove_dir_all(cfg.home).unwrap();
    }
}
#[tokio::test]
async fn corrupt_missing_or_unresolved_resource_evidence_denies_before_state_or_http() {
    let _launch = PROCESS_LAUNCH.read().await;
    for mutation in 0..13 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let (cfg, store, task, _) = fixture(&base, false).await;
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        match mutation {
            0 => {
                db.execute(
                    "DELETE FROM events WHERE task_id=?1 AND kind='resource_completed'",
                    [&task],
                )
                .unwrap();
            }
            1 => {
                db.execute("UPDATE events SET payload=json_set(payload,'$.allocation.host_memory_bytes',21) WHERE task_id=?1 AND kind='resource_intent'",[&task]).unwrap();
            }
            2 => {
                db.execute("UPDATE events SET payload=json_set(payload,'$.settings.capacity.compute_slots',2) WHERE task_id=?1 AND kind='resource_intent'",[&task]).unwrap();
            }
            3 => {
                db.execute("UPDATE events SET payload=json_set(payload,'$.reservation_id',?2) WHERE task_id=?1 AND kind='resource_completed'",rusqlite::params![task,uuid::Uuid::new_v4().to_string()]).unwrap();
            }
            4 => {
                db.execute("UPDATE events SET source='forged' WHERE task_id=?1 AND kind='resource_admitted'",[&task]).unwrap();
            }
            5 => {
                db.execute(
                    "DELETE FROM events WHERE task_id=?1 AND kind='resource_target_intent'",
                    [&task],
                )
                .unwrap();
            }
            6 => {
                db.execute("UPDATE events SET payload=json_set(payload,'$.resident_catalog_sha256','changed') WHERE task_id=?1 AND kind='resource_intent'",[&task]).unwrap();
            }
            7 => {
                db.execute("UPDATE events SET payload=json_set(payload,'$.allowed',json('false')) WHERE task_id=?1 AND kind='resource_target'",[&task]).unwrap();
            }
            8 => {
                db.execute("UPDATE events SET payload=json_set(payload,'$.coverage.target.target.model','unlisted') WHERE task_id=?1 AND kind='resource_target'",[&task]).unwrap();
            }
            9 => {
                db.execute(
                    "DELETE FROM events WHERE task_id=?1 AND kind='resource_intent'",
                    [&task],
                )
                .unwrap();
            }
            10 => {
                db.execute("UPDATE events SET payload=json_remove(payload,'$.configuration.resource_admission') WHERE task_id=?1 AND kind='execution_settings'",[&task]).unwrap();
            }
            11 => {
                let other = store
                    .create_task("Unresolved unrelated fixture", "fast")
                    .unwrap();
                store.event(&other,"resource_dispatch","jev-plus",json!({"format_version":1,"pool_id":uuid::Uuid::new_v4().to_string(),"reservation_id":uuid::Uuid::new_v4().to_string(),"provider_stop_proven":false})).unwrap();
            }
            _ => {
                db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='resource_target'",[&task]).unwrap();
            }
        }
        drop(db);
        let before = store.task(&task).unwrap();
        assert!(
            cogfabric::recovery::continuation::preview(&store, &task).is_err(),
            "mutation {mutation}"
        );
        assert!(
            cogfabric::recovery::continuation::resume(
                store.clone(),
                &task,
                true,
                std::future::pending()
            )
            .await
            .is_err(),
            "mutation {mutation}"
        );
        assert_eq!(store.task(&task).unwrap(), before);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), listener.accept())
                .await
                .is_err()
        );
        drop(store);
        std::fs::remove_dir_all(cfg.home).unwrap();
    }
}
#[tokio::test]
async fn ready_cancellation_and_admission_audit_failure_preserve_original_resource_history() {
    let _launch = PROCESS_LAUNCH.read().await;
    for failure in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let (cfg, store, task, _) = fixture(&base, false).await;
        let before = store.task(&task).unwrap();
        if failure {
            let db = rusqlite::Connection::open(cfg.db()).unwrap();
            db.execute_batch("CREATE TRIGGER refuse_resume BEFORE INSERT ON events WHEN NEW.kind='continuation' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
            assert!(cogfabric::recovery::continuation::resume(
                store.clone(),
                &task,
                true,
                std::future::pending()
            )
            .await
            .is_err());
            assert_eq!(store.task(&task).unwrap(), before);
        } else {
            let report =
                cogfabric::recovery::continuation::resume(store.clone(), &task, true, async {})
                    .await
                    .unwrap();
            assert_eq!(report["new_calls"], 0);
            assert_eq!(report["charged_resources"]["calls"], 1);
            assert_eq!(report["status"], "interrupted");
            assert!(cogfabric::recovery::continuation::preview(&store, &task).is_ok());
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(10), listener.accept())
                .await
                .is_err()
        );
        drop(store);
        std::fs::remove_dir_all(cfg.home).unwrap();
    }
}

#[tokio::test]
async fn initial_solver_and_legacy_empty_catalog_checkpoints_keep_resource_configuration() {
    let _launch = PROCESS_LAUNCH.read().await;
    for (mode, with_residents, legacy) in [
        ("fast", true, false),
        ("deep", false, false),
        ("deep", false, true),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let (cfg, store, task, _) = fixture_mode(&base, false, mode, with_residents).await;
        if legacy {
            let db = rusqlite::Connection::open(cfg.db()).unwrap();
            db.execute("UPDATE events SET payload=json_remove(payload,'$.resident_allocated','$.resident_catalog_sha256','$.residency_attested') WHERE task_id=?1 AND kind='resource_intent'",[&task]).unwrap();
        }
        let preview = cogfabric::recovery::continuation::preview(&store, &task).unwrap();
        assert_eq!(
            preview["resource_history"]["completed_calls"],
            if mode == "fast" { 0 } else { 1 }
        );
        let provider = tokio::spawn(http_once(listener));
        let report = cogfabric::recovery::continuation::resume(
            store.clone(),
            &task,
            true,
            std::future::pending(),
        )
        .await
        .unwrap();
        provider.await.unwrap();
        assert_eq!(report["status"], "completed");
        assert_eq!(report["new_calls"], 1);
        assert_eq!(
            report["restored_resource_pool"]["resident_allocated"]["device_memory_bytes"],
            if with_residents { 30 } else { 0 }
        );
        drop(store);
        std::fs::remove_dir_all(cfg.home).unwrap();
    }
}
#[tokio::test]
async fn live_resource_owner_and_missing_approval_block_restore_without_mutation() {
    let _launch = PROCESS_LAUNCH.read().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (cfg, store, task, _) = fixture(&base, false).await;
    let before = store.task(&task).unwrap();
    assert!(cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        false,
        std::future::pending()
    )
    .await
    .is_err());
    let other = Arc::new(Store::open(&cfg.db()).unwrap());
    let owner = Engine::new(cfg.clone(), other);
    let held = owner.configured_resource_pool().unwrap().unwrap();
    assert!(cogfabric::recovery::continuation::preview(&store, &task).is_err());
    assert!(cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending()
    )
    .await
    .is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), listener.accept())
            .await
            .is_err()
    );
    drop(held);
    drop(owner);
    assert!(cogfabric::recovery::continuation::preview(&store, &task).is_ok());
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}

#[tokio::test]
async fn partial_advisory_graph_restores_pool_and_only_runs_pending_node_and_final_solver() {
    let _launch = PROCESS_LAUNCH.read().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (cfg, store, task, _) = fixture_mode(&base, false, "graph", true).await;
    let preview = cogfabric::recovery::continuation::preview(&store, &task).unwrap();
    assert_eq!(preview["resource_history"]["completed_calls"], 2);
    let provider = tokio::spawn(http_answers(
        listener,
        vec![(192, "resumed B advice"), (384, "[1,2,3]")],
    ));
    let report = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    provider.await.unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["new_calls"], 2);
    assert_eq!(report["charged_resources"]["calls"], 4);
    assert_eq!(report["charged_resources"]["reserved_output_tokens"], 1024);
    let trace = store.task(&task).unwrap();
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "resource_completed")
            .count(),
        4
    );
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}

#[tokio::test]
async fn priority_policy_restores_from_original_history_and_rejects_changed_queue_metadata() {
    let _launch = PROCESS_LAUNCH.read().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (cfg, store, task, _) = fixture_mode_policy(&base, false, "deep", true, true).await;
    assert!(cogfabric::recovery::continuation::preview(&store, &task).is_ok());
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    for kind in ["resource_intent", "resource_admitted"] {
        let (id, raw): (u64, String) = db
            .query_row(
                "SELECT id,payload FROM events WHERE task_id=?1 AND kind=?2 ORDER BY id LIMIT 1",
                rusqlite::params![task, kind],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        for variant in 0..3 {
            let mut changed: Value = serde_json::from_str(&raw).unwrap();
            match variant {
                0 => changed["queue_priority"] = json!(15),
                1 => {
                    changed.as_object_mut().unwrap().remove("queue_priority");
                }
                _ => changed["queue_bypasses"] = json!(3),
            };
            db.execute(
                "UPDATE events SET payload=?1 WHERE id=?2",
                rusqlite::params![changed.to_string(), id],
            )
            .unwrap();
            assert!(
                cogfabric::recovery::continuation::preview(&store, &task).is_err(),
                "{kind} mutation {variant}"
            );
            db.execute(
                "UPDATE events SET payload=?1 WHERE id=?2",
                rusqlite::params![raw, id],
            )
            .unwrap();
        }
    }
    drop(db);
    let provider = tokio::spawn(http_once(listener));
    let report = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["new_calls"], 1);
    provider.await.unwrap();
    let trace = store.task(&task).unwrap();
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "resource_admitted"
            && e["payload"]["role"] == "solver"
            && e["payload"]["queue_priority"] == 15
            && e["payload"]["queue_bypasses"] == 0));
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
