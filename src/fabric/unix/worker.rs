//! Bounded worker sessions with authority supplied by trusted host setup only.
#[cfg(test)]
use super::read;
use super::{authenticate, read_optional, write};
use crate::{
    backend::InferenceBackend,
    fabric::{actor, now_ms, ModuleDescriptor, Packet, Payload},
    security::TaskPolicy,
};
use anyhow::{anyhow, bail, Result};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::{io::AsyncReadExt, net::UnixStream, sync::watch};

/// Audit attachment chosen by trusted setup, never by a request's task field.
pub struct Audit {
    store: Arc<crate::store::Store>,
    anchor: String,
    session: String,
}
impl Audit {
    pub fn new(store: Arc<crate::store::Store>, anchor: &str) -> Result<Self> {
        uuid::Uuid::parse_str(anchor)?;
        let exists: bool = store.lock()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [anchor],
            |row| row.get(0),
        )?;
        if !exists {
            bail!("Worker audit anchor must exist")
        }
        Ok(Self {
            store,
            anchor: anchor.into(),
            session: uuid::Uuid::new_v4().to_string(),
        })
    }
    fn record(&self, state: &str, authority: &Authority, packet: Option<&Packet>) -> Result<()> {
        // No prompts, answers, backend errors or model-controlled metadata.
        let request = packet.map(
            |p| serde_json::json!({"task_id":p.header.task,"correlation_id":p.header.correlation}),
        );
        let charged_calls: u32 = authority.tasks.values().map(|u| u.calls).sum();
        let reserved_output_tokens: u64 = authority.tasks.values().map(|u| u.tokens).sum();
        let charged_prompt_bytes: u64 = authority.tasks.values().map(|u| u.bytes).sum();
        self.store.worker_audit(
            &self.anchor,
            serde_json::json!({
                "format_version":1,"session_id":self.session,"state":state,
                "module_id":authority.descriptor.id,"module_version":authority.descriptor.version,
                "approved_tasks":authority.tasks.len(),"request":request,"session_seconds":authority.session_seconds,
                "charged_calls":charged_calls,"reserved_output_tokens":reserved_output_tokens,
                "charged_prompt_bytes":charged_prompt_bytes
            }),
        )
    }
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub calls_per_task: u32,
    pub output_tokens_per_task: u64,
    pub prompt_bytes_per_task: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            calls_per_task: 8,
            output_tokens_per_task: 1600,
            prompt_bytes_per_task: 128_000,
        }
    }
}

