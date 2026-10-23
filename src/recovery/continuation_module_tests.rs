use super::*;
use crate::fabric::modules::{
    Broker, Capability, Descriptor, Grant, Limits, Module, ModuleFuture, Request, Response,
};
use crate::security::TaskPolicy;
use std::sync::atomic::{AtomicUsize, Ordering};

struct SortingModule {
    version: &'static str,
    calls: Arc<AtomicUsize>,
    store: Arc<Store>,
}
impl Module for SortingModule {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "fixture.sorting".into(),
            version: self.version.into(),
            capabilities: [Capability::SortingVerify].into(),
        }
    }
    fn execute<'a>(&'a self, _: &'a str, _: Instant, request: Request) -> ModuleFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            // Same-image workers must obtain this shared execution lease too.
            // A retained exclusive admission lease would block their dispatch.
            let _execution_lease = self.store.execution_lease()?;
            assert!(matches!(request, Request::Sort { .. }));
            Ok(Response::SortDiagnostic {
                claimed_valid: true,
            })
        })
    }
}
fn registry(store: Arc<Store>) -> (Arc<Broker>, Arc<AtomicUsize>) {
    let broker = Arc::new(Broker::new(store.clone()));
    let calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(SortingModule {
                version: "1.0.0",
                calls: calls.clone(),
                store: store.clone(),
            }),
            Limits::default(),
        )
        .unwrap();
    (broker, calls)
}
fn snapshot(store: &Store, task: &str, broker: &Broker) {
    store.event(task, "typed_module_snapshot", "fabric", json!({
        "format_version":1,"broker_id":broker.identity_id().to_string(),
        "modules":[{"id":"fixture.sorting","version":"1.0.0","capabilities":["sorting.verify"]}],
        "permissions_granted":false
    })).unwrap();
}
async fn completed_dispatch(store: &Store, task: &str, broker: &Broker) {
    store.event(task, "tool_reservation", "security-kernel", json!({"format_version":1,"invocation":1,"actor":"jev-plus","capability":"sorting.verify"})).unwrap();
    let pin = broker.pin(task, "fixture.sorting", "1.0.0").unwrap();
    let policy = TaskPolicy::reasoning(task).unwrap();
    let grant = Grant::narrow(task, "jev-plus", [Capability::SortingVerify], 1, &policy).unwrap();
    broker
        .dispatch(
            &pin,
            &grant,
            &policy,
            "jev-plus",
            Request::Sort {
                values: vec![3, 1, 2],
                candidate: vec![1, 2, 3],
                descending: false,
            },
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn exact_registry_restores_retained_versions_without_dispatch_or_activation() {
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store.create_task("Restore old module", "standard").unwrap();
    let (broker, calls) = registry(store.clone());
    snapshot(&store, &task, &broker);
    completed_dispatch(&store, &task, &broker).await;
    let new_calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(SortingModule {
                version: "2.0.0",
                calls: new_calls.clone(),
                store: store.clone(),
            }),
            Limits::default(),
        )
        .unwrap();
    let before = store.task(&task).unwrap();
    let restored = module_history::restore(&store.lock().unwrap(), &task, Some(broker.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.pins[&Capability::SortingVerify]
            .descriptor()
            .version,
        "1.0.0"
    );
    assert_eq!(restored.evidence()["completed_dispatches"], 1);
    assert_eq!(store.task(&task).unwrap(), before);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(new_calls.load(Ordering::SeqCst), 0);
    assert_eq!(broker.discover().unwrap()[0].version, "2.0.0");
}

#[tokio::test]
async fn unknown_duplicate_or_forged_dispatch_evidence_refuses_without_writes() {
    for scenario in 0..10 {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store
            .create_task("Reject unknown module work", "standard")
            .unwrap();
        let (broker, calls) = registry(store.clone());
        snapshot(&store, &task, &broker);
        completed_dispatch(&store, &task, &broker).await;
        {
            let db = store.lock().unwrap();
            match scenario {
                0 => {
                    db.execute(
                        "DELETE FROM events WHERE task_id=?1 AND kind='module_dispatch_outcome'",
                        [&task],
                    )
                    .unwrap();
                }
                1 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.status','deadline_unknown') WHERE task_id=?1 AND kind='module_dispatch_outcome'", [&task]).unwrap();
                }
                2 => {
                    db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='module_dispatch_outcome'", [&task]).unwrap();
                }
                3 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.request_sha256',?2) WHERE task_id=?1 AND kind='module_dispatch_outcome'", rusqlite::params![task,"0".repeat(64)]).unwrap();
                }
                4 => {
                    db.execute(
                        "DELETE FROM events WHERE task_id=?1 AND kind='tool_reservation'",
                        [&task],
                    )
                    .unwrap();
                }
                5 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.module_version','2.0.0') WHERE task_id=?1 AND kind='module_dispatch_intent'", [&task]).unwrap();
                }
                6 => {
                    db.execute("UPDATE events SET source='solver' WHERE task_id=?1 AND kind='module_dispatch_outcome'", [&task]).unwrap();
                }
                7 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.response_sha256','bad') WHERE task_id=?1 AND kind='module_dispatch_outcome'", [&task]).unwrap();
                }
                8 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.module_granted_permissions',1) WHERE task_id=?1 AND kind='module_dispatch_intent'", [&task]).unwrap();
                }
                9 => {
                    db.execute(
                        "DELETE FROM events WHERE task_id=?1 AND kind='typed_module_snapshot'",
                        [&task],
                    )
                    .unwrap();
                }
                _ => unreachable!(),
            }
        }
        let before = store.task(&task).unwrap();
        assert!(
            module_history::restore(&store.lock().unwrap(), &task, Some(broker)).is_err(),
            "scenario {scenario}"
        );
        assert_eq!(store.task(&task).unwrap(), before);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn another_registry_store_or_retired_version_is_not_a_restoration() {
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store
        .create_task("Reject registry replacement", "standard")
        .unwrap();
    let (original, _) = registry(store.clone());
    snapshot(&store, &task, &original);
    let (replacement, calls) = registry(store.clone());
    assert!(module_history::restore(&store.lock().unwrap(), &task, Some(replacement)).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let other = Store::in_memory().unwrap();
    assert!(original.require_store(&other).is_err());
    original.deactivate("fixture.sorting").unwrap();
    original.retire("fixture.sorting", "1.0.0").unwrap();
    assert!(module_history::restore(&store.lock().unwrap(), &task, Some(original)).is_err());
}

#[tokio::test]
async fn restored_registry_preview_and_resume_keep_old_versions_and_original_charges() {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    let root = std::env::temp_dir().join(format!("fabric-module-resume-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(Store::open(&root.join("state.db")).unwrap());
    let (store, task) = super::tests::fixture_in_with_policy(store, false, None).await;
    let (broker, old_calls) = registry(store.clone());
    snapshot(&store, &task, &broker);
    store
        .lock()
        .unwrap()
        .execute("UPDATE tasks SET status='running' WHERE id=?1", [&task])
        .unwrap();
    completed_dispatch(&store, &task, &broker).await;
    store.finish_task(&task, "interrupted", "").unwrap();
    let new_calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(SortingModule {
                version: "2.0.0",
                calls: new_calls.clone(),
                store: store.clone(),
            }),
            Limits::default(),
        )
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    store.lock().unwrap().execute(
        "UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",
        rusqlite::params![task,endpoint],
    ).unwrap();
    let runtime = RuntimeRestoration {
        modules: Some(broker.clone()),
        ..Default::default()
    };
    let before = store.task(&task).unwrap();
    assert!(preview(&store, &task).is_err());
    let report = preview_with_runtime(&store, &task, runtime.clone()).unwrap();
    assert_eq!(report["resume_authorized"], false);
    assert_eq!(
        report["restored_typed_modules"]["original_versions_preserved"],
        true
    );
    assert_eq!(report["consumed"]["calls"], 2);
    assert_eq!(report["consumed"]["tool_calls"], 1);
    assert_eq!(report["restored_typed_modules"]["completed_dispatches"], 1);
    assert_eq!(store.task(&task).unwrap(), before);
    assert!(resume_with_runtime(
        store.clone(),
        &task,
        false,
        runtime.clone(),
        std::future::pending()
    )
    .await
    .is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    let server = tokio::spawn(async move {
        for answer in ["remaining advisory", "[1,2,3]"] {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut length = None;
            loop {
                let mut line = String::new();
                assert!(socket.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length: ") {
                    length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            let mut body = vec![0; length.unwrap()];
            socket.read_exact(&mut body).await.unwrap();
            let response = json!({"choices":[{"message":{"content":answer}}]}).to_string();
            socket.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).as_bytes()).await.unwrap();
        }
    });
    let report = tokio::time::timeout(
        Duration::from_secs(10),
        resume_with_runtime(store.clone(), &task, true, runtime, std::future::pending()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["new_calls"], 2);
    assert_eq!(report["charged_resources"]["calls"], 4);
    assert_eq!(report["charged_resources"]["tool_calls"], 2);
    assert_eq!(report["automatic_replay"], false);
    assert_eq!(old_calls.load(Ordering::SeqCst), 2);
    assert_eq!(new_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.task(&task).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "module_dispatch_outcome"
                && event["payload"]["status"] == "returned")
            .count(),
        2
    );
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(broker.discover().unwrap()[0].version, "2.0.0");
    drop(broker);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recorded_process_completion_requires_every_launch_owned_pid_and_result_link() {
    for scenario in 0..7 {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store
            .create_task("Validate process audit data only", "standard")
            .unwrap();
        let (broker, calls) = registry(store.clone());
        snapshot(&store, &task, &broker);
        let correlation = uuid::Uuid::new_v4().to_string();
        let request_hash = sha256(b"fixture request");
        let response_hash = sha256(b"fixture response");
        let identity = json!({"format_version":1,"correlation_id":correlation,
            "module_id":"fixture.sorting","module_version":"1.0.0","capability":"sorting.verify",
            "request_sha256":request_hash});
        store.event(&task,"tool_reservation","security-kernel",json!({"format_version":1,"invocation":1,"actor":"jev-plus","capability":"sorting.verify"})).unwrap();
        let mut intent = identity.clone();
        intent["request_bytes"] = json!(12);
        intent["deadline_ms"] = json!(1000);
        intent["module_granted_permissions"] = json!(false);
        store
            .event(&task, "module_dispatch_intent", "jev-plus", intent)
            .unwrap();
        let mut process_intent = identity.clone();
        process_intent["request_bytes"] = json!(12);
        process_intent["permissions_granted"] = json!(false);
        store
            .event(
                &task,
                "module_process_dispatch_intent",
                "jev-plus",
                process_intent,
            )
            .unwrap();
        let mut launch = identity.clone();
        launch["sandboxed"] = json!(false);
        launch["calls_granted"] = json!(1);
        store
            .event(&task, "module_process_launch", "security-kernel", launch)
            .unwrap();
        // Synthetic audit IDs: validation must never turn these into OS handles.
        if scenario != 1 {
            store
                .event(
                    &task,
                    "module_process_owned",
                    "fabric",
                    json!({"pid":1234,"sandboxed":false}),
                )
                .unwrap();
        }
        if scenario != 2 {
            store.event(&task,"module_process_stopped","fabric",json!({"pid":if scenario==3 {1235} else {1234},"forced":scenario==4,"exit_code":0,"reaped":true})).unwrap();
        }
        if scenario != 5 {
            let mut result = identity.clone();
            result["sandboxed"] = json!(false);
            result["status"] = json!("returned");
            result["response_sha256"] = json!(if scenario == 6 {
                sha256(b"another response")
            } else {
                response_hash.clone()
            });
            store
                .event(&task, "module_process_dispatch_outcome", "jev-plus", result)
                .unwrap();
        }
        let mut outcome = identity;
        outcome["status"] = json!("returned");
        outcome["provider_stop_proven"] = json!(false);
        outcome["response_sha256"] = json!(response_hash);
        outcome["response_bytes"] = json!(1);
        store
            .event(&task, "module_dispatch_outcome", "jev-plus", outcome)
            .unwrap();
        let before = store.task(&task).unwrap();
        let result = module_history::restore(&store.lock().unwrap(), &task, Some(broker));
        assert_eq!(result.is_ok(), scenario == 0, "scenario {scenario}");
        assert_eq!(store.task(&task).unwrap(), before);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
