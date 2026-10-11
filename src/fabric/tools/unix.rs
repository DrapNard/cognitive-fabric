//! Task-granted arithmetic commands over mutually authenticated local sockets.
pub mod process;
use super::{decode, encode, ArithmeticBackend, Descriptor, Identity, Packet};
use anyhow::{bail, Context, Result};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{watch, Mutex},
};

fn authenticate(stream: &UnixStream, uid: u32, pid: u32) -> Result<()> {
    let peer = stream.peer_cred()?;
    if pid == 0 || peer.uid() != uid || peer.pid().map(|p| p as u32) != Some(pid) {
        bail!("Arithmetic peer differs from host-approved process")
    }
    Ok(())
}
async fn read(stream: &mut UnixStream) -> Result<Option<Packet>> {
    let mut prefix = [0; 4];
    if stream.read(&mut prefix[..1]).await? == 0 {
        return Ok(None);
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        stream.read_exact(&mut prefix[1..]).await?;
        let length = u32::from_be_bytes(prefix) as usize;
        if !(8..=8192).contains(&length) {
            bail!("Arithmetic frame length exceeds bounds")
        }
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).await?;
        Ok(Some(decode(&bytes)?))
    })
    .await
    .context("Arithmetic partial frame deadline exhausted")?
}
async fn write(stream: &mut UnixStream, packet: &Packet) -> Result<()> {
    let bytes = encode(packet)?;
    tokio::time::timeout(Duration::from_secs(2), async {
        stream.write_u32(u32::try_from(bytes.len())?).await?;
        stream.write_all(&bytes).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("Arithmetic write deadline exhausted")?
}

/// Authority is supplied through trusted setup, never decoded from a request.
#[derive(Clone)]
pub struct Grant {
    task: String,
    descriptor: Descriptor,
    calls: u32,
    seconds: u64,
}
impl Grant {
    pub fn new(task: &str, descriptor: Descriptor, calls: u32, seconds: u64) -> Result<Self> {
        uuid::Uuid::parse_str(task)?;
        descriptor.validate()?;
        if !(1..=64).contains(&calls) || !(1..=300).contains(&seconds) {
            bail!("Arithmetic grant exceeds bounds")
        }
        Ok(Self {
            task: task.into(),
            descriptor,
            calls,
            seconds,
        })
    }
    fn authorize(&self, packet: &Packet) -> Result<()> {
        let i = &packet.identity;
        let now = crate::fabric::protocol::now_ms();
        if i.task != self.task
            || i.module != self.descriptor.id
            || i.version != self.descriptor.version
            || i.expires <= now
            || i.expires.saturating_sub(now) > 30_000
            || packet.expression.is_none()
        {
            bail!("Arithmetic request exceeds task, module or deadline grant")
        }
        crate::security::TaskPolicy::reasoning(&self.task)?.authorize(
            &self.task,
            &i.actor,
            "math.evaluate",
        )
    }
}

struct Connection {
    stream: Option<UnixStream>,
    calls: u32,
}
/// Cancelled or malformed exchanges permanently close the connection.
pub struct UnixArithmetic {
    grant: Grant,
    deadline: tokio::time::Instant,
    connection: Mutex<Connection>,
}
impl UnixArithmetic {
    pub fn connected(stream: UnixStream, grant: Grant, uid: u32, pid: u32) -> Result<Self> {
        authenticate(&stream, uid, pid)?;
        Ok(Self {
            deadline: tokio::time::Instant::now() + Duration::from_secs(grant.seconds),
            grant,
            connection: Mutex::new(Connection {
                stream: Some(stream),
                calls: 0,
            }),
        })
    }
}
impl ArithmeticBackend for UnixArithmetic {
    fn module_descriptor(&self) -> Option<&Descriptor> {
        Some(&self.grant.descriptor)
    }

    fn evaluate<'a>(
        &'a self,
        _: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async { bail!("Arithmetic process dispatch requires a task identity") })
    }
    fn evaluate_for<'a>(
        &'a self,
        task: &'a str,
        expression: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            if task != self.grant.task || expression.is_empty() || expression.len() > 120 {
                bail!("Arithmetic dispatch exceeds task or input grant")
            }
            let identity = Identity {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: task.into(),
                actor: "jev-plus".into(),
                module: self.grant.descriptor.id.clone(),
                version: self.grant.descriptor.version.clone(),
                expires: crate::fabric::protocol::now_ms().saturating_add(30_000),
            };
            let packet = Packet {
                identity: identity.clone(),
                expression: Some(expression.into()),
                answer: None,
                failed: false,
            };
            self.grant.authorize(&packet)?;
            tokio::time::timeout_at(
                self.deadline
                    .min(tokio::time::Instant::now() + Duration::from_secs(30)),
                async {
                    let mut slot = self.connection.lock().await;
                    if slot.calls >= self.grant.calls {
                        bail!("Arithmetic process call grant exhausted")
                    }
                    let mut stream = slot
                        .stream
                        .take()
                        .context("Arithmetic process connection closed")?;
                    slot.calls += 1;
                    write(&mut stream, &packet).await?;
                    let response = read(&mut stream)
                        .await?
                        .context("Arithmetic process disconnected")?;
                    if response.identity != identity
                        || response.expression.is_some()
                        || response.identity.expires <= crate::fabric::protocol::now_ms()
                    {
                        bail!("Arithmetic response identity, shape or deadline mismatch")
                    }
                    slot.stream = Some(stream);
                    if response.failed {
                        bail!("Arithmetic process rejected request")
                    }
                    response.answer.context("Arithmetic response has no answer")
                },
            )
            .await
            .context("Arithmetic process deadline exhausted")?
        })
    }
}

