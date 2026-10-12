//! Opt-in same-binary inference workers, with one immutable task per process.
use super::{
    bootstrap::Bootstrap,
    worker::{self, Audit},
    UnixInference,
};
use crate::{
    backend::{Generation, InferenceBackend, LocalBackend},
    config::Config,
    fabric::{protocol::ROLES, ModuleDescriptor},
    store::Store,
};
use anyhow::{bail, Context, Result};
use std::{
    future::Future,
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::{watch, Mutex, OnceCell},
};

pub(crate) struct SocketDirectory {
    path: PathBuf,
    dev: u64,
    ino: u64,
    socket: Option<(u64, u64)>,
}
impl SocketDirectory {
    pub(crate) fn record(&self, store: &Store, task: &str) -> Result<()> {
        let (socket_device, socket_inode) =
            self.socket.context("Worker socket identity missing")?;
        let record = crate::recovery::worker_files::SocketRecord {
            format_version: 1,
            name: self
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .context("Invalid worker directory name")?
                .into(),
            uid: unsafe { libc::geteuid() },
            directory_device: self.dev,
            directory_inode: self.ino,
            socket_device,
            socket_inode,
        };
        store.event(
            task,
            "worker_socket",
            "fabric",
            serde_json::to_value(record)?,
        )
    }
    pub(crate) fn bind_tool() -> Result<(Self, UnixListener, PathBuf)> {
        let directory =
            PathBuf::from("/tmp").join(format!("cogfabric-tool-worker-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let identity = std::fs::symlink_metadata(&directory)?;
        let mut guard = Self {
            path: directory.clone(),
            dev: identity.dev(),
            ino: identity.ino(),
            socket: None,
        };
        let socket = directory.join("worker.sock");
        let listener = UnixListener::bind(&socket)?;
        let metadata = std::fs::symlink_metadata(&socket)?;
        guard.socket = Some((metadata.dev(), metadata.ino()));
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        Ok((guard, listener, socket))
    }
}
impl Drop for SocketDirectory {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.is_dir() && m.dev() == self.dev && m.ino() == self.ino)
        {
            let socket = self.path.join("worker.sock");
            if std::fs::symlink_metadata(&socket)
                .is_ok_and(|m| m.file_type().is_socket() && self.socket == Some((m.dev(), m.ino())))
            {
                let _ = std::fs::remove_file(socket);
            }
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}
struct Managed {
    adapter: UnixInference,
    // Tokio kills/reaps this same-binary child when task ownership ends.
    child: Mutex<tokio::process::Child>,
    _socket: SocketDirectory,
    _lease: Option<crate::recovery::ExecutionLease>,
}
struct Starting {
    child: tokio::process::Child,
    socket: SocketDirectory,
    lease: Option<crate::recovery::ExecutionLease>,
}
pub struct ProcessTask {
    cfg: Config,
    store: Arc<Store>,
    task: String,
    worker: OnceCell<Managed>,
    starting: Mutex<Option<Starting>>,
    stopped: AtomicBool,
    initialization: Mutex<()>,
}
impl ProcessTask {
    pub async fn shutdown(&self) -> Result<()> {
        self.stopped.store(true, Ordering::Release);
        let _initialization = self.initialization.lock().await;
        if let Some(worker) = self.worker.get() {
            worker.adapter.stream.lock().await.take();
            let mut child = worker.child.lock().await;
            return self.reap(&mut child).await;
        }
        if let Some(mut starting) = self.starting.lock().await.take() {
            return self.reap(&mut starting.child).await;
        }
        Ok(())
    }
    async fn reap(&self, child: &mut tokio::process::Child) -> Result<()> {
        let pid = child.id();
        let mut forced = false;
        let status = match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                forced = true;
                child.start_kill()?;
                tokio::time::timeout(Duration::from_secs(2), child.wait())
                    .await
                    .context("Worker reap deadline exhausted")??
            }
        };
        self.store.event(
            &self.task,
            "worker_process_stopped",
            "fabric",
            serde_json::json!({"pid":pid,"forced":forced,"exit_code":status.code(),"reaped":true}),
        )?;
        Ok(())
    }
    pub fn new(cfg: Config, store: Arc<Store>, task: &str) -> Result<Self> {
        cfg.validate()?;
        uuid::Uuid::parse_str(task)?;
        Audit::new(store.clone(), task)?;
        Ok(Self {
            cfg,
            store,
            task: task.into(),
            worker: OnceCell::new(),
            starting: Mutex::new(None),
            stopped: AtomicBool::new(false),
            initialization: Mutex::new(()),
        })
    }
    async fn spawn_owned(
        &self,
        command: &mut tokio::process::Command,
        socket: SocketDirectory,
        lease: Option<crate::recovery::ExecutionLease>,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<Starting>>> {
        socket.record(&self.store, &self.task)?;
        self.store.event(
            &self.task,
            "worker_process_launch",
            "fabric",
            serde_json::json!({"sandboxed":false}),
        )?;
        let mut pending = self.starting.lock().await;
        if pending.is_some() {
            bail!("An unfinished inference worker startup cannot be retried")
        }
        let child = command.spawn()?;
        let pid = child.id().context("Worker process has no PID")?;
        *pending = Some(Starting {
            child,
            socket,
            lease,
        });
        self.store.event(
            &self.task,
            "worker_process_owned",
            "fabric",
            serde_json::json!({"pid":pid,"sandboxed":false}),
        )?;
        Ok(pending)
    }
    async fn launch(&self) -> Result<Managed> {
        let lease = self.store.execution_lease()?;
        let running: bool = self.store.lock()?.query_row(
            "SELECT status='running' FROM tasks WHERE id=?1",
            [&self.task],
            |row| row.get(0),
        )?;
        if !running {
            bail!("Cannot launch inference for a terminal task")
        }
        let started = std::time::Instant::now();
        let home = std::fs::canonicalize(&self.cfg.home)?;
        let metadata = std::fs::symlink_metadata(&home)?;
        if !metadata.is_dir() || metadata.mode() & 0o777 != 0o700 {
            bail!("Worker requires a private existing home")
        }
        let directory =
            PathBuf::from("/tmp").join(format!("cogfabric-worker-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let identity = std::fs::symlink_metadata(&directory)?;
        let mut guard = SocketDirectory {
            path: directory.clone(),
            dev: identity.dev(),
            ino: identity.ino(),
            socket: None,
        };
        let socket = directory.join("worker.sock");
        let listener = UnixListener::bind(&socket)?;
        let socket_metadata = std::fs::symlink_metadata(&socket)?;
        guard.socket = Some((socket_metadata.dev(), socket_metadata.ino()));
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let credential = if self.cfg.backend == "openai-local" {
            crate::credentials::resolve(&self.cfg)?
        } else {
            None
        };
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .arg("--home")
            .arg(&home)
            .arg("inference-worker")
            .arg("--socket")
            .arg(&socket)
            .arg("--host-pid")
            .arg(std::process::id().to_string())
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        if let Some(credential) = credential {
            credential.forward(&mut command);
        }
        let mut pending = self.spawn_owned(&mut command, guard, lease).await?;
        let pid = pending
            .as_ref()
            .and_then(|s| s.child.id())
            .context("Worker process has no PID")?;
        let starting = pending
            .as_mut()
            .context("Worker startup ownership missing")?;
        let stream = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(10), listener.accept()) => result.context("Worker connection deadline exhausted")??.0,
            result = starting.child.wait() => bail!("Worker exited before connecting: {:?}", result?),
        };
        super::authenticate(&stream, metadata.uid(), pid)?;
        let mut stream = stream;
        Bootstrap {
            task: self.task.clone(),
            config: self.cfg.clone(),
        }
        .send(&mut stream)
        .await?;
        let adapter = UnixInference::connected(
            stream,
            ModuleDescriptor {
                id: "inference.local".into(),
                version: "1.0.0".into(),
                roles: ROLES.iter().map(|r| (*r).into()).collect(),
            },
            metadata.uid(),
            pid,
        )?;
        self.store.event(&self.task, "worker_process", "fabric", serde_json::json!({"pid":pid,"launch_handoff_ms":started.elapsed().as_secs_f64()*1000.0,"weight_loading_measured":false,"sandboxed":false}))?;
        drop(listener);
        let starting = pending.take().context("Worker startup ownership missing")?;
        Ok(Managed {
            adapter,
            child: Mutex::new(starting.child),
            _socket: starting.socket,
            _lease: starting.lease,
        })
    }
}
impl InferenceBackend for ProcessTask {
    fn availability(&self) -> crate::fabric::Availability {
        if self.stopped.load(Ordering::Acquire) {
            return crate::fabric::Availability::Closed;
        }
        self.worker
            .get()
            .map_or(crate::fabric::Availability::Unknown, |worker| {
                worker.adapter.availability()
            })
    }
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.generate_for(&self.task, role, prompt, tokens)
    }
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            if task != self.task {
                bail!("Inference worker belongs to another task")
            }
            super::actor(role)?;
            if prompt.len() > 32_000 || tokens == 0 || tokens > 16_384 {
                bail!("Inference worker request exceeds bounds")
            }
            let initialization = self.initialization.lock().await;
            if self.stopped.load(Ordering::Acquire) {
                bail!("Inference worker has been shut down")
            }
            let worker = self.worker.get_or_try_init(|| self.launch()).await?;
            drop(initialization);
            if self.stopped.load(Ordering::Acquire) {
                bail!("Inference worker has been shut down")
            }
            worker
                .adapter
                .generate_for(task, role, prompt, tokens)
                .await
        })
    }
}