/// Not deserializable: connection messages cannot grant tasks, roles or budgets.
pub struct Authority {
    descriptor: ModuleDescriptor,
    host_uid: u32,
    host_pid: u32,
    tasks: BTreeMap<String, Usage>,
    limits: Limits,
    session_seconds: u64,
}
#[derive(Default)]
struct Usage {
    calls: u32,
    tokens: u64,
    bytes: u64,
}
impl Authority {
    pub fn new(
        descriptor: ModuleDescriptor,
        host_uid: u32,
        host_pid: u32,
        tasks: &[String],
        limits: Limits,
    ) -> Result<Self> {
        descriptor.validate()?;
        if host_pid == 0
            || tasks.is_empty()
            || tasks.len() > 64
            || !(1..=128).contains(&limits.calls_per_task)
            || !(1..=1_048_576).contains(&limits.output_tokens_per_task)
            || !(1..=4_194_304).contains(&limits.prompt_bytes_per_task)
        {
            bail!("Invalid worker authority bounds")
        }
        let mut approved = BTreeMap::new();
        for task in tasks {
            TaskPolicy::reasoning(task)?;
            if approved.insert(task.clone(), Usage::default()).is_some() {
                bail!("Duplicate worker task grant")
            }
        }
        Ok(Self {
            descriptor,
            host_uid,
            host_pid,
            tasks: approved,
            limits,
            session_seconds: 300,
        })
    }
    pub fn with_session_seconds(mut self, seconds: u64) -> Result<Self> {
        if !(1..=3600).contains(&seconds) {
            bail!("Worker lifetime exceeds approved bounds")
        }
        self.session_seconds = seconds;
        Ok(self)
    }
    fn charge(&mut self, packet: &Packet) -> Result<Duration> {
        let Payload::Request {
            role,
            prompt,
            tokens,
        } = &packet.payload
        else {
            bail!("Worker expects an inference request")
        };
        let header = &packet.header;
        let now = now_ms();
        if header.destination != self.descriptor.id
            || header.module_version != self.descriptor.version
            || header.sender != actor(role)?
            || !self.descriptor.roles.contains(role)
            || header.expires <= now
            || header.expires.saturating_sub(now) > 300_000
        {
            bail!("Worker identity, role or deadline denied")
        }
        TaskPolicy::reasoning(&header.task)?.authorize(
            &header.task,
            &header.sender,
            "llm.generate",
        )?;
        let usage = self
            .tasks
            .get_mut(&header.task)
            .ok_or_else(|| anyhow!("Worker task is not approved"))?;
        let requested_tokens = *tokens as u64;
        let requested_bytes = prompt.len() as u64;
        if usage.calls >= self.limits.calls_per_task
            || requested_tokens
                > self
                    .limits
                    .output_tokens_per_task
                    .saturating_sub(usage.tokens)
            || requested_bytes
                > self
                    .limits
                    .prompt_bytes_per_task
                    .saturating_sub(usage.bytes)
        {
            bail!("Worker task budget exhausted")
        }
        // Charge dispatched calls and reserved output even if inference fails.
        usage.calls += 1;
        usage.tokens += requested_tokens;
        usage.bytes += requested_bytes;
        Ok(Duration::from_millis(header.expires - now))
    }
}

