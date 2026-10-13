use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
fn limits() -> Limits {
    Limits {
        output_tokens: 128,
        prompt_bytes: 64000,
        tool_proposals: 1,
        seconds: 3,
    }
}
fn request() -> Turn {
    serde_json::from_value(json!({"messages":[{"role":"user","content":"Read client workspace data"}],"tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object"}}}],"max_tokens":128})).unwrap()
}
struct Fixed(AtomicUsize);
impl ChatBackend for Fixed {
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(Completion {
                content: None,
                tool_calls: vec![super::super::ToolCall {
                    id: "read_1".into(),
                    kind: "function".into(),
                    function: super::super::FunctionCall {
                        name: "read".into(),
                        arguments: "{\"path\":\"fixture.txt\"}".into(),
                    },
                }],
                model: "fixture/v1".into(),
                finish_reason: "tool_calls".into(),
                input_tokens: None,
                output_tokens: Some(10),
            })
        })
    }
}
fn grant(task: &str, stream: &UnixStream) -> Grant {
    Grant::new(
        task,
        stream.peer_cred().unwrap().uid(),
        std::process::id(),
        limits(),
    )
    .unwrap()
}
#[tokio::test]
async fn native_connected_worker_preserves_tool_proposals_and_audits_one_use() {
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store.create_task("Native IPC", "fast").unwrap();
    let backend = Arc::new(Fixed(AtomicUsize::new(0)));
    let (host, worker) = UnixStream::pair().unwrap();
    let host_grant = grant(&task, &host);
    let authority = grant(&task, &worker);
    let adapter = UnixChat::connected(host, host_grant).unwrap();
    let serving = tokio::spawn(serve_connected(
        worker,
        authority,
        backend.clone(),
        store.clone(),
        std::future::pending(),
    ));
    let answer = adapter.complete_for(&task, &request(), 128).await.unwrap();
    assert!(answer.content.is_none());
    assert_eq!(answer.tool_calls[0].function.name, "read");
    assert!(adapter.complete_for(&task, &request(), 128).await.is_err());
    serving.await.unwrap().unwrap();
    assert_eq!(backend.0.load(Ordering::SeqCst), 1);
    let trace = store.task(&task).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert!(events
        .iter()
        .any(|e| e["payload"]["state"] == "authorized" && e["payload"]["charged_calls"] == 1));
    assert!(events
        .iter()
        .any(|e| e["payload"]["state"] == "session_stopped"));
    assert!(!trace.to_string().contains("fixture.txt"));
}
#[tokio::test]
async fn forged_tasks_peers_and_ceilings_fail_before_generation() {
    for variant in 0..4 {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Native denial", "fast").unwrap();
        let backend = Arc::new(Fixed(AtomicUsize::new(0)));
        let (mut host, worker) = UnixStream::pair().unwrap();
        let mut authority = grant(&task, &worker);
        if variant == 0 {
            authority.pid += 1;
            assert!(serve_connected(
                worker,
                authority,
                backend.clone(),
                store,
                std::future::pending()
            )
            .await
            .is_err());
            continue;
        }
        if variant == 1 {
            authority.limits.output_tokens = 127;
        }
        if variant == 2 {
            authority.limits.prompt_bytes = 1;
        }
        let serving = tokio::spawn(serve_connected(
            worker,
            authority,
            backend.clone(),
            store.clone(),
            std::future::pending(),
        ));
        let packet = Packet {
            identity: Identity {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: if variant == 3 {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    task.clone()
                },
                expires_at_ms: now_ms() + 1000,
            },
            payload: Payload::Request {
                turn: request(),
                allocation: 128,
            },
        };
        write(&mut host, &packet, Instant::now() + Duration::from_secs(2))
            .await
            .unwrap();
        assert!(serving.await.unwrap().is_err());
        assert_eq!(backend.0.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn audit_failure_and_pipelined_bytes_prevent_backend_construction() {
    for fail_audit in [true, false] {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Native no dispatch", "fast").unwrap();
        let backend = Arc::new(Fixed(AtomicUsize::new(0)));
        if fail_audit {
            store.lock().unwrap().execute_batch("CREATE TRIGGER deny_native_audit BEFORE INSERT ON events WHEN NEW.kind='worker.audit' AND json_extract(NEW.payload,'$.state')='authorized' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        }
        let (mut host, worker) = UnixStream::pair().unwrap();
        let authority = grant(&task, &worker);
        let packet = Packet {
            identity: Identity {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: task.clone(),
                expires_at_ms: now_ms() + 1000,
            },
            payload: Payload::Request {
                turn: request(),
                allocation: 128,
            },
        };
        let bytes = protocol::encode(&packet).unwrap();
        let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&bytes);
        if !fail_audit {
            frame.push(1);
        }
        host.write_all(&frame).await.unwrap();
        assert!(serve_connected(
            worker,
            authority,
            backend.clone(),
            store.clone(),
            std::future::pending()
        )
        .await
        .is_err());
        assert_eq!(backend.0.load(Ordering::SeqCst), 0);
        assert!(store.task(&task).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |e| e["payload"]["state"] == "session_failed" && e["payload"]["charged_calls"] == 1
            ));
    }
}
struct Pending {
    entered: tokio::sync::Notify,
    dropped: AtomicUsize,
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl ChatBackend for Pending {
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async move {
            let _active = Active(&self.dropped);
            self.entered.notify_one();
            std::future::pending().await
        })
    }
}
#[tokio::test]
async fn disconnect_and_shutdown_drop_pending_inference_and_keep_charges() {
    for disconnect in [true, false] {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Native cancel", "fast").unwrap();
        let backend = Arc::new(Pending {
            entered: tokio::sync::Notify::new(),
            dropped: AtomicUsize::new(0),
        });
        let (host, worker) = UnixStream::pair().unwrap();
        let host_grant = grant(&task, &host);
        let authority = grant(&task, &worker);
        let adapter = Arc::new(UnixChat::connected(host, host_grant).unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let serving = tokio::spawn(serve_connected(
            worker,
            authority,
            backend.clone(),
            store.clone(),
            async {
                let _ = stopped.await;
            },
        ));
        let client = adapter.clone();
        let caller_task = task.clone();
        let calling =
            tokio::spawn(async move { client.complete_for(&caller_task, &request(), 128).await });
        tokio::time::timeout(Duration::from_secs(1), backend.entered.notified())
            .await
            .unwrap();
        if disconnect {
            calling.abort();
            let _ = calling.await;
            assert!(serving.await.unwrap().is_err());
            drop(stop);
        } else {
            stop.send(()).unwrap();
            serving.await.unwrap().unwrap();
            assert!(calling.await.unwrap().is_err());
        }
        assert_eq!(backend.dropped.load(Ordering::SeqCst), 1);
        assert!(adapter.complete_for(&task, &request(), 128).await.is_err());
        assert!(store.task(&task).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["payload"]["charged_calls"] == 1
                && e["payload"]["reserved_output_tokens"] == 128));
    }
}
#[tokio::test]
async fn forged_response_identity_and_undeclared_tools_consume_connection() {
    for forged_task in [true, false] {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Native forged response", "fast").unwrap();
        let (host, mut worker) = UnixStream::pair().unwrap();
        let adapter = UnixChat::connected(host, grant(&task, &worker)).unwrap();
        let responder = tokio::spawn(async move {
            let packet = read(&mut worker, Instant::now() + Duration::from_secs(2))
                .await
                .unwrap();
            let mut identity = packet.identity;
            if forged_task {
                identity.task = uuid::Uuid::new_v4().to_string();
            }
            let fixed = Fixed(AtomicUsize::new(0));
            let mut c = fixed.complete(&request(), 128).await.unwrap();
            if !forged_task {
                c.tool_calls[0].function.name = "shell".into();
            }
            write(
                &mut worker,
                &Packet {
                    identity,
                    payload: Payload::Response(c),
                },
                Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap();
        });
        assert!(adapter.complete_for(&task, &request(), 128).await.is_err());
        assert!(adapter.complete_for(&task, &request(), 128).await.is_err());
        responder.await.unwrap();
    }
}

#[tokio::test]
async fn response_wait_uses_grant_deadline_but_partial_frames_stay_bounded() {
    let (mut host, mut worker) = UnixStream::pair().unwrap();
    let task = uuid::Uuid::new_v4().to_string();
    let sender = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(40)).await;
        let packet = Packet {
            identity: Identity {
                correlation: uuid::Uuid::new_v4().to_string(),
                task,
                expires_at_ms: 1,
            },
            payload: Payload::Error("fixture".into()),
        };
        write(
            &mut worker,
            &packet,
            Instant::now() + Duration::from_secs(1),
        )
        .await
        .unwrap();
    });
    assert!(read_with_frame_timeout(
        &mut host,
        Instant::now() + Duration::from_secs(1),
        Duration::from_millis(20)
    )
    .await
    .is_ok());
    sender.await.unwrap();
    let (mut host, mut worker) = UnixStream::pair().unwrap();
    worker.write_all(&[0]).await.unwrap();
    assert!(read_with_frame_timeout(
        &mut host,
        Instant::now() + Duration::from_secs(1),
        Duration::from_millis(20)
    )
    .await
    .is_err());
}