/// Internal CLI entry point. Generation settings come only from authenticated IPC.
pub async fn run_worker(home: &Path, socket: &Path, host_pid: u32) -> Result<()> {
    let metadata = std::fs::symlink_metadata(home)?;
    if !metadata.is_dir() || metadata.mode() & 0o777 != 0o700 {
        bail!("Worker home must be a private existing directory")
    }
    let mut stream =
        tokio::time::timeout(Duration::from_secs(10), UnixStream::connect(socket)).await??;
    let mut setup = Bootstrap::receive(&mut stream, metadata.uid(), host_pid).await?;
    setup.config.home = home.into();
    let store = Arc::new(Store::open_with_limit(
        &setup.config.db(),
        setup.config.max_database_bytes,
    )?);
    let _lease = store.execution_lease()?;
    let running: bool = store.lock()?.query_row(
        "SELECT status='running' FROM tasks WHERE id=?1",
        [&setup.task],
        |row| row.get(0),
    )?;
    if !running {
        bail!("Worker task is no longer running")
    }
    let audit = Audit::new(store, &setup.task)?;
    let authority = setup.authority(metadata.uid(), host_pid)?;
    let backend = Arc::new(LocalBackend(setup.config));
    let (_shutdown, receiver) = watch::channel(false);
    worker::serve(stream, authority, backend, receiver, audit).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_child_fixture() {
        if std::env::var_os("COGFABRIC_TEST_INFERENCE_STARTUP_HOLD").is_some() {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    #[tokio::test]
    async fn cancelled_startup_and_owned_audit_failure_retain_reapable_children() {
        for fail_audit in [false, true] {
            let store = Arc::new(Store::in_memory().unwrap());
            let task = store
                .create_task("Inference startup ownership", "fast")
                .unwrap();
            let process =
                Arc::new(ProcessTask::new(Config::default(), store.clone(), &task).unwrap());
            if fail_audit {
                store.lock().unwrap().execute_batch("CREATE TRIGGER fail_owned BEFORE INSERT ON events WHEN NEW.kind='worker_process_owned' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
            }
            let (socket, listener, path) = SocketDirectory::bind_tool().unwrap();
            let directory = path.parent().unwrap().to_path_buf();
            let entered = Arc::new(tokio::sync::Notify::new());
            let startup_process = process.clone();
            let startup_entered = entered.clone();
            let startup = tokio::spawn(async move {
                let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
                command
                    .args([
                        "--exact",
                        "fabric::unix::process::tests::startup_child_fixture",
                    ])
                    .env("COGFABRIC_TEST_INFERENCE_STARTUP_HOLD", "1")
                    .stdout(std::process::Stdio::null())
                    .kill_on_drop(true);
                let owned = startup_process
                    .spawn_owned(&mut command, socket, None)
                    .await?;
                startup_entered.notify_one();
                let _owned = owned;
                std::future::pending::<Result<()>>().await
            });
            if fail_audit {
                assert!(startup.await.unwrap().is_err());
            } else {
                entered.notified().await;
                startup.abort();
                assert!(startup.await.unwrap_err().is_cancelled());
            }
            assert!(process.worker.get().is_none());
            assert!(process.starting.lock().await.is_some());
            // A failed initialization must not silently spawn a second child.
            let (another_socket, another_listener, another_path) =
                SocketDirectory::bind_tool().unwrap();
            let mut unused_command =
                tokio::process::Command::new("/nonexistent-inference-test-command");
            assert!(process
                .spawn_owned(&mut unused_command, another_socket, None)
                .await
                .err()
                .unwrap()
                .to_string()
                .contains("cannot be retried"));
            drop(another_listener);
            assert!(!another_path.parent().unwrap().exists());
            drop(listener);
            process.shutdown().await.unwrap();
            assert!(process.starting.lock().await.is_none());
            assert!(!directory.exists());
            let trace = store.task(&task).unwrap();
            assert_eq!(
                trace["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["kind"] == "worker_process_stopped"
                        && e["payload"]["reaped"] == true
                        && e["payload"]["forced"] == true)
                    .count(),
                1
            );
        }
    }
    #[tokio::test]
    async fn shutdown_before_initialization_permanently_refuses_launch() {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Shutdown before launch", "fast").unwrap();
        let process = ProcessTask::new(Config::default(), store.clone(), &task).unwrap();
        process.shutdown().await.unwrap();
        process.shutdown().await.unwrap();
        assert_eq!(process.availability(), crate::fabric::Availability::Closed);
        let error = process
            .generate_for(&task, "solver", "goal", 128)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("shut down"));
        assert!(process.worker.get().is_none());
        assert!(process.starting.lock().await.is_none());
        assert!(!store.task(&task).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "worker_process_launch"));
    }

    #[tokio::test]
    async fn managed_inference_reports_connected_and_shutdown_adapter_state() {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Managed availability", "fast").unwrap();
        let process = Arc::new(ProcessTask::new(Config::default(), store, &task).unwrap());
        assert_eq!(process.availability(), crate::fabric::Availability::Unknown);
        let (client, _server) = UnixStream::pair().unwrap();
        let adapter = UnixInference::connected(
            client,
            ModuleDescriptor {
                id: "inference.local".into(),
                version: "1.0.0".into(),
                roles: vec!["solver".into()],
            },
            unsafe { libc::geteuid() },
            std::process::id(),
        )
        .unwrap();
        let (socket, _listener, _path) = SocketDirectory::bind_tool().unwrap();
        let child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        assert!(process
            .worker
            .set(Managed {
                adapter,
                child: Mutex::new(child),
                _socket: socket,
                _lease: None
            })
            .is_ok());
        assert_eq!(
            process.availability(),
            crate::fabric::Availability::Connected
        );
        let initialization = process.initialization.lock().await;
        let cleanup_process = process.clone();
        let cleanup = tokio::spawn(async move { cleanup_process.shutdown().await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while !process.stopped.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!cleanup.is_finished());
        drop(initialization);
        cleanup.await.unwrap().unwrap();
        assert_eq!(process.availability(), crate::fabric::Availability::Closed);
    }

    #[tokio::test]
    async fn wrong_task_never_initializes_a_process() {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Worker scope", "fast").unwrap();
        let backend = ProcessTask::new(Config::default(), store.clone(), &task).unwrap();
        assert!(backend
            .generate_for(&uuid::Uuid::new_v4().to_string(), "solver", "goal", 128)
            .await
            .is_err());
        assert!(backend.worker.get().is_none());
        store
            .finish_task(&task, "interrupted", "No replay")
            .unwrap();
        assert!(backend
            .generate_for(&task, "solver", "goal", 128)
            .await
            .unwrap_err()
            .to_string()
            .contains("terminal task"));
        assert!(backend.worker.get().is_none());
    }
    #[tokio::test]
    async fn custom_registry_is_not_silently_replaced_by_process_mode() {
        let store = Arc::new(Store::in_memory().unwrap());
        let engine = crate::engine::Engine::new(
            Config {
                inference_process: true,
                ..Config::default()
            },
            store.clone(),
        );
        engine.broker.unregister("inference.local").unwrap();
        assert!(engine
            .run("goal", "fast")
            .await
            .unwrap_err()
            .to_string()
            .contains("default inference registry"));
        let count: u32 = store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