/// Serve one authenticated connection for its approved lifetime/1024 exchanges.
/// Disconnect, extra pipelined bytes, shutdown or expiry drops active inference.
pub async fn serve(
    mut stream: UnixStream,
    mut authority: Authority,
    backend: Arc<dyn InferenceBackend>,
    mut shutdown: watch::Receiver<bool>,
    audit: Audit,
) -> Result<()> {
    let _execution_lease = audit.store.execution_lease()?;
    audit.record("session_started", &authority, None)?;
    let result = async {
    authenticate(&stream, authority.host_uid, authority.host_pid)?;
    tokio::time::timeout(Duration::from_secs(authority.session_seconds), async {
        for _ in 0..1024 {
            if *shutdown.borrow() { return Ok(()); }
            let packet = tokio::select! {
                biased;
                _ = shutdown.changed() => return Ok(()),
                result = read_optional(&mut stream) => match result? { Some(packet) => packet, None => return Ok(()) },
            };
            let remaining = match authority.charge(&packet) {
                Ok(remaining) => remaining,
                Err(error) => { audit.record("denied", &authority, Some(&packet))?; return Err(error); }
            };
            audit.record("authorized", &authority, Some(&packet))?;
            let remaining = remaining.min(Duration::from_millis(packet.header.expires.saturating_sub(now_ms())));
            if remaining.is_zero() { bail!("Worker deadline exhausted during authorization audit") }
            let Payload::Request { role, prompt, tokens } = &packet.payload else { unreachable!() };
            let mut extra = [0u8; 1];
            let result = tokio::select! {
                biased;
                _ = shutdown.changed() => return Ok(()),
                _ = stream.read(&mut extra) => bail!("Worker host disconnected or pipelined data"),
                result = tokio::time::timeout(remaining, backend.generate_for(&packet.header.task, role, prompt, *tokens)) => result.map_err(|_| anyhow!("Worker inference deadline exhausted"))?,
            };
            let payload = match result {
                Ok(report) if report.output_tokens.is_some_and(|n| n > *tokens as u64) => bail!("Worker backend exceeded output ceiling"),
                Ok(report) => Payload::Response(report),
                Err(error) => Payload::Error(error.to_string().chars().take(250).collect()),
            };
            let response = Packet { header: packet.header, payload };
            // Validate backend output locally before transmitting it.
            super::decode(&super::encode(&response)?)?;
            audit.record(if matches!(response.payload, Payload::Response(_)) { "inference_completed" } else { "inference_failed" }, &authority, Some(&response))?;
            if now_ms() >= response.header.expires { bail!("Worker deadline exhausted during outcome audit") }
            tokio::select! {
                biased;
                _ = shutdown.changed() => return Ok(()),
                result = tokio::time::timeout(Duration::from_secs(2), write(&mut stream, &response)) => result.map_err(|_| anyhow!("Worker write deadline exhausted"))??,
            }
        }
        bail!("Worker session exchange limit exhausted")
    }).await.map_err(|_| anyhow!("Worker session deadline exhausted"))?
    }.await;
    audit.record(
        if result.is_ok() {
            "session_stopped"
        } else {
            "session_failed"
        },
        &authority,
        None,
    )?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    fn audit() -> Audit {
        let store = Arc::new(crate::store::Store::in_memory().unwrap());
        let anchor = store.create_task("Worker fixture audit", "fast").unwrap();
        Audit::new(store, &anchor).unwrap()
    }
    #[tokio::test(start_paused = true)]
    async fn approved_lifetime_allows_idle_time_before_the_next_request() {
        let task = uuid::Uuid::new_v4().to_string();
        let (mut host, worker) = UnixStream::pair().unwrap();
        let authority = Authority::new(
            descriptor(),
            worker.peer_cred().unwrap().uid(),
            std::process::id(),
            std::slice::from_ref(&task),
            Limits::default(),
        )
        .unwrap()
        .with_session_seconds(30)
        .unwrap();
        let (_shutdown, receiver) = watch::channel(false);
        let handle = tokio::spawn(serve(
            worker,
            authority,
            Arc::new(Fixture(AtomicUsize::new(0))),
            receiver,
            audit(),
        ));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(11)).await;
        assert!(!handle.is_finished());
        // Real socket readiness must not race the paused clock's automatic
        // advance to session expiry when the executor is idle on a busy runner.
        tokio::time::resume();
        write(&mut host, &packet(&task)).await.unwrap();
        assert!(matches!(
            read(&mut host).await.unwrap().payload,
            Payload::Response(_)
        ));
        drop(host);
        handle.await.unwrap().unwrap();
    }
    #[tokio::test(start_paused = true)]
    async fn incomplete_frames_and_session_expiry_remain_bounded() {
        for partial in [false, true] {
            let task = uuid::Uuid::new_v4().to_string();
            let (mut host, worker) = UnixStream::pair().unwrap();
            let authority = Authority::new(
                descriptor(),
                worker.peer_cred().unwrap().uid(),
                std::process::id(),
                std::slice::from_ref(&task),
                Limits::default(),
            )
            .unwrap()
            .with_session_seconds(if partial { 30 } else { 1 })
            .unwrap();
            if partial {
                host.write_u8(0).await.unwrap();
                worker.readable().await.unwrap();
            }
            let (_shutdown, receiver) = watch::channel(false);
            let backend = Arc::new(Fixture(AtomicUsize::new(0)));
            let handle = tokio::spawn(serve(worker, authority, backend.clone(), receiver, audit()));
            tokio::task::yield_now().await;
            tokio::time::advance(Duration::from_secs(if partial { 11 } else { 2 })).await;
            let error = handle.await.unwrap().unwrap_err().to_string();
            assert!(
                error.contains(if partial {
                    "Incomplete Unix module frame"
                } else {
                    "Worker session deadline"
                }),
                "{error}"
            );
            assert_eq!(backend.0.load(Ordering::SeqCst), 0);
        }
    }
    #[test]
    fn missing_or_removed_audit_anchors_fail_without_orphan_records() {
        let audit = audit();
        let unknown = uuid::Uuid::new_v4().to_string();
        assert!(Audit::new(audit.store.clone(), &unknown).is_err());
        let authority = Authority::new(descriptor(), 0, 1, &[unknown], Limits::default()).unwrap();
        audit
            .store
            .lock()
            .unwrap()
            .execute("DELETE FROM tasks WHERE id=?1", [&audit.anchor])
            .unwrap();
        assert!(audit.record("session_started", &authority, None).is_err());
        let count: u32 = audit
            .store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    use crate::{backend::Generation, fabric::Header};
    use std::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
    };
    struct Fixture(AtomicUsize);
    impl InferenceBackend for Fixture {
        fn generate<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(Generation {
                    text: "answer".into(),
                    model: "fixture".into(),
                    input_tokens: None,
                    output_tokens: Some(1),
                    elapsed_ms: 0.0,
                    queue_wait_ms: None,
                })
            })
        }
    }
    fn descriptor() -> ModuleDescriptor {
        ModuleDescriptor {
            id: "worker".into(),
            version: "1.0.0".into(),
            roles: vec!["solver".into()],
        }
    }
    #[test]
    fn authority_bounds_tokens_bytes_deadlines_and_versions() {
        let task = uuid::Uuid::new_v4().to_string();
        for seconds in [0, 3601] {
            assert!(Authority::new(
                descriptor(),
                0,
                1,
                std::slice::from_ref(&task),
                Limits::default()
            )
            .unwrap()
            .with_session_seconds(seconds)
            .is_err());
        }
        for limits in [
            Limits {
                output_tokens_per_task: 128,
                ..Limits::default()
            },
            Limits {
                prompt_bytes_per_task: 4,
                ..Limits::default()
            },
        ] {
            let mut authority =
                Authority::new(descriptor(), 0, 1, std::slice::from_ref(&task), limits).unwrap();
            authority.charge(&packet(&task)).unwrap();
            assert!(authority.charge(&packet(&task)).is_err());
        }
        for invalid in 0..4 {
            let mut authority = Authority::new(
                descriptor(),
                0,
                1,
                std::slice::from_ref(&task),
                Limits::default(),
            )
            .unwrap();
            let mut request = packet(&task);
            match invalid {
                0 => request.header.expires = 0,
                1 => request.header.expires = now_ms() + 400_000,
                2 => request.header.module_version = "2.0.0".into(),
                _ => request.header.destination = "other".into(),
            }
            assert!(authority.charge(&request).is_err());
            assert_eq!(authority.tasks[&task].calls, 0);
        }
        assert!(
            Authority::new(descriptor(), 0, 1, &[task.clone(), task], Limits::default()).is_err()
        );
    }

    struct Hanging {
        entered: tokio::sync::Notify,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }
    struct DropFlag(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    impl InferenceBackend for Hanging {
        fn generate<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async move {
                let _guard = DropFlag(self.dropped.clone());
                self.entered.notify_one();
                std::future::pending().await
            })
        }
    }
    #[tokio::test]
    async fn shutdown_and_disconnect_drop_active_inference() {
        for disconnect in [false, true] {
            let task = uuid::Uuid::new_v4().to_string();
            let (mut host, worker) = UnixStream::pair().unwrap();
            let authority = Authority::new(
                descriptor(),
                worker.peer_cred().unwrap().uid(),
                std::process::id(),
                std::slice::from_ref(&task),
                Limits::default(),
            )
            .unwrap();
            let backend = Arc::new(Hanging {
                entered: tokio::sync::Notify::new(),
                dropped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            });
            let (shutdown, receiver) = watch::channel(false);
            let handle = tokio::spawn(serve(worker, authority, backend.clone(), receiver, audit()));
            write(&mut host, &packet(&task)).await.unwrap();
            backend.entered.notified().await;
            if disconnect {
                drop(host);
            } else {
                shutdown.send(true).unwrap();
            }
            let result = tokio::time::timeout(Duration::from_secs(1), handle)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(result.is_err(), disconnect);
            assert!(backend.dropped.load(Ordering::SeqCst));
        }
    }
    fn packet(task: &str) -> Packet {
        Packet {
            header: Header {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: task.into(),
                sender: "jev-r".into(),
                destination: "worker".into(),
                module_version: "1.0.0".into(),
                expires: now_ms() + 10_000,
            },
            payload: Payload::Request {
                role: "solver".into(),
                prompt: "goal".into(),
                tokens: 128,
            },
        }
    }
    #[tokio::test]
    async fn audit_is_durable_private_and_bound_to_the_trusted_anchor() {
        let root =
            std::env::temp_dir().join(format!("fabric-worker-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("audit.db");
        let store = Arc::new(crate::store::Store::open(&path).unwrap());
        let anchor = store.create_task("Worker audit", "fast").unwrap();
        let task = uuid::Uuid::new_v4().to_string();
        let forged = uuid::Uuid::new_v4().to_string();
        let (mut host, worker) = UnixStream::pair().unwrap();
        let authority = Authority::new(
            descriptor(),
            worker.peer_cred().unwrap().uid(),
            std::process::id(),
            std::slice::from_ref(&task),
            Limits::default(),
        )
        .unwrap();
        let (_shutdown, receiver) = watch::channel(false);
        let handle = tokio::spawn(serve(
            worker,
            authority,
            Arc::new(Fixture(AtomicUsize::new(0))),
            receiver,
            Audit::new(store.clone(), &anchor).unwrap(),
        ));
        let mut request = packet(&task);
        if let Payload::Request { prompt, .. } = &mut request.payload {
            *prompt = "PRIVATE PROMPT TOKEN".into();
        }
        write(&mut host, &request).await.unwrap();
        read(&mut host).await.unwrap();
        write(&mut host, &packet(&forged)).await.unwrap();
        assert!(handle.await.unwrap().is_err());
        drop(store);
        let reopened = crate::store::Store::open(&path).unwrap();
        let trace = reopened.task(&anchor).unwrap();
        let events = trace["events"].as_array().unwrap();
        let states: Vec<_> = events
            .iter()
            .map(|e| e["payload"]["state"].as_str().unwrap())
            .collect();
        assert_eq!(
            states,
            [
                "session_started",
                "authorized",
                "inference_completed",
                "denied",
                "session_failed"
            ]
        );
        assert_eq!(events[3]["payload"]["request"]["task_id"], forged);
        assert_eq!(events[3]["payload"]["charged_calls"], 1);
        assert!(!trace.to_string().contains("PRIVATE PROMPT TOKEN"));
        assert!(!trace.to_string().contains("worker answer"));
        assert!(events
            .iter()
            .all(|event| ["prompt", "text", "answer", "error"]
                .iter()
                .all(|field| event["payload"].get(*field).is_none())));
        let count: u32 = reopened
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE task_id!=?1",
                [&anchor],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn failed_authorization_audit_prevents_backend_dispatch() {
        let store = Arc::new(crate::store::Store::in_memory().unwrap());
        let anchor = store.create_task("Worker audit failure", "fast").unwrap();
        store.lock().unwrap().execute_batch("CREATE TRIGGER deny_authorized_audit BEFORE INSERT ON events WHEN json_extract(NEW.payload,'$.state')='authorized' BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END;").unwrap();
        let task = uuid::Uuid::new_v4().to_string();
        let (mut host, worker) = UnixStream::pair().unwrap();
        let authority = Authority::new(
            descriptor(),
            worker.peer_cred().unwrap().uid(),
            std::process::id(),
            std::slice::from_ref(&task),
            Limits::default(),
        )
        .unwrap();
        let backend = Arc::new(Fixture(AtomicUsize::new(0)));
        let (_shutdown, receiver) = watch::channel(false);
        let handle = tokio::spawn(serve(
            worker,
            authority,
            backend.clone(),
            receiver,
            Audit::new(store, &anchor).unwrap(),
        ));
        write(&mut host, &packet(&task)).await.unwrap();
        assert!(handle.await.unwrap().is_err());
        assert_eq!(backend.0.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn adapter_and_worker_exchange_only_preapproved_tasks() {
        let task = uuid::Uuid::new_v4().to_string();
        let (host, worker) = UnixStream::pair().unwrap();
        let uid = host.peer_cred().unwrap().uid();
        let setup = super::super::bootstrap::Bootstrap {
            task: task.clone(),
            config: crate::config::Config::default(),
        };
        let setup = super::super::bootstrap::Bootstrap::decode(&setup.encode().unwrap()).unwrap();
        let authority = setup.authority(uid, std::process::id()).unwrap();
        let mut adapter_descriptor = descriptor();
        adapter_descriptor.id = "inference.local".into();
        let adapter = super::super::UnixInference::connected(
            host,
            adapter_descriptor,
            uid,
            std::process::id(),
        )
        .unwrap();
        let backend = Arc::new(Fixture(AtomicUsize::new(0)));
        let (shutdown, receiver) = watch::channel(false);
        let handle = tokio::spawn(serve(worker, authority, backend.clone(), receiver, audit()));
        assert_eq!(
            adapter
                .generate_for(&task, "solver", "goal", 128)
                .await
                .unwrap()
                .text,
            "answer"
        );
        shutdown.send(true).unwrap();
        handle.await.unwrap().unwrap();
        assert_eq!(backend.0.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn worker_checks_grants_and_cumulative_reservations_before_backend_calls() {
        let task = uuid::Uuid::new_v4().to_string();
        let (mut host, worker) = UnixStream::pair().unwrap();
        let uid = worker.peer_cred().unwrap().uid();
        let authority = Authority::new(
            descriptor(),
            uid,
            std::process::id(),
            std::slice::from_ref(&task),
            Limits {
                calls_per_task: 1,
                ..Limits::default()
            },
        )
        .unwrap();
        let backend = Arc::new(Fixture(AtomicUsize::new(0)));
        let (shutdown, receiver) = watch::channel(false);
        let worker_backend = backend.clone();
        let handle = tokio::spawn(serve(worker, authority, worker_backend, receiver, audit()));
        write(&mut host, &packet(&task)).await.unwrap();
        assert!(matches!(
            read(&mut host).await.unwrap().payload,
            Payload::Response(_)
        ));
        write(&mut host, &packet(&task)).await.unwrap();
        assert!(handle
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("budget"));
        assert_eq!(backend.0.load(Ordering::SeqCst), 1);
        drop(shutdown);
    }
    #[tokio::test]
    async fn unapproved_tasks_and_forged_actors_never_invoke_backend() {
        for forged_actor in [false, true] {
            let task = uuid::Uuid::new_v4().to_string();
            let (mut host, worker) = UnixStream::pair().unwrap();
            let authority = Authority::new(
                descriptor(),
                worker.peer_cred().unwrap().uid(),
                std::process::id(),
                std::slice::from_ref(&task),
                Limits::default(),
            )
            .unwrap();
            let backend = Arc::new(Fixture(AtomicUsize::new(0)));
            let (_shutdown, receiver) = watch::channel(false);
            let handle = tokio::spawn(serve(worker, authority, backend.clone(), receiver, audit()));
            let mut request = packet(&task);
            if forged_actor {
                request.header.sender = "human-cli".into();
            } else {
                request.header.task = uuid::Uuid::new_v4().to_string();
            }
            write(&mut host, &request).await.unwrap();
            assert!(handle.await.unwrap().is_err());
            assert_eq!(backend.0.load(Ordering::SeqCst), 0);
        }
    }
}
