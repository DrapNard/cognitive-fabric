//! Opt-in built-in arithmetic and SQL workers. No arbitrary executable is accepted.
use super::{authenticate, Grant, UnixArithmetic};
use crate::fabric::tools::sql::{
    unix::{Grant as SqlGrant, UnixSql},
    LocalSql, SqlBackend,
};
use crate::{
    config::Config,
    fabric::{
        tools::{ArithmeticBackend, Descriptor, LocalMath},
        unix::process::{managed_executable, SocketDirectory},
    },
    store::Store,
};
use anyhow::{bail, Context, Result};
use std::{
    future::Future,
    os::unix::fs::MetadataExt,
    path::Path,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{watch, Mutex, OnceCell},
};
#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/tool_bootstrap_generated.rs"));
}
use generated::tool_setup as fb;

struct WorkerGrant {
    task: String,
    descriptor: Descriptor,
    calls: u32,
    seconds: u64,
}
impl WorkerGrant {
    fn new(task: &str, descriptor: Descriptor, calls: u32, seconds: u64) -> Result<Self> {
        if descriptor == sql_descriptor() {
            SqlGrant::new(task, descriptor.clone(), calls, seconds)?;
        } else {
            Grant::new(task, descriptor.clone(), calls, seconds)?;
        }
        Ok(Self {
            task: task.into(),
            descriptor,
            calls,
            seconds,
        })
    }
    fn arithmetic(&self) -> Result<Grant> {
        Grant::new(
            &self.task,
            self.descriptor.clone(),
            self.calls,
            self.seconds,
        )
    }
    fn sql(&self) -> Result<SqlGrant> {
        SqlGrant::new(
            &self.task,
            self.descriptor.clone(),
            self.calls,
            self.seconds,
        )
    }
}
struct Setup {
    grant: WorkerGrant,
    database_bytes: u64,
}
impl Setup {
    fn validate(&self) -> Result<()> {
        WorkerGrant::new(
            &self.grant.task,
            self.grant.descriptor.clone(),
            self.grant.calls,
            self.grant.seconds,
        )?;
        if (self.grant.descriptor != descriptor() && self.grant.descriptor != sql_descriptor())
            || self.grant.calls != 1
            || !(64_000_000..=2_000_000_000).contains(&self.database_bytes)
        {
            bail!("Invalid built-in tool worker setup")
        }
        Ok(())
    }
    fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut b = flatbuffers::FlatBufferBuilder::new();
        let args = fb::BootstrapArgs {
            version: if self.grant.descriptor == sql_descriptor() {
                2
            } else {
                1
            },
            task_id: Some(b.create_string(&self.grant.task)),
            module_id: Some(b.create_string(&self.grant.descriptor.id)),
            module_version: Some(b.create_string(&self.grant.descriptor.version)),
            capability: Some(b.create_string(&self.grant.descriptor.capability)),
            calls: self.grant.calls,
            seconds: self.grant.seconds,
            max_database_bytes: self.database_bytes,
        };
        let root = fb::Bootstrap::create(&mut b, &args);
        fb::finish_bootstrap_buffer(&mut b, root);
        Ok(b.finished_data().to_vec())
    }
    fn decode(bytes: &[u8]) -> Result<Self> {
        if !(8..=2048).contains(&bytes.len()) || !fb::bootstrap_buffer_has_identifier(bytes) {
            bail!("Invalid tool bootstrap frame")
        }
        let p = fb::root_as_bootstrap(bytes)?;
        if !matches!(p.version(), 1 | 2) {
            bail!("Unsupported tool bootstrap version")
        }
        let setup = Self {
            grant: WorkerGrant::new(
                p.task_id(),
                Descriptor {
                    id: p.module_id().into(),
                    version: p.module_version().into(),
                    capability: p.capability().into(),
                },
                p.calls(),
                p.seconds(),
            )?,
            database_bytes: p.max_database_bytes(),
        };
        setup.validate()?;
        if (p.version() == 2) != (setup.grant.descriptor == sql_descriptor()) {
            bail!("Tool bootstrap version differs from capability")
        }
        Ok(setup)
    }
    async fn send(&self, stream: &mut UnixStream) -> Result<()> {
        let bytes = self.encode()?;
        tokio::time::timeout(Duration::from_secs(2), async {
            stream.write_u32(u32::try_from(bytes.len())?).await?;
            stream.write_all(&bytes).await?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("Tool bootstrap write deadline exhausted")?
    }
    async fn receive(stream: &mut UnixStream) -> Result<Self> {
        tokio::time::timeout(Duration::from_secs(2), async {
            let length = stream.read_u32().await? as usize;
            if !(8..=2048).contains(&length) {
                bail!("Tool bootstrap length exceeds bounds")
            }
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).await?;
            Self::decode(&bytes)
        })
        .await
        .context("Tool bootstrap read deadline exhausted")?
    }
}
fn descriptor() -> Descriptor {
    Descriptor {
        id: "arithmetic.local".into(),
        version: "1.0.0".into(),
        capability: "math.evaluate".into(),
    }
}
fn sql_descriptor() -> Descriptor {
    Descriptor {
        id: "sql.local".into(),
        version: "1.0.0".into(),
        capability: "sql.query".into(),
    }
}
enum Adapter {
    Arithmetic(UnixArithmetic),
    Sql(UnixSql),
}
impl Adapter {
    fn availability(&self) -> crate::fabric::Availability {
        match self {
            Self::Arithmetic(adapter) => adapter.availability(),
            Self::Sql(adapter) => adapter.availability(),
        }
    }
    async fn close(&self) {
        match self {
            Self::Arithmetic(adapter) => {
                adapter.connection.lock().await.stream.take();
            }
            Self::Sql(adapter) => adapter.close().await,
        }
    }
}
fn private_home(home: &Path) -> Result<std::fs::Metadata> {
    let metadata = std::fs::symlink_metadata(home)?;
    if !metadata.is_dir()
        || metadata.mode() & 0o777 != 0o700
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        bail!("Tool worker requires an owner-only home")
    }
    Ok(metadata)
}
struct Managed {
    adapter: Adapter,
    child: Mutex<tokio::process::Child>,
    _directory: SocketDirectory,
    _lease: Option<crate::recovery::ExecutionLease>,
}
struct Starting {
    child: tokio::process::Child,
    directory: SocketDirectory,
    lease: Option<crate::recovery::ExecutionLease>,
}
pub(crate) struct ProcessTask {
    config: Config,
    store: Arc<Store>,
    task: String,
    deadline: Instant,
    descriptor: Descriptor,
    worker: OnceCell<Managed>,
    starting: Mutex<Option<Starting>>,
    stopped: AtomicBool,
    initialization: Mutex<()>,
    ownership: Option<crate::fabric::worker_cleanup::Ownership>,
}
impl Drop for ProcessTask {
    fn drop(&mut self) {
        use crate::fabric::worker_cleanup::{detached, Pending};
        let mut pending = Vec::new();
        if let Some(worker) = self.worker.take() {
            drop(worker.adapter);
            pending.push(Pending {
                child: worker.child.into_inner(),
                socket: worker._directory,
                lease: worker._lease,
            });
        }
        if let Some(starting) = self.starting.get_mut().take() {
            pending.push(Pending {
                child: starting.child,
                socket: starting.directory,
                lease: starting.lease,
            });
        }
        if let Some(ownership) = self.ownership.take() {
            detached(
                pending,
                ownership,
                self.store.clone(),
                self.task.clone(),
                "tool_worker_process_stopped",
            );
        }
    }
}
impl ProcessTask {
    fn observed_availability(&self) -> crate::fabric::Availability {
        if self.stopped.load(Ordering::Acquire) {
            return crate::fabric::Availability::Closed;
        }
        if let Some(worker) = self.worker.get() {
            worker.adapter.availability()
        } else if Instant::now() >= self.deadline {
            crate::fabric::Availability::Expired
        } else {
            crate::fabric::Availability::Unknown
        }
    }
    pub(crate) fn new(
        config: Config,
        store: Arc<Store>,
        task: &str,
        deadline: Instant,
    ) -> Result<Self> {
        config.validate()?;
        uuid::Uuid::parse_str(task)?;
        Ok(Self {
            config,
            store,
            task: task.into(),
            deadline,
            descriptor: descriptor(),
            worker: OnceCell::new(),
            starting: Mutex::new(None),
            stopped: AtomicBool::new(false),
            initialization: Mutex::new(()),
            ownership: Some(crate::fabric::worker_cleanup::Ownership::acquire()?),
        })
    }
    pub(crate) fn new_sql(
        config: Config,
        store: Arc<Store>,
        task: &str,
        deadline: Instant,
    ) -> Result<Self> {
        let mut process = Self::new(config, store, task, deadline)?;
        process.descriptor = sql_descriptor();
        Ok(process)
    }
    async fn launch(&self) -> Result<Managed> {
        let lease = self.store.execution_lease()?;
        let running: bool = self.store.lock()?.query_row(
            "SELECT status='running' FROM tasks WHERE id=?1",
            [&self.task],
            |r| r.get(0),
        )?;
        if !running {
            bail!("Cannot launch tool worker for a terminal task")
        }
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("Tool worker task deadline exhausted")
        }
        let seconds = remaining
            .as_secs()
            .saturating_add(u64::from(remaining.subsec_nanos() != 0))
            .min(30);
        let home = std::fs::canonicalize(&self.config.home)?;
        let metadata = private_home(&home)?;
        let database: String = self.store.lock()?.query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get(0),
        )?;
        if database.is_empty()
            || std::fs::canonicalize(self.config.db())? != std::fs::canonicalize(database)?
        {
            bail!("Tool worker database differs from workspace storage")
        }
        let (guard, listener, socket) = SocketDirectory::bind_tool()?;
        guard.record(&self.store, &self.task)?;
        let mut command = tokio::process::Command::new(managed_executable()?);
        command
            .arg("--home")
            .arg(&home)
            .arg(if self.descriptor == sql_descriptor() {
                "sql-worker"
            } else {
                "arithmetic-worker"
            })
            .arg("--socket")
            .arg(&socket)
            .arg("--host-pid")
            .arg(std::process::id().to_string())
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        self.store.event(
            &self.task,
            "tool_worker_launch",
            "fabric",
            serde_json::json!({"calls_granted":1,"seconds_granted":seconds,"sandboxed":false}),
        )?;
        let mut pending = self.starting.lock().await;
        if pending.is_some() {
            bail!("An unfinished tool worker startup cannot be retried")
        }
        let child = command.spawn()?;
        let pid = child.id().context("Tool worker has no PID")?;
        *pending = Some(Starting {
            child,
            directory: guard,
            lease,
        });
        self.store.event(&self.task, "tool_worker_process", "fabric", serde_json::json!({"pid":pid,"calls_granted":1,"seconds_granted":seconds,"sandboxed":false}))?;
        let starting = pending.as_mut().context("Tool startup ownership missing")?;
        let mut stream = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(5), listener.accept()) => result.context("Tool worker connect deadline exhausted")??.0,
            status = starting.child.wait() => bail!("Tool worker exited before setup: {:?}", status?),
        };
        authenticate(&stream, metadata.uid(), pid)?;
        let setup = Setup {
            grant: WorkerGrant::new(&self.task, self.descriptor.clone(), 1, seconds)?,
            database_bytes: self.config.max_database_bytes,
        };
        setup.send(&mut stream).await?;
        let adapter = if self.descriptor == sql_descriptor() {
            Adapter::Sql(UnixSql::connected(
                stream,
                setup.grant.sql()?,
                metadata.uid(),
                pid,
            )?)
        } else {
            Adapter::Arithmetic(UnixArithmetic::connected(
                stream,
                setup.grant.arithmetic()?,
                metadata.uid(),
                pid,
            )?)
        };
        self.store.event(
            &self.task,
            "tool_worker_ready",
            "fabric",
            serde_json::json!({"pid":pid,"bootstrap_delivered":true}),
        )?;
        drop(listener);
        let starting = pending.take().context("Tool startup ownership missing")?;
        Ok(Managed {
            adapter,
            child: Mutex::new(starting.child),
            _directory: starting.directory,
            _lease: starting.lease,
        })
    }
    pub(crate) async fn shutdown(&self) -> Result<()> {
        self.stopped.store(true, Ordering::Release);
        let _initialization = self.initialization.lock().await;
        if let Some(worker) = self.worker.get() {
            worker.adapter.close().await;
            let mut child = worker.child.lock().await;
            let result = self.reap(&mut child).await;
            if result.is_err() {
                crate::fabric::worker_cleanup::record_failure();
            }
            return result;
        }
        let mut pending = self.starting.lock().await;
        if let Some(starting) = pending.as_mut() {
            if let Err(error) = self.reap(&mut starting.child).await {
                crate::fabric::worker_cleanup::record_failure();
                return Err(error);
            }
            pending.take();
        }
        Ok(())
    }
    async fn reap(&self, child: &mut tokio::process::Child) -> Result<()> {
        let pid = child.id();
        if pid.is_none() {
            return Ok(());
        }
        let mut forced = false;
        let status = match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                forced = true;
                child.start_kill()?;
                tokio::time::timeout(Duration::from_secs(2), child.wait())
                    .await
                    .context("Tool worker reap deadline exhausted")??
            }
        };
        self.store.event(
            &self.task,
            "tool_worker_process_stopped",
            "fabric",
            serde_json::json!({"pid":pid,"forced":forced,"exit_code":status.code(),"reaped":true}),
        )
    }
}
impl ArithmeticBackend for ProcessTask {
    fn availability(&self) -> crate::fabric::Availability {
        self.observed_availability()
    }
    fn module_descriptor(&self) -> Option<&Descriptor> {
        Some(&self.descriptor)
    }
    fn evaluate<'a>(
        &'a self,
        expression: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        self.evaluate_for(&self.task, expression)
    }
    fn evaluate_for<'a>(
        &'a self,
        task: &'a str,
        expression: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            if self.descriptor != descriptor()
                || task != self.task
                || expression.is_empty()
                || expression.len() > 120
            {
                bail!("Tool process request exceeds task/input scope")
            }
            let initialization = self.initialization.lock().await;
            if self.stopped.load(Ordering::Acquire) {
                bail!("Tool worker has been shut down")
            }
            let worker = self.worker.get_or_try_init(|| self.launch()).await?;
            drop(initialization);
            if self.stopped.load(Ordering::Acquire) {
                bail!("Tool worker has been shut down")
            }
            match &worker.adapter {
                Adapter::Arithmetic(adapter) => adapter.evaluate_for(task, expression).await,
                Adapter::Sql(_) => bail!("SQL worker cannot execute arithmetic"),
            }
        })
    }
}