/// One task, cumulative calls and a fixed session deadline. The audit anchor and
/// backend are trusted host inputs; this worker provides no executable sandbox.
pub async fn serve(
    mut stream: UnixStream,
    grant: Grant,
    uid: u32,
    pid: u32,
    backend: Arc<dyn ArithmeticBackend>,
    store: Arc<crate::store::Store>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    authenticate(&stream, uid, pid)?;
    let exists: bool = store.lock()?.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
        [&grant.task],
        |row| row.get(0),
    )?;
    if !exists {
        bail!("Arithmetic audit task does not exist")
    }
    let _lease = store.execution_lease()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(grant.seconds);
    let session = uuid::Uuid::new_v4().to_string();
    let mut calls = 0;
    let mut seen = std::collections::BTreeSet::new();
    let record = |state: &str, calls: u32, correlation: Option<&str>| {
        store.event(
            &grant.task,
            "tool_worker",
            "security-kernel",
            serde_json::json!({
                "session_id":session,"state":state,"charged_calls":calls,
                "correlation_id":correlation,"module_id":grant.descriptor.id,
                "module_version":grant.descriptor.version,"capability":"math.evaluate"
            }),
        )
    };
    record("started", calls, None)?;
    let result = tokio::time::timeout_at(deadline, async {
        loop {
            if *shutdown.borrow() { return Ok(()); }
            let packet = tokio::select! {
                biased;
                _ = shutdown.changed() => return Ok(()),
                packet = read(&mut stream) => match packet? { Some(p) => p, None => return Ok(()) },
            };
            if let Err(error) = grant.authorize(&packet) {
                record("denied", calls, Some(&packet.identity.correlation))?;
                return Err(error);
            }
            if calls >= grant.calls || !seen.insert(packet.identity.correlation.clone()) {
                record("denied", calls, Some(&packet.identity.correlation))?;
                bail!("Arithmetic worker call grant exhausted")
            }
            calls += 1;
            record("authorized", calls, Some(&packet.identity.correlation))?;
            let remaining = packet.identity.expires.saturating_sub(crate::fabric::protocol::now_ms());
            if remaining == 0 { bail!("Arithmetic request expired during audit") }
            let expression = packet.expression.as_ref().context("Missing arithmetic request")?;
            let result = tokio::select! {
                biased;
                _ = shutdown.changed() => return Ok(()),
                byte = stream.read_u8() => {
                    if byte.is_ok() { bail!("Arithmetic pipelining is not allowed") }
                    return Ok(());
                },
                result = tokio::time::timeout(Duration::from_millis(remaining), backend.evaluate_for(&grant.task, expression)) => result.context("Arithmetic execution deadline exhausted")?,
            };
            let (answer, failed) = match result { Ok(answer) if answer.len() <= 256 => (Some(answer), false), _ => (None, true) };
            let response = Packet { identity: packet.identity, expression: None, answer, failed };
            write(&mut stream, &response).await?;
            record("responded", calls, Some(&response.identity.correlation))?;
        }
    }).await.context("Arithmetic worker session expired").and_then(|r| r);
    record(
        if result.is_ok() { "closed" } else { "failed" },
        calls,
        None,
    )?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(calls: u32, seconds: u64) -> (Arc<crate::store::Store>, Grant) {
        let store = Arc::new(crate::store::Store::in_memory().unwrap());
        let task = store.create_task("arithmetic worker test", "fast").unwrap();
        let grant = Grant::new(
            &task,
            Descriptor {
                id: "arithmetic.local".into(),
                version: "2.0.0".into(),
                capability: "math.evaluate".into(),
            },
            calls,
            seconds,
        )
        .unwrap();
        (store, grant)
    }
    fn uid(stream: &UnixStream) -> u32 {
        stream.peer_cred().unwrap().uid()
    }
    fn request(grant: &Grant) -> Packet {
        Packet {
            identity: Identity {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: grant.task.clone(),
                actor: "jev-plus".into(),
                module: grant.descriptor.id.clone(),
                version: grant.descriptor.version.clone(),
                expires: crate::fabric::protocol::now_ms() + 30_000,
            },
            expression: Some("6*7".into()),
            answer: None,
            failed: false,
        }
    }
    #[test]
    fn child_process_fixture() {
        let Some(socket) = std::env::var_os("COGFABRIC_TEST_TOOL_SOCKET") else {
            return;
        };
        let task = std::env::var("COGFABRIC_TEST_TOOL_TASK").unwrap();
        let parent: u32 = std::env::var("COGFABRIC_TEST_TOOL_PARENT")
            .unwrap()
            .parse()
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let stream = UnixStream::connect(socket).await.unwrap();
            let store = Arc::new(crate::store::Store::in_memory().unwrap());
            store
                .create_named_task(&task, "process tool fixture", "fast")
                .unwrap();
            let grant = Grant::new(
                &task,
                Descriptor {
                    id: "arithmetic.local".into(),
                    version: "2.0.0".into(),
                    capability: "math.evaluate".into(),
                },
                1,
                10,
            )
            .unwrap();
            let (_stop, receive) = watch::channel(false);
            serve(
                stream,
                grant,
                unsafe { libc::geteuid() },
                parent,
                Arc::new(super::super::LocalMath),
                store,
                receive,
            )
            .await
            .unwrap();
        });
    }
    #[tokio::test]
    async fn separate_process_serves_real_arithmetic_command() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let directory =
            std::env::temp_dir().join(format!("cogfabric-tool-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let socket = directory.join("tool.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (_, grant) = setup(1, 10);
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "fabric::tools::unix::tests::child_process_fixture",
            ])
            .env("COGFABRIC_TEST_TOOL_SOCKET", &socket)
            .env("COGFABRIC_TEST_TOOL_TASK", &grant.task)
            .env("COGFABRIC_TEST_TOOL_PARENT", std::process::id().to_string())
            .stdout(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = child.spawn().unwrap();
        let (stream, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let adapter = UnixArithmetic::connected(
            stream,
            grant.clone(),
            unsafe { libc::geteuid() },
            child.id().unwrap(),
        )
        .unwrap();
        assert_eq!(
            adapter.evaluate_for(&grant.task, "6*7").await.unwrap(),
            "42"
        );
        drop(adapter);
        assert!(tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success());
        drop(listener);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn broker_dispatch_reuses_authenticated_connection_and_charges_both_sides() {
        let (store, grant) = setup(2, 10);
        let (client, server) = UnixStream::pair().unwrap();
        let adapter = Arc::new(
            UnixArithmetic::connected(client, grant.clone(), uid(&server), std::process::id())
                .unwrap(),
        );
        let (stop, receive) = watch::channel(false);
        let worker_store = store.clone();
        let worker_grant = grant.clone();
        let worker = tokio::spawn(serve(
            server,
            worker_grant,
            unsafe { libc::geteuid() },
            std::process::id(),
            Arc::new(super::super::LocalMath),
            worker_store,
            receive,
        ));
        let broker = super::super::Broker::default();
        let mut wrong = grant.descriptor.clone();
        wrong.version = "3.0.0".into();
        assert!(broker.register(wrong, adapter.clone()).is_err());
        broker
            .register(grant.descriptor.clone(), adapter.clone())
            .unwrap();
        let handle = broker.pin().unwrap().unwrap();
        for _ in 0..2 {
            assert_eq!(
                handle
                    .evaluate(&grant.task, "jev-plus", "6*7")
                    .await
                    .unwrap(),
                "42"
            );
        }
        assert!(handle
            .evaluate(&grant.task, "jev-plus", "6*7")
            .await
            .is_err());
        assert_eq!(adapter.connection.lock().await.calls, 2);
        stop.send_replace(true);
        worker.await.unwrap().unwrap();
        let trace = store.task(&grant.task).unwrap();
        let records = trace["events"].as_array().unwrap();
        assert_eq!(
            records
                .iter()
                .filter(|e| e["kind"] == "tool_worker" && e["payload"]["state"] == "authorized")
                .count(),
            2
        );
        assert!(!trace.to_string().contains("6*7"));
    }
    #[tokio::test]
    async fn wrong_peers_and_task_grants_fail_closed() {
        let (_, grant) = setup(1, 10);
        for mismatch in 0..3 {
            let (client, _) = UnixStream::pair().unwrap();
            let expected_uid = uid(&client).wrapping_add(u32::from(mismatch == 0));
            let expected_pid = if mismatch == 1 {
                0
            } else {
                std::process::id().wrapping_add(1)
            };
            assert!(
                UnixArithmetic::connected(client, grant.clone(), expected_uid, expected_pid)
                    .is_err()
            );
        }
        let (client, _server) = UnixStream::pair().unwrap();
        let adapter = UnixArithmetic::connected(
            client,
            grant.clone(),
            unsafe { libc::geteuid() },
            std::process::id(),
        )
        .unwrap();
        assert!(adapter.evaluate("6*7").await.is_err());
        assert!(adapter
            .evaluate_for(&uuid::Uuid::new_v4().to_string(), "6*7")
            .await
            .is_err());
        assert_eq!(adapter.connection.lock().await.calls, 0);
        assert!(Grant::new(&grant.task, grant.descriptor.clone(), 0, 10).is_err());
        assert!(Grant::new(&grant.task, grant.descriptor.clone(), 1, 301).is_err());
    }
    #[tokio::test]
    async fn worker_denies_changed_tasks_and_cumulative_calls() {
        for wrong_task in [true, false] {
            let (store, grant) = setup(1, 10);
            let (mut client, server) = UnixStream::pair().unwrap();
            let (_stop, receive) = watch::channel(false);
            let worker = tokio::spawn(serve(
                server,
                grant.clone(),
                unsafe { libc::geteuid() },
                std::process::id(),
                Arc::new(super::super::LocalMath),
                store.clone(),
                receive,
            ));
            let mut packet = request(&grant);
            if wrong_task {
                packet.identity.task = uuid::Uuid::new_v4().to_string();
            }
            write(&mut client, &packet).await.unwrap();
            if !wrong_task {
                assert_eq!(
                    read(&mut client).await.unwrap().unwrap().answer.as_deref(),
                    Some("42")
                );
                packet.identity.correlation = uuid::Uuid::new_v4().to_string();
                write(&mut client, &packet).await.unwrap();
            }
            assert!(worker.await.unwrap().is_err());
            let trace = store.task(&grant.task).unwrap();
            assert!(trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "tool_worker" && e["payload"]["state"] == "denied"));
        }
    }
    #[tokio::test]
    async fn corrupt_or_uncorrelated_responses_permanently_close_adapter() {
        for malformed in [true, false] {
            let (_, grant) = setup(2, 10);
            let (client, mut server) = UnixStream::pair().unwrap();
            let adapter =
                UnixArithmetic::connected(client, grant.clone(), uid(&server), std::process::id())
                    .unwrap();
            let peer = tokio::spawn(async move {
                let mut packet = read(&mut server).await.unwrap().unwrap();
                if malformed {
                    server.write_u32(8193).await.unwrap();
                } else {
                    packet.identity.correlation = uuid::Uuid::new_v4().to_string();
                    packet.expression = None;
                    packet.answer = Some("42".into());
                    write(&mut server, &packet).await.unwrap();
                }
            });
            assert!(adapter.evaluate_for(&grant.task, "6*7").await.is_err());
            peer.await.unwrap();
            assert!(adapter.connection.lock().await.stream.is_none());
            assert!(adapter.evaluate_for(&grant.task, "6*7").await.is_err());
        }
    }
    #[tokio::test]
    async fn cancellation_discards_stream_and_worker_drops_pending_backend() {
        let (store, grant) = setup(2, 10);
        let (client, server) = UnixStream::pair().unwrap();
        let adapter = Arc::new(
            UnixArithmetic::connected(client, grant.clone(), uid(&server), std::process::id())
                .unwrap(),
        );
        let entered = Arc::new(tokio::sync::Notify::new());
        struct Pending(Arc<tokio::sync::Notify>);
        impl ArithmeticBackend for Pending {
            fn evaluate<'a>(
                &'a self,
                _: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
                Box::pin(async {
                    self.0.notify_one();
                    std::future::pending().await
                })
            }
        }
        let (_stop, receive) = watch::channel(false);
        let worker = tokio::spawn(serve(
            server,
            grant.clone(),
            unsafe { libc::geteuid() },
            std::process::id(),
            Arc::new(Pending(entered.clone())),
            store,
            receive,
        ));
        let caller_adapter = adapter.clone();
        let task = grant.task.clone();
        let caller = tokio::spawn(async move { caller_adapter.evaluate_for(&task, "6*7").await });
        entered.notified().await;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert!(adapter.connection.lock().await.stream.is_none());
        tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn replayed_correlation_is_denied_without_a_second_execution() {
        let (store, grant) = setup(2, 10);
        let (mut client, server) = UnixStream::pair().unwrap();
        let (_stop, receive) = watch::channel(false);
        let worker = tokio::spawn(serve(
            server,
            grant.clone(),
            unsafe { libc::geteuid() },
            std::process::id(),
            Arc::new(super::super::LocalMath),
            store.clone(),
            receive,
        ));
        let packet = request(&grant);
        write(&mut client, &packet).await.unwrap();
        assert_eq!(
            read(&mut client).await.unwrap().unwrap().answer.as_deref(),
            Some("42")
        );
        write(&mut client, &packet).await.unwrap();
        assert!(worker.await.unwrap().is_err());
        let trace = store.task(&grant.task).unwrap();
        assert_eq!(
            trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["payload"]["state"] == "authorized")
                .count(),
            1
        );
    }
    #[tokio::test]
    async fn audit_failure_prevents_backend_execution() {
        let (store, grant) = setup(1, 10);
        store.lock().unwrap().execute_batch("CREATE TRIGGER fail_tool_authorization BEFORE INSERT ON events WHEN NEW.kind = 'tool_worker' AND json_extract(NEW.payload, '$.state') = 'authorized' BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END;").unwrap();
        struct Count(Arc<std::sync::atomic::AtomicUsize>);
        impl ArithmeticBackend for Count {
            fn evaluate<'a>(
                &'a self,
                _: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
                Box::pin(async {
                    self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok("42".into())
                })
            }
        }
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (mut client, server) = UnixStream::pair().unwrap();
        let (_stop, receive) = watch::channel(false);
        let worker = tokio::spawn(serve(
            server,
            grant.clone(),
            unsafe { libc::geteuid() },
            std::process::id(),
            Arc::new(Count(count.clone())),
            store,
            receive,
        ));
        write(&mut client, &request(&grant)).await.unwrap();
        assert!(worker.await.unwrap().is_err());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
    #[tokio::test(start_paused = true)]
    async fn incomplete_frames_and_idle_sessions_expire() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        client.write_u8(0).await.unwrap();
        assert!(read(&mut server)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("deadline"));
        let (store, grant) = setup(1, 1);
        let (_client, server) = UnixStream::pair().unwrap();
        let (_stop, receive) = watch::channel(false);
        assert!(serve(
            server,
            grant,
            unsafe { libc::geteuid() },
            std::process::id(),
            Arc::new(super::super::LocalMath),
            store,
            receive
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("expired"));
    }
}
