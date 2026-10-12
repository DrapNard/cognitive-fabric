//! Authenticated task-scoped SQL process RPC using structured COGQ frames.
use super::{decode, execute, request, response, Decoded, SqlBackend};
use crate::fabric::tools::{unix::authenticate, Descriptor, Identity};
use anyhow::{bail, Context, Result};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{watch, Mutex},
};

async fn read(stream: &mut UnixStream) -> Result<Option<(Vec<u8>, Decoded)>> {
    let mut prefix = [0; 4];
    if stream.read(&mut prefix[..1]).await? == 0 {
        return Ok(None);
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        stream.read_exact(&mut prefix[1..]).await?;
        let length = u32::from_be_bytes(prefix) as usize;
        if !(8..=262144).contains(&length) {
            bail!("SQL process frame length exceeds bounds")
        }
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).await?;
        let decoded = decode(&bytes)?;
        Ok(Some((bytes, decoded)))
    })
    .await
    .context("SQL process partial frame deadline exhausted")?
}
async fn write(stream: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    if !(8..=262144).contains(&bytes.len()) {
        bail!("SQL process frame exceeds bounds")
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        stream.write_u32(u32::try_from(bytes.len())?).await?;
        stream.write_all(bytes).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("SQL process write deadline exhausted")?
}
/// Trusted setup only: requests cannot create task authority or resource grants.
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
        if descriptor.capability != "sql.query"
            || !(1..=64).contains(&calls)
            || !(1..=300).contains(&seconds)
        {
            bail!("Invalid SQL process grant")
        }
        Ok(Self {
            task: task.into(),
            descriptor,
            calls,
            seconds,
        })
    }
    fn authorize(&self, packet: &Decoded) -> Result<()> {
        let i = &packet.identity;
        let now = crate::fabric::protocol::now_ms();
        if i.task != self.task
            || i.module != self.descriptor.id
            || i.version != self.descriptor.version
            || i.expires <= now
            || i.expires.saturating_sub(now) > 30000
            || packet.request.is_none()
        {
            bail!("SQL process request exceeds task, module or deadline grant")
        }
        crate::security::TaskPolicy::reasoning(&self.task)?.authorize(
            &self.task,
            &i.actor,
            "sql.query",
        )
    }
}
struct Connection {
    stream: Option<UnixStream>,
    calls: u32,
}
/// Cancelled or malformed exchanges permanently discard their connection.
pub struct UnixSql {
    grant: Grant,
    deadline: tokio::time::Instant,
    connection: Mutex<Connection>,
}
impl UnixSql {
    pub(crate) async fn close(&self) {
        self.connection.lock().await.stream.take();
    }
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
impl SqlBackend for UnixSql {
    fn availability(&self) -> crate::fabric::Availability {
        match self.connection.try_lock() {
            Ok(slot) if slot.stream.is_none() => crate::fabric::Availability::Closed,
            Ok(_) if tokio::time::Instant::now() >= self.deadline => {
                crate::fabric::Availability::Expired
            }
            Ok(slot) if slot.calls >= self.grant.calls => crate::fabric::Availability::Exhausted,
            Ok(slot) if slot.stream.is_some() => crate::fabric::Availability::Connected,
            Ok(_) => crate::fabric::Availability::Closed,
            Err(_) if tokio::time::Instant::now() >= self.deadline => {
                crate::fabric::Availability::Expired
            }
            Err(_) => crate::fabric::Availability::InUse,
        }
    }
    fn module_descriptor(&self) -> Option<&Descriptor> {
        Some(&self.grant.descriptor)
    }
    fn query<'a>(
        &'a self,
        task: &'a str,
        input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            if task != self.grant.task {
                bail!("SQL process belongs to another task")
            }
            let identity = Identity {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: task.into(),
                actor: "jev-plus".into(),
                module: self.grant.descriptor.id.clone(),
                version: self.grant.descriptor.version.clone(),
                expires: crate::fabric::protocol::now_ms().saturating_add(30000),
            };
            let bytes = request(&identity, input)?;
            self.grant.authorize(&decode(&bytes)?)?;
            tokio::time::timeout_at(
                self.deadline
                    .min(tokio::time::Instant::now() + Duration::from_secs(30)),
                async {
                    let mut slot = self.connection.lock().await;
                    if slot.calls >= self.grant.calls {
                        bail!("SQL process call grant exhausted")
                    }
                    let mut stream = slot
                        .stream
                        .take()
                        .context("SQL process connection closed")?;
                    slot.calls += 1;
                    write(&mut stream, &bytes).await?;
                    let (bytes, decoded) = read(&mut stream)
                        .await?
                        .context("SQL process disconnected")?;
                    if decoded.identity != identity
                        || decoded.request.is_some()
                        || identity.expires <= crate::fabric::protocol::now_ms()
                    {
                        bail!("SQL process response identity, shape or deadline mismatch")
                    }
                    let result = response(&bytes, &identity);
                    slot.stream = Some(stream);
                    result
                },
            )
            .await
            .context("SQL process deadline exhausted")?
        })
    }
}

