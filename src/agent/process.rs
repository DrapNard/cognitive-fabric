//! Same-image native workers, with explicit child/socket ownership through reaping.
use super::{
    unix::{self, Grant, Limits, UnixChat},
    ChatBackend, Completion, LocalChat, Turn,
};
use crate::{
    config::Config,
    fabric::unix::{
        bootstrap::Bootstrap,
        process::{managed_executable, SocketDirectory},
    },
    recovery::ExecutionLease,
    store::Store,
};
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::{
    future::Future,
    os::unix::fs::MetadataExt,
    path::Path,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
use tokio::{
    net::UnixStream,
    process::Child,
    sync::{Mutex, OwnedSemaphorePermit, Semaphore},
};
static WORKER_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
static CLEANUP_FAILED: AtomicBool = AtomicBool::new(false);
/// Drain native ownership before an embedding runtime shuts down. Cleanup
/// errors remain visible for this host lifetime instead of claiming safe exit.
pub async fn drain_workers() -> Result<()> {
    if let Some(slots) = WORKER_SLOTS.get() {
        let permits =
            tokio::time::timeout(Duration::from_secs(10), slots.clone().acquire_many_owned(8))
                .await
                .context("Native worker shutdown drain deadline exhausted")??;
        drop(permits);
    }
    if CLEANUP_FAILED.load(Ordering::Acquire) {
        bail!("Native worker cleanup encountered errors; inspect its audit")
    }
    Ok(())
}
fn worker_slot() -> Result<OwnedSemaphorePermit> {
    if CLEANUP_FAILED.load(Ordering::Acquire) {
        bail!("Native worker admission disabled after cleanup failure")
    }

    WORKER_SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(8)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| anyhow::anyhow!("Native worker capacity exhausted"))
}
struct Owned {
    child: Child,
    pid: u32,
    forced: bool,
    _socket: SocketDirectory,
    _lease: Option<ExecutionLease>,
    // Drop the capacity permit after the socket and lease cleanup.
    _slot: OwnedSemaphorePermit,
}
pub(crate) struct ProcessTask {
    cfg: Config,
    store: Arc<Store>,
    task: String,
    deadline: Instant,
    used: AtomicBool,
    owned: Mutex<Option<Owned>>,
}
impl ProcessTask {
    pub(crate) fn new(
        cfg: Config,
        store: Arc<Store>,
        task: &str,
        deadline: Instant,
    ) -> Result<Self> {
        cfg.validate()?;
        uuid::Uuid::parse_str(task)?;
        Ok(Self {
            cfg,
            store,
            task: task.into(),
            deadline,
            used: AtomicBool::new(false),
            owned: Mutex::new(None),
        })
    }
    async fn exchange(&self, turn: &Turn, tokens: usize) -> Result<Completion> {
        if self.used.swap(true, Ordering::AcqRel) {
            bail!("Native managed turn cannot be replayed")
        }
        if Instant::now() >= self.deadline {
            bail!("Native managed deadline exhausted before startup")
        }
        let metadata = std::fs::symlink_metadata(&self.cfg.home)?;
        if !metadata.is_dir()
            || metadata.mode() & 0o777 != 0o700
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            bail!("Native managed inference requires an existing private owner workspace")
        }
        let running: bool = self.store.lock()?.query_row(
            "SELECT status='running' FROM tasks WHERE id=?1",
            [&self.task],
            |r| r.get(0),
        )?;
        if !running {
            bail!("Native task is no longer running")
        }
        let slot = worker_slot()?;
        let (socket, listener, path) = SocketDirectory::bind_native()?;
        socket.record(&self.store, &self.task)?;
        let lease = self.store.execution_lease()?;
        let mut cfg = self.cfg.clone();
        cfg.max_calls = 1;
        cfg.max_tokens = tokens;
        cfg.max_wall_seconds = self
            .deadline
            .saturating_duration_since(Instant::now())
            .as_secs()
            .saturating_add(1)
            .clamp(1, 300);
        let credential = if cfg.backend == "openai-local" {
            crate::credentials::resolve(&cfg)?
        } else {
            None
        };
        let mut command = tokio::process::Command::new(managed_executable()?);
        command
            .arg("--home")
            .arg(&self.cfg.home)
            .arg("agent-worker")
            .arg("--socket")
            .arg(&path)
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
        self.store.event(
            &self.task,
            "native_worker_process_launch",
            "fabric",
            json!({"sandboxed":false,"calls_granted":1}),
        )?;
        let mut owned = self.owned.lock().await;
        let child = command.spawn()?;
        let pid = child.id().context("Native child has no PID")?;
        *owned = Some(Owned {
            child,
            _slot: slot,
            pid,
            forced: false,
            _socket: socket,
            _lease: lease,
        });
        self.store.event(
            &self.task,
            "native_worker_process_owned",
            "fabric",
            json!({"pid":pid,"sandboxed":false}),
        )?;
        let child = &mut owned.as_mut().unwrap().child;
        let (stream, _) = tokio::select! {result=tokio::time::timeout_at(tokio::time::Instant::from_std(self.deadline.min(Instant::now()+Duration::from_secs(10))),listener.accept())=>result.context("Native worker connection deadline exhausted")??,result=child.wait()=>bail!("Native child exited before connection: {:?}",result?)};
        let peer = stream.peer_cred()?;
        if peer.uid() != metadata.uid() || peer.pid().map(|p| p as u32) != Some(pid) {
            bail!("Native child connection identity mismatch")
        }
        let mut stream = stream;
        Bootstrap {
            task: self.task.clone(),
            config: cfg.clone(),
        }
        .send(&mut stream)
        .await?;
        let mut grant = Grant::new(
            &self.task,
            metadata.uid(),
            pid,
            Limits {
                output_tokens: tokens,
                prompt_bytes: cfg.max_input_bytes,
                tool_proposals: cfg.max_tool_calls,
                seconds: cfg.max_wall_seconds,
            },
        )?;
        grant.restrict_deadline(self.deadline);
        let adapter = UnixChat::connected(stream, grant)?;
        // Ownership stays in the process wrapper when cancellation drops this call.
        drop(owned);
        adapter.complete_for(&self.task, turn, tokens).await
    }
    pub(crate) async fn shutdown(&self) -> Result<()> {
        let mut owned = self.owned.lock().await;
        if let Some(child) = owned.as_mut() {
            if let Err(error) = reap(child, &self.store, &self.task).await {
                CLEANUP_FAILED.store(true, Ordering::Release);
                return Err(error);
            }
        }
        owned.take();
        Ok(())
    }
}
async fn reap(owned: &mut Owned, store: &Store, task: &str) -> Result<()> {
    let status = match tokio::time::timeout(Duration::from_secs(1), owned.child.wait()).await {
        Ok(status) => status?,
        Err(_) => {
            owned.forced = true;
            owned.child.start_kill()?;
            tokio::time::timeout(Duration::from_secs(2), owned.child.wait())
                .await
                .context("Native child reap deadline exhausted")??
        }
    };
    store.event(
        task,
        "native_worker_process_stopped",
        "fabric",
        json!({"pid":owned.pid,"forced":owned.forced,"exit_code":status.code(),"reaped":true}),
    )
}
impl Drop for ProcessTask {
    fn drop(&mut self) {
        if let Some(mut owned) = self.owned.get_mut().take() {
            let store = self.store.clone();
            let task = self.task.clone();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    if reap(&mut owned, &store, &task).await.is_err() {
                        CLEANUP_FAILED.store(true, Ordering::Release);
                    }
                });
            }
            // Without a live runtime, kill_on_drop remains best effort; no reap is claimed.
        }
    }
}
impl ChatBackend for ProcessTask {
    fn inference_target(&self) -> Result<Option<crate::backend::InferenceTarget>> {
        if self.cfg.backend != "openai-local" {
            return Ok(None);
        }
        crate::backend::configured_target(&self.cfg, None)
    }
    fn complete_for<'a>(
        &'a self,
        task: &'a str,
        turn: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async move {
            if task != self.task {
                bail!("Native process is pinned to another task")
            };
            self.exchange(turn, tokens).await
        })
    }
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async { bail!("Native process dispatch requires its explicit task identity") })
    }
}
/// Hidden same-image entry point; trusted bootstrap is accepted only from the host PID.
pub async fn run_worker(home: &Path, socket: &Path, host_pid: u32) -> Result<()> {
    let metadata = std::fs::symlink_metadata(home)?;
    if !metadata.is_dir()
        || metadata.mode() & 0o777 != 0o700
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        bail!("Native worker home must be private and owner-controlled")
    }
    let mut stream =
        tokio::time::timeout(Duration::from_secs(10), UnixStream::connect(socket)).await??;
    let mut setup = Bootstrap::receive(&mut stream, metadata.uid(), host_pid).await?;
    if setup.config.max_calls != 1 || !setup.config.role_profiles.is_empty() {
        bail!("Native bootstrap must grant one generation without role overrides")
    }
    setup.config.home = home.into();
    let store = Arc::new(Store::open_with_limit(
        &setup.config.db(),
        setup.config.max_database_bytes,
    )?);
    let grant = Grant::new(
        &setup.task,
        metadata.uid(),
        host_pid,
        Limits {
            output_tokens: setup.config.max_tokens,
            prompt_bytes: setup.config.max_input_bytes,
            tool_proposals: setup.config.max_tool_calls,
            seconds: setup.config.max_wall_seconds,
        },
    )?;
    unix::serve_connected(
        stream,
        grant,
        Arc::new(LocalChat(setup.config)),
        store,
        std::future::pending(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn native_worker_slots_remain_bounded_until_ownership_releases() {
        let slots = (0..8).map(|_| worker_slot().unwrap()).collect::<Vec<_>>();
        assert!(worker_slot().is_err());
        drop(slots);
        let slot = worker_slot().unwrap();
        let drain = drain_workers();
        tokio::pin!(drain);
        assert!(tokio::time::timeout(Duration::from_millis(10), &mut drain)
            .await
            .is_err());
        drop(slot);
        drain.await.unwrap();
    }
}
