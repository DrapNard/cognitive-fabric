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
    sync::Arc,
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
pub struct ProcessTask {
    cfg: Config,
    store: Arc<Store>,
    task: String,
    worker: OnceCell<Managed>,
}
impl ProcessTask {
    pub async fn shutdown(&self) -> Result<()> {
        let Some(worker) = self.worker.get() else {
            return Ok(());
        };
        worker.adapter.stream.lock().await.take();
        let mut child = worker.child.lock().await;
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
        })
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
        let mut child = command.spawn()?;
        let pid = child.id().context("Worker process has no PID")?;
        let stream = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(10), listener.accept()) => result.context("Worker connection deadline exhausted")??.0,
            result = child.wait() => bail!("Worker exited before connecting: {:?}", result?),
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
        Ok(Managed {
            adapter,
            child: Mutex::new(child),
            _socket: guard,
            _lease: lease,
        })
    }
}
impl InferenceBackend for ProcessTask {
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
            let worker = self.worker.get_or_try_init(|| self.launch()).await?;
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