impl SqlBackend for ProcessTask {
    fn availability(&self) -> crate::fabric::Availability {
        self.observed_availability()
    }
    fn module_descriptor(&self) -> Option<&Descriptor> {
        Some(&self.descriptor)
    }
    fn query<'a>(
        &'a self,
        task: &'a str,
        input: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            if self.descriptor != sql_descriptor() || task != self.task {
                bail!("SQL process request exceeds task/capability scope")
            }
            crate::sql::parse_task(input)?;
            let initialization = self.initialization.lock().await;
            if self.stopped.load(Ordering::Acquire) {
                bail!("Tool worker has been shut down")
            }
            let worker = self.worker.get_or_try_init(|| self.launch()).await?;
            drop(initialization);
            if self.stopped.load(Ordering::Acquire) {
                bail!("Tool worker has been shut down")
            }
            match &worker.adapter {
                Adapter::Sql(adapter) => adapter.query(task, input).await,
                Adapter::Arithmetic(_) => bail!("Arithmetic worker cannot execute SQL"),
            }
        })
    }
}

pub async fn run_worker(home: &Path, socket: &Path, host_pid: u32, sql: bool) -> Result<()> {
    let metadata = private_home(home)?;
    let mut stream =
        tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(socket)).await??;
    authenticate(&stream, metadata.uid(), host_pid)?;
    let setup = Setup::receive(&mut stream).await?;
    if sql != (setup.grant.descriptor == sql_descriptor()) {
        bail!("Worker entry point differs from granted capability")
    }
    let store = Arc::new(Store::open_with_limit(
        &home.join("fabric.sqlite3"),
        setup.database_bytes,
    )?);
    let running: bool = store.lock()?.query_row(
        "SELECT status='running' FROM tasks WHERE id=?1",
        [&setup.grant.task],
        |r| r.get(0),
    )?;
    if !running {
        bail!("Tool worker task is no longer running")
    }
    let (_stop, receive) = watch::channel(false);
    if sql {
        crate::fabric::tools::sql::unix::serve(
            stream,
            setup.grant.sql()?,
            metadata.uid(),
            host_pid,
            Arc::new(LocalSql),
            store,
            receive,
        )
        .await
    } else {
        super::serve(
            stream,
            setup.grant.arithmetic()?,
            metadata.uid(),
            host_pid,
            Arc::new(LocalMath),
            store,
            receive,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn shutdown_before_initialization_refuses_both_tool_launches() {
        for sql in [false, true] {
            let store = Arc::new(Store::in_memory().unwrap());
            let task = store.create_task("Shutdown before launch", "fast").unwrap();
            let deadline = Instant::now() + Duration::from_secs(30);
            let process = if sql {
                ProcessTask::new_sql(Config::default(), store.clone(), &task, deadline)
            } else {
                ProcessTask::new(Config::default(), store.clone(), &task, deadline)
            }
            .unwrap();
            process.shutdown().await.unwrap();
            process.shutdown().await.unwrap();
            assert_eq!(
                process.observed_availability(),
                crate::fabric::Availability::Closed
            );
            let result = if sql {
                process
                    .query(&task, r#"{"tables":[],"query":"SELECT 42 AS answer"}"#)
                    .await
            } else {
                process.evaluate_for(&task, "6*7").await
            };
            assert!(result.unwrap_err().to_string().contains("shut down"));
            assert!(process.worker.get().is_none());
            assert!(process.starting.lock().await.is_none());
            assert!(!store.task(&task).unwrap()["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "tool_worker_process_launch"));
        }
    }

    #[tokio::test]
    async fn managed_tool_states_follow_adapters_and_unlaunched_deadlines() {
        for sql in [false, true] {
            let store = Arc::new(Store::in_memory().unwrap());
            let task = store.create_task("Managed availability", "fast").unwrap();
            let make = |deadline| {
                if sql {
                    ProcessTask::new_sql(Config::default(), store.clone(), &task, deadline)
                } else {
                    ProcessTask::new(Config::default(), store.clone(), &task, deadline)
                }
            };
            let expired = make(Instant::now() - Duration::from_secs(1)).unwrap();
            assert_eq!(
                expired.observed_availability(),
                crate::fabric::Availability::Expired
            );
            assert!(expired.worker.get().is_none());
            let process = Arc::new(make(Instant::now() + Duration::from_secs(30)).unwrap());
            assert_eq!(
                process.observed_availability(),
                crate::fabric::Availability::Unknown
            );
            let (client, _server) = UnixStream::pair().unwrap();
            let uid = unsafe { libc::geteuid() };
            let adapter = if sql {
                Adapter::Sql(
                    UnixSql::connected(
                        client,
                        SqlGrant::new(&task, sql_descriptor(), 1, 30).unwrap(),
                        uid,
                        std::process::id(),
                    )
                    .unwrap(),
                )
            } else {
                Adapter::Arithmetic(
                    UnixArithmetic::connected(
                        client,
                        Grant::new(&task, descriptor(), 1, 30).unwrap(),
                        uid,
                        std::process::id(),
                    )
                    .unwrap(),
                )
            };
            let (directory, _listener, _path) = SocketDirectory::bind_tool().unwrap();
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
                    _directory: directory,
                    _lease: None
                })
                .is_ok());
            assert_eq!(
                ArithmeticBackend::availability(process.as_ref()),
                crate::fabric::Availability::Connected
            );
            assert_eq!(
                SqlBackend::availability(process.as_ref()),
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
            assert_eq!(
                process.observed_availability(),
                crate::fabric::Availability::Closed
            );
        }
    }

    #[tokio::test]
    async fn dropped_startup_retains_socket_until_child_is_reaped() {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("Dropped worker owner", "fast").unwrap();
        let process = ProcessTask::new(
            Config::default(),
            store.clone(),
            &task,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let (socket, listener, path) = SocketDirectory::bind_tool().unwrap();
        let directory = path.parent().unwrap().to_path_buf();
        let child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fabric::tools::unix::process::tests::startup_child_fixture",
            ])
            .env("COGFABRIC_TEST_TOOL_STARTUP_HOLD", "1")
            .stdout(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        *process.starting.lock().await = Some(Starting {
            child,
            directory: socket,
            lease: None,
        });
        drop(listener);
        drop(process);
        assert!(directory.exists());
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let trace = store.task(&task).unwrap();
                if trace["events"].as_array().unwrap().iter().any(|event| {
                    event["kind"] == "tool_worker_process_stopped"
                        && event["payload"]["pid"] == pid
                        && event["payload"]["reaped"] == true
                        && event["payload"]["dropped_owner"] == true
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!directory.exists());
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    }
    #[test]
    fn startup_child_fixture() {
        if std::env::var_os("COGFABRIC_TEST_TOOL_STARTUP_HOLD").is_some() {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    #[tokio::test]
    async fn unfinished_startup_is_owned_reaped_and_socket_directory_removed() {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store
            .create_task("unfinished tool startup", "fast")
            .unwrap();
        let process = ProcessTask::new(
            Config::default(),
            store.clone(),
            &task,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
        let (directory, listener, socket) = SocketDirectory::bind_tool().unwrap();
        let path = socket.parent().unwrap().to_path_buf();
        let child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fabric::tools::unix::process::tests::startup_child_fixture",
            ])
            .env("COGFABRIC_TEST_TOOL_STARTUP_HOLD", "1")
            .stdout(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        *process.starting.lock().await = Some(Starting {
            child,
            directory,
            lease: None,
        });
        drop(listener);
        process.shutdown().await.unwrap();
        assert!(process.starting.lock().await.is_none());
        assert!(!path.exists());
        let trace = store.task(&task).unwrap();
        assert!(trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "tool_worker_process_stopped"
                && e["payload"]["reaped"] == true
                && e["payload"]["forced"] == true));
    }
    #[test]
    fn bootstrap_round_trip_and_invalid_grants() {
        let mut setup = Setup {
            grant: WorkerGrant::new(&uuid::Uuid::new_v4().to_string(), descriptor(), 1, 30)
                .unwrap(),
            database_bytes: 256_000_000,
        };
        let bytes = setup.encode().unwrap();
        assert_eq!(Setup::decode(&bytes).unwrap().grant.task, setup.grant.task);
        assert!(Setup::decode(&[0; 16]).is_err());
        assert!(Setup::decode(&vec![0; 2049]).is_err());
        setup.grant.calls = 2;
        assert!(setup.encode().is_err());
        setup.grant.calls = 1;
        setup.grant.descriptor.version = "2.0.0".into();
        assert!(setup.encode().is_err());
        setup.grant.descriptor = descriptor();
        setup.database_bytes = 1;
        assert!(setup.encode().is_err());
    }
    #[test]
    fn sql_bootstrap_is_versioned_and_cannot_cross_capabilities() {
        let task = uuid::Uuid::new_v4().to_string();
        let setup = Setup {
            grant: WorkerGrant::new(&task, sql_descriptor(), 1, 30).unwrap(),
            database_bytes: 256_000_000,
        };
        let bytes = setup.encode().unwrap();
        assert_eq!(fb::root_as_bootstrap(&bytes).unwrap().version(), 2);
        assert_eq!(
            Setup::decode(&bytes).unwrap().grant.descriptor,
            sql_descriptor()
        );
        assert!(setup.grant.arithmetic().is_err());
        for (version, descriptor) in [
            (1, sql_descriptor()),
            (2, descriptor()),
            (3, sql_descriptor()),
        ] {
            let mut b = flatbuffers::FlatBufferBuilder::new();
            let args = fb::BootstrapArgs {
                version,
                task_id: Some(b.create_string(&task)),
                module_id: Some(b.create_string(&descriptor.id)),
                module_version: Some(b.create_string(&descriptor.version)),
                capability: Some(b.create_string(&descriptor.capability)),
                calls: 1,
                seconds: 30,
                max_database_bytes: 256_000_000,
            };
            let root = fb::Bootstrap::create(&mut b, &args);
            fb::finish_bootstrap_buffer(&mut b, root);
            assert!(Setup::decode(b.finished_data()).is_err());
        }
    }
    #[tokio::test]
    async fn sql_scope_and_custom_registry_are_denied_before_launch() {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("SQL worker scope", "fast").unwrap();
        let process = ProcessTask::new_sql(
            Config::default(),
            store.clone(),
            &task,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
        assert!(process.evaluate("6*7").await.is_err());
        assert!(process
            .query(
                &uuid::Uuid::new_v4().to_string(),
                r#"{"tables":[],"query":"SELECT 1"}"#
            )
            .await
            .is_err());
        assert!(process.worker.get().is_none());
        let engine = crate::engine::Engine::new(
            Config {
                sql_process: true,
                ..Config::default()
            },
            store.clone(),
        );
        let mut replacement = sql_descriptor();
        replacement.version = "2.0.0".into();
        engine
            .tools
            .register_sql(replacement, Arc::new(LocalSql))
            .unwrap();
        assert!(engine
            .run(r#"sql:{"tables":[],"query":"SELECT 1"}"#, "fast")
            .await
            .unwrap_err()
            .to_string()
            .contains("default tool registry"));
        let tasks: u32 = store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tasks, 1);
    }
    #[tokio::test]
    async fn denied_scope_and_terminal_task_never_launch() {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("tool scope", "fast").unwrap();
        let process = ProcessTask::new(
            Config::default(),
            store.clone(),
            &task,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
        assert!(process
            .evaluate_for(&uuid::Uuid::new_v4().to_string(), "6*7")
            .await
            .is_err());
        assert!(process.worker.get().is_none());
        store
            .finish_task(&task, "interrupted", "No replay")
            .unwrap();
        assert!(process
            .evaluate("6*7")
            .await
            .unwrap_err()
            .to_string()
            .contains("terminal"));
        assert!(process.worker.get().is_none());
    }
    #[tokio::test]
    async fn managed_mode_refuses_custom_tool_registration_before_task_creation() {
        let store = Arc::new(Store::in_memory().unwrap());
        let engine = crate::engine::Engine::new(
            Config {
                arithmetic_process: true,
                ..Config::default()
            },
            store.clone(),
        );
        let mut replaced = descriptor();
        replaced.version = "2.0.0".into();
        engine
            .tools
            .register(replaced, Arc::new(LocalMath))
            .unwrap();
        assert!(engine
            .run("calc: 6*7", "fast")
            .await
            .unwrap_err()
            .to_string()
            .contains("default tool registry"));
        let tasks: u32 = store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tasks, 0);
    }
}