/// A trusted embedding host supplies the peer, grant, backend and audit database.
/// This confines RPC admission, not arbitrary Rust code or external side effects.
pub async fn serve(
    mut stream: UnixStream,
    grant: Grant,
    uid: u32,
    pid: u32,
    backend: Arc<dyn SqlBackend>,
    store: Arc<crate::store::Store>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    authenticate(&stream, uid, pid)?;
    let exists: bool = store.lock()?.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
        [&grant.task],
        |r| r.get(0),
    )?;
    if !exists {
        bail!("SQL process audit task does not exist")
    }
    let _lease = store.execution_lease()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(grant.seconds);
    let session = uuid::Uuid::new_v4().to_string();
    let mut calls = 0;
    let mut seen = std::collections::BTreeSet::new();
    let record = |state: &str, calls: u32, correlation: Option<&str>| {
        store.event(&grant.task, "tool_worker", "security-kernel", serde_json::json!({"format_version":1,"session_id":session,"state":state,"charged_calls":calls,"correlation_id":correlation,"module_id":grant.descriptor.id,"module_version":grant.descriptor.version,"capability":"sql.query"}))
    };
    record("started", calls, None)?;
    let result = tokio::time::timeout_at(deadline, async {
        loop {
            if *shutdown.borrow() { return Ok(()); }
            let (bytes, packet) = tokio::select! {
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
                bail!("SQL process call/replay grant exhausted")
            }
            calls += 1;
            record("authorized", calls, Some(&packet.identity.correlation))?;
            let remaining = packet.identity.expires.saturating_sub(crate::fabric::protocol::now_ms());
            if remaining == 0 { bail!("SQL process request expired during audit") }
            let bytes = tokio::select! {
                biased;
                _ = shutdown.changed() => return Ok(()),
                byte = stream.read_u8() => { if byte.is_ok() { bail!("SQL process pipelining denied") } return Ok(()); },
                result = tokio::time::timeout(Duration::from_millis(remaining), execute(&bytes, &packet.identity, &grant.descriptor, backend.as_ref())) => result.context("SQL process execution deadline exhausted")??,
            };
            write(&mut stream, &bytes).await?;
            record("responded", calls, Some(&packet.identity.correlation))?;
        }
    }).await.context("SQL process session expired").and_then(|r| r);
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
    const INPUT: &str = r#"{"tables":[],"query":"SELECT 42 AS answer"}"#;
    fn setup(calls: u32, seconds: u64) -> (Arc<crate::store::Store>, Grant) {
        let store = Arc::new(crate::store::Store::in_memory().unwrap());
        let task = store.create_task("SQL process test", "fast").unwrap();
        let grant = Grant::new(
            &task,
            Descriptor {
                id: "sql.local".into(),
                version: "2.0.0".into(),
                capability: "sql.query".into(),
            },
            calls,
            seconds,
        )
        .unwrap();
        (store, grant)
    }
    fn uid() -> u32 {
        unsafe { libc::geteuid() }
    }
    fn identity(grant: &Grant) -> Identity {
        Identity {
            correlation: uuid::Uuid::new_v4().to_string(),
            task: grant.task.clone(),
            actor: "jev-plus".into(),
            module: grant.descriptor.id.clone(),
            version: grant.descriptor.version.clone(),
            expires: crate::fabric::protocol::now_ms() + 30000,
        }
    }
    fn launch(
        server: UnixStream,
        grant: Grant,
        store: Arc<crate::store::Store>,
        stop: watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<Result<()>> {
        tokio::spawn(serve(
            server,
            grant,
            uid(),
            std::process::id(),
            Arc::new(super::super::LocalSql),
            store,
            stop,
        ))
    }
    #[tokio::test]
    async fn registered_socket_backend_routes_sql_and_enforces_both_call_grants() {
        let (store, grant) = setup(2, 10);
        let (client, server) = UnixStream::pair().unwrap();
        let adapter =
            Arc::new(UnixSql::connected(client, grant.clone(), uid(), std::process::id()).unwrap());
        let (stop, receive) = watch::channel(false);
        let worker = launch(server, grant.clone(), store.clone(), receive);
        let broker = crate::fabric::tools::Broker::default();
        let mut wrong = grant.descriptor.clone();
        wrong.version = "3.0.0".into();
        assert!(broker.register_sql(wrong, adapter.clone()).is_err());
        broker
            .register_sql(grant.descriptor.clone(), adapter.clone())
            .unwrap();
        let handle = broker.pin_capability("sql.query").unwrap().unwrap();
        for _ in 0..2 {
            let result = handle
                .query_sql(&grant.task, "jev-plus", INPUT)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&result).unwrap()["rows"][0][0],
                42
            );
        }
        assert!(handle
            .query_sql(&grant.task, "jev-plus", INPUT)
            .await
            .is_err());
        assert_eq!(adapter.connection.lock().await.calls, 2);
        assert_eq!(
            adapter.availability(),
            crate::fabric::Availability::Exhausted
        );
        assert!(broker.pin_capability("sql.query").unwrap().is_none());
        assert!(broker
            .activate(&grant.descriptor.id, &grant.descriptor.version)
            .is_err());
        stop.send_replace(true);
        worker.await.unwrap().unwrap();
        let trace = store.task(&grant.task).unwrap();
        assert_eq!(
            trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "tool_worker" && e["payload"]["state"] == "authorized")
                .count(),
            2
        );
        assert!(!trace.to_string().contains("SELECT 42"));
    }
    #[tokio::test]
    async fn peer_task_and_cross_capability_grants_are_denied() {
        let (_, grant) = setup(1, 10);
        assert!(crate::fabric::tools::unix::Grant::new(
            &grant.task,
            grant.descriptor.clone(),
            1,
            10
        )
        .is_err());
        let mut arithmetic = grant.descriptor.clone();
        arithmetic.capability = "math.evaluate".into();
        assert!(Grant::new(&grant.task, arithmetic, 1, 10).is_err());
        for (expected_uid, expected_pid) in [
            (uid().wrapping_add(1), std::process::id()),
            (uid(), 0),
            (uid(), std::process::id().wrapping_add(1)),
        ] {
            let (client, _server) = UnixStream::pair().unwrap();
            assert!(UnixSql::connected(client, grant.clone(), expected_uid, expected_pid).is_err());
        }
        let (client, _server) = UnixStream::pair().unwrap();
        let adapter = UnixSql::connected(client, grant, uid(), std::process::id()).unwrap();
        assert!(adapter
            .query(&uuid::Uuid::new_v4().to_string(), INPUT)
            .await
            .is_err());
        assert_eq!(adapter.connection.lock().await.calls, 0);
    }
    #[tokio::test]
    async fn wrong_task_replay_and_excess_calls_fail_before_another_execution() {
        for case in 0..3 {
            let (store, grant) = setup(if case == 1 { 2 } else { 1 }, 10);
            let (mut client, server) = UnixStream::pair().unwrap();
            let (_stop, receive) = watch::channel(false);
            let worker = launch(server, grant.clone(), store.clone(), receive);
            let mut i = identity(&grant);
            if case == 0 {
                i.task = uuid::Uuid::new_v4().to_string();
            }
            write(&mut client, &request(&i, INPUT).unwrap())
                .await
                .unwrap();
            if case != 0 {
                let (bytes, _) = read(&mut client).await.unwrap().unwrap();
                response(&bytes, &i).unwrap();
                if case == 2 {
                    i.correlation = uuid::Uuid::new_v4().to_string();
                }
                write(&mut client, &request(&i, INPUT).unwrap())
                    .await
                    .unwrap();
            }
            assert!(worker.await.unwrap().is_err());
            let trace = store.task(&grant.task).unwrap();
            assert_eq!(
                trace["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["payload"]["state"] == "authorized")
                    .count(),
                usize::from(case != 0)
            );
            assert!(trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["payload"]["state"] == "denied"));
        }
    }
    #[tokio::test]
    async fn corrupt_frames_and_forged_responses_close_connection_permanently() {
        for forged in [true, false] {
            let (_, grant) = setup(2, 10);
            let (client, mut server) = UnixStream::pair().unwrap();
            let adapter = Arc::new(
                UnixSql::connected(client, grant.clone(), uid(), std::process::id()).unwrap(),
            );
            let broker = crate::fabric::tools::Broker::default();
            broker
                .register_sql(grant.descriptor.clone(), adapter.clone())
                .unwrap();
            let pinned = broker.pin_capability("sql.query").unwrap().unwrap();
            let peer = tokio::spawn(async move {
                let (_, mut decoded) = read(&mut server).await.unwrap().unwrap();
                if forged {
                    decoded.identity.task = uuid::Uuid::new_v4().to_string();
                    let bytes = super::super::encode(
                        &decoded.identity,
                        None,
                        Some(r#"{"columns":["answer"],"rows":[[42]]}"#),
                        false,
                    )
                    .unwrap();
                    write(&mut server, &bytes).await.unwrap();
                } else {
                    server.write_u32(262145).await.unwrap();
                }
            });
            assert!(adapter.query(&grant.task, INPUT).await.is_err());
            peer.await.unwrap();
            assert!(adapter.connection.lock().await.stream.is_none());
            assert_eq!(adapter.availability(), crate::fabric::Availability::Closed);
            assert!(broker.pin_capability("sql.query").unwrap().is_none());
            assert!(broker
                .activate(&grant.descriptor.id, &grant.descriptor.version)
                .is_err());
            assert!(pinned
                .query_sql(&grant.task, "jev-plus", INPUT)
                .await
                .unwrap_err()
                .to_string()
                .contains("unavailable"));
            assert!(broker
                .statuses()
                .unwrap()
                .iter()
                .any(|status| status.descriptor == grant.descriptor
                    && status.active
                    && status.availability == crate::fabric::Availability::Closed));
            assert!(adapter.query(&grant.task, INPUT).await.is_err());
        }
    }
    struct Pending(Arc<tokio::sync::Notify>);
    impl SqlBackend for Pending {
        fn query<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async {
                self.0.notify_one();
                std::future::pending().await
            })
        }
    }
    #[tokio::test]
    async fn cancellation_closes_stream_and_stops_pending_worker_future() {
        let (store, grant) = setup(1, 10);
        let (client, server) = UnixStream::pair().unwrap();
        let adapter =
            Arc::new(UnixSql::connected(client, grant.clone(), uid(), std::process::id()).unwrap());
        let entered = Arc::new(tokio::sync::Notify::new());
        let (_stop, receive) = watch::channel(false);
        let worker = tokio::spawn(serve(
            server,
            grant.clone(),
            uid(),
            std::process::id(),
            Arc::new(Pending(entered.clone())),
            store,
            receive,
        ));
        let caller_adapter = adapter.clone();
        let caller = tokio::spawn(async move { caller_adapter.query(&grant.task, INPUT).await });
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
    async fn authorization_audit_failure_prevents_backend_work() {
        let (store, grant) = setup(1, 10);
        store.lock().unwrap().execute_batch("CREATE TRIGGER fail_sql_authorization BEFORE INSERT ON events WHEN NEW.kind='tool_worker' AND json_extract(NEW.payload,'$.state')='authorized' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        let entered = Arc::new(tokio::sync::Notify::new());
        let (mut client, server) = UnixStream::pair().unwrap();
        let (_stop, receive) = watch::channel(false);
        let worker = tokio::spawn(serve(
            server,
            grant.clone(),
            uid(),
            std::process::id(),
            Arc::new(Pending(entered.clone())),
            store,
            receive,
        ));
        write(&mut client, &request(&identity(&grant), INPUT).unwrap())
            .await
            .unwrap();
        assert!(worker.await.unwrap().is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(10), entered.notified())
                .await
                .is_err()
        );
    }
    #[tokio::test(start_paused = true)]
    async fn expired_adapter_is_not_routable_even_with_an_unused_stream() {
        let (_, grant) = setup(1, 1);
        let (client, _server) = UnixStream::pair().unwrap();
        let adapter =
            Arc::new(UnixSql::connected(client, grant.clone(), uid(), std::process::id()).unwrap());
        let broker = crate::fabric::tools::Broker::default();
        broker
            .register_sql(grant.descriptor.clone(), adapter.clone())
            .unwrap();
        let pinned = broker.pin_capability("sql.query").unwrap().unwrap();
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(adapter.availability(), crate::fabric::Availability::Expired);
        assert!(broker.pin_capability("sql.query").unwrap().is_none());
        assert!(broker
            .activate(&grant.descriptor.id, &grant.descriptor.version)
            .is_err());
        assert!(pinned
            .query_sql(&grant.task, "jev-plus", INPUT)
            .await
            .is_err());
        assert_eq!(adapter.connection.lock().await.calls, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn incomplete_frames_and_idle_sessions_have_absolute_limits() {
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
            uid(),
            std::process::id(),
            Arc::new(super::super::LocalSql),
            store,
            receive
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("expired"));
    }
    #[test]
    fn child_process_fixture() {
        let Some(socket) = std::env::var_os("COGFABRIC_TEST_SQL_SOCKET") else {
            return;
        };
        let task = std::env::var("COGFABRIC_TEST_SQL_TASK").unwrap();
        let parent = std::env::var("COGFABRIC_TEST_SQL_PARENT")
            .unwrap()
            .parse()
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let store = Arc::new(crate::store::Store::in_memory().unwrap());
            store
                .create_named_task(&task, "SQL child fixture", "fast")
                .unwrap();
            let grant = Grant::new(
                &task,
                Descriptor {
                    id: "sql.local".into(),
                    version: "2.0.0".into(),
                    capability: "sql.query".into(),
                },
                1,
                10,
            )
            .unwrap();
            let stream = UnixStream::connect(socket).await.unwrap();
            let (_stop, receive) = watch::channel(false);
            serve(
                stream,
                grant,
                uid(),
                parent,
                Arc::new(super::super::LocalSql),
                store,
                receive,
            )
            .await
            .unwrap();
        });
    }
    #[tokio::test]
    async fn separate_process_executes_structured_sql() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let directory =
            std::env::temp_dir().join(format!("cogfabric-sql-module-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let socket = directory.join("sql.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (_, grant) = setup(1, 10);
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fabric::tools::sql::unix::tests::child_process_fixture",
            ])
            .env("COGFABRIC_TEST_SQL_SOCKET", &socket)
            .env("COGFABRIC_TEST_SQL_TASK", &grant.task)
            .env("COGFABRIC_TEST_SQL_PARENT", std::process::id().to_string())
            .stdout(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let (stream, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let adapter =
            UnixSql::connected(stream, grant.clone(), uid(), child.id().unwrap()).unwrap();
        let answer = adapter.query(&grant.task, INPUT).await.unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&answer).unwrap()["rows"][0][0],
            42
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
}
