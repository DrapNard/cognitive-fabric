//! Kernel-released execution leases distinguish live engines from abandoned tasks.
mod advisory;
pub mod continuation;
pub(crate) mod graph;
mod inference;
mod result;
mod revision;
pub mod worker_files;
use crate::{security, store::Store};
use anyhow::{bail, Result};
use rusqlite::params;
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
};

pub(crate) struct ExecutionLease(#[allow(dead_code)] File);
impl ExecutionLease {
    /// Convert only while holding the admission SQLite write transaction: flock
    /// conversion may briefly release ownership. Failure must roll back admission.
    pub(crate) fn into_shared(self) -> Result<Self> {
        if unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(self)
    }

    pub(crate) fn acquire(path: &Path, exclusive: bool) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        let metadata = file.metadata()?;
        // The lease inode must remain stable; it is deliberately never unlinked.
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            bail!("Runtime lease must be a private owner-controlled regular file")
        }
        let operation = if exclusive {
            libc::LOCK_EX
        } else {
            libc::LOCK_SH
        } | libc::LOCK_NB;
        // flock applies to this open file description and is released on process death.
        if unsafe { libc::flock(file.as_raw_fd(), operation) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(file))
    }
}
impl Store {
    /// Reconcile open worker audit sessions only under exclusive runtime ownership.
    /// This never kills a PID, replays inference or claims external GPU work stopped.
    pub fn recover_workers(&self) -> Result<Vec<RecoveredWorker>> {
        security::authorize("human-cli", "worker.recover")?;
        let path = self
            .runtime_lock
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Worker recovery requires a persistent store"))?;
        let _exclusive = ExecutionLease::acquire(path, true).map_err(|error| {
            error.context("Cannot acquire exclusive runtime lease for worker recovery")
        })?;
        let mut db = self.lock()?;
        let started = std::time::Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > std::time::Duration::from_secs(5)),
        );
        let result = (|| -> Result<Vec<RecoveredWorker>> {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let rows = tx.prepare(
                "SELECT e.task_id,e.kind,
                    CASE WHEN length(CAST(e.payload AS BLOB))<=4096 THEN e.payload ELSE NULL END
                 FROM events e JOIN tasks t ON t.id=e.task_id
                 WHERE ((e.kind='worker.audit' AND json_extract(e.payload,'$.state')='session_started')
                     OR (e.kind='tool_worker' AND json_extract(e.payload,'$.state')='started'))
                 AND NOT EXISTS (
                    SELECT 1 FROM events f WHERE f.task_id=e.task_id AND f.kind=e.kind
                    AND json_extract(f.payload,'$.session_id')=json_extract(e.payload,'$.session_id')
                    AND ((e.kind='worker.audit' AND json_extract(f.payload,'$.state')
                            IN ('session_stopped','session_failed','session_interrupted'))
                        OR (e.kind='tool_worker' AND json_extract(f.payload,'$.state')
                            IN ('closed','failed','interrupted'))))
                 ORDER BY e.id LIMIT 10001",
            )?.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if rows.len() > 10000 {
                bail!("Worker recovery exceeds 10,000 sessions; inspect offline")
            }
            let mut recovered = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for (anchor, kind, raw) in rows {
                if started.elapsed() > std::time::Duration::from_secs(5) {
                    bail!("Worker recovery deadline exhausted")
                }
                uuid::Uuid::parse_str(&anchor)?;
                let raw =
                    raw.ok_or_else(|| anyhow::anyhow!("Worker recovery metadata exceeds bounds"))?;
                let payload: serde_json::Value = serde_json::from_str(&raw)?;
                let session = payload["session_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing worker session identity"))?;
                uuid::Uuid::parse_str(session)?;
                let tool = kind == "tool_worker";
                // Tool v1 initially omitted the metadata version; retain recovery
                // compatibility without admitting unknown explicit versions.
                if ((!tool || payload.get("format_version").is_some())
                    && payload["format_version"] != 1)
                    || !seen.insert(session.to_string())
                {
                    bail!("Invalid or duplicate worker recovery session")
                }
                let module = payload["module_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing worker module identity"))?;
                let version = payload["module_version"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing worker module version"))?;
                let capability = if tool {
                    let capability = payload["capability"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing worker capability"))?;
                    crate::fabric::tools::Descriptor {
                        id: module.into(),
                        version: version.into(),
                        capability: capability.into(),
                    }
                    .validate()?;
                    if payload["charged_calls"] != 0 {
                        bail!("Invalid initial tool worker charge")
                    }
                    Some(capability)
                } else {
                    crate::fabric::ModuleDescriptor {
                        id: module.into(),
                        version: version.into(),
                        roles: vec!["solver".into()],
                    }
                    .validate()?;
                    None
                };
                let record = serde_json::json!({"format_version":1,"session_id":session,"module_id":module,"module_version":version,"capability":capability,"state":if tool {"interrupted"} else {"session_interrupted"},"reason":"exclusive_runtime_lease","automatic_replay":false,"external_inference_stopped_verified":false,"external_effects_stopped_verified":false,"resource_usage_complete":false});
                tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,?2,'human-cli',?3,unixepoch())", params![anchor,kind,record.to_string()])?;
                recovered.push(RecoveredWorker {
                    task_id: anchor,
                    session_id: session.into(),
                });
            }
            if started.elapsed() > std::time::Duration::from_secs(5) {
                bail!("Worker recovery deadline exhausted")
            }
            tx.commit()?;
            Ok(recovered)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        result
    }
    pub(crate) fn execution_lease(&self) -> Result<Option<ExecutionLease>> {
        self.runtime_lock
            .as_deref()
            .map(|path| ExecutionLease::acquire(path, false))
            .transpose()
    }

    /// Explicit recovery never reruns tools or assumes an interrupted model call succeeded.
    pub fn recover_tasks(&self) -> Result<Vec<String>> {
        security::authorize("human-cli", "task.recover")?;
        let path = self
            .runtime_lock
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Recovery requires a persistent store"))?;
        let _exclusive = ExecutionLease::acquire(path, true).map_err(|error| {
            error.context("Cannot acquire exclusive runtime lease for task recovery")
        })?;
        let mut db = self.lock()?;
        let started = std::time::Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > std::time::Duration::from_secs(5)),
        );
        let result = (|| -> Result<Vec<String>> {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let ids = tx
            .prepare("SELECT CASE WHEN length(CAST(id AS BLOB))<=64 THEN id END FROM tasks WHERE status='running' ORDER BY created,id LIMIT 10001")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
            if ids.len() > 10000 {
                bail!("Recovery exceeds 10,000 tasks; inspect state offline")
            }
            for id in &ids {
                uuid::Uuid::parse_str(id)?;
            }
            let mut progress = Vec::new();
            for id in &ids {
                if started.elapsed() > std::time::Duration::from_secs(5) {
                    bail!("Task recovery deadline exhausted")
                }
                tx.execute("UPDATE tasks SET status='interrupted',answer='Task interrupted before a durable terminal result; no automatic replay',updated=unixepoch() WHERE id=?1 AND status='running'", [id])?;
                tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'recovery','human-cli',?2,unixepoch())", params![id,serde_json::json!({"status":"interrupted","reason":"exclusive_runtime_lease","automatic_replay":false,"resource_usage_complete":false,"subtask_results_verified":false}).to_string()])?;
                progress.push(
                    crate::events::Progress::from_audit(
                        tx.last_insert_rowid() as u64,
                        id,
                        "recovery",
                        "human-cli",
                        &serde_json::json!({"status":"interrupted"}),
                    )?
                    .expect("Recovery is a typed event"),
                );
            }
            if started.elapsed() > std::time::Duration::from_secs(5) {
                bail!("Task recovery deadline exhausted")
            }
            tx.commit()?;
            for event in &progress {
                self.events.publish(event)?;
            }
            Ok(ids)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        result
    }
}

#[derive(Debug, serde::Serialize)]
pub struct RecoveredWorker {
    pub task_id: String,
    pub session_id: String,
}

#[cfg(test)]
#[path = "recovery/tool_tests.rs"]
mod tool_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn duplicated_lease_descriptor_blocks_recovery_until_its_last_owner_closes() {
        use std::os::fd::AsRawFd;
        let root =
            std::env::temp_dir().join(format!("fabric-duplicated-lease-{}", uuid::Uuid::new_v4()));
        let store = super::Store::open(&root.join("state.db")).unwrap();
        let task = store
            .create_task("Retained descriptor recovery", "fast")
            .unwrap();
        worker_start(&store, &task, &uuid::Uuid::new_v4().to_string());
        let lease = store.execution_lease().unwrap().unwrap();
        let duplicate = lease.0.try_clone().unwrap();
        for descriptor in [lease.0.as_raw_fd(), duplicate.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
            assert!(flags >= 0);
            assert_ne!(flags & libc::FD_CLOEXEC, 0);
        }
        drop(lease);
        assert!(store.recover_workers().is_err());
        let interrupted: u32 = store.lock().unwrap().query_row(
            "SELECT count(*) FROM events WHERE kind='worker.audit' AND json_extract(payload,'$.state')='session_interrupted'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(interrupted, 0);
        drop(duplicate);
        assert_eq!(store.recover_workers().unwrap().len(), 1);
        assert!(store.recover_workers().unwrap().is_empty());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn admission_lease_conversion_allows_workers_but_blocks_recovery() {
        let path =
            std::env::temp_dir().join(format!("fabric-lease-conversion-{}", uuid::Uuid::new_v4()));
        let exclusive = super::ExecutionLease::acquire(&path, true).unwrap();
        assert!(super::ExecutionLease::acquire(&path, false).is_err());
        let shared = exclusive.into_shared().unwrap();
        let worker = super::ExecutionLease::acquire(&path, false).unwrap();
        assert!(super::ExecutionLease::acquire(&path, true).is_err());
        drop(worker);
        drop(shared);
        assert!(super::ExecutionLease::acquire(&path, true).is_ok());
        std::fs::remove_file(path).unwrap();
    }

    use super::*;
    fn worker_start(store: &Store, task: &str, session: &str) {
        store.worker_audit(task, serde_json::json!({"format_version":1,"session_id":session,"module_id":"inference.local","module_version":"1.0.0","state":"session_started"})).unwrap();
    }
    #[test]
    fn worker_reconciliation_preserves_task_results_and_is_idempotent() {
        let root =
            std::env::temp_dir().join(format!("fabric-worker-recovery-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&root.join("state.db")).unwrap();
        let running = store.create_task("unfinished", "fast").unwrap();
        let completed = store.create_task("finished", "fast").unwrap();
        store.finish_task(&completed, "completed", "42").unwrap();
        let first = uuid::Uuid::new_v4().to_string();
        let second = uuid::Uuid::new_v4().to_string();
        let closed = uuid::Uuid::new_v4().to_string();
        worker_start(&store, &running, &first);
        worker_start(&store, &completed, &second);
        worker_start(&store, &completed, &closed);
        store.worker_audit(&completed, serde_json::json!({"format_version":1,"session_id":closed,"state":"session_stopped"})).unwrap();
        let lease = store.execution_lease().unwrap();
        assert!(store.recover_workers().is_err());
        drop(lease);
        let recovered = store.recover_workers().unwrap();
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0].session_id, first);
        assert_eq!(recovered[1].session_id, second);
        assert_eq!(store.task(&running).unwrap()["task"]["status"], "running");
        assert_eq!(store.task(&completed).unwrap()["task"]["answer"], "42");
        assert!(store.recover_workers().unwrap().is_empty());
        let trace = store.task(&completed).unwrap();
        assert_eq!(
            trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["payload"]["state"] == "session_interrupted")
                .count(),
            1
        );
        assert!(security::authorize("jev-r", "worker.recover").is_err());
        assert!(Store::in_memory().unwrap().recover_workers().is_err());
        drop(store);
        let reopened = Store::open(&root.join("state.db")).unwrap();
        assert!(reopened.recover_workers().unwrap().is_empty());
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn worker_reconciliation_refuses_oversized_batches_without_partial_changes() {
        let root = std::env::temp_dir().join(format!(
            "fabric-worker-recovery-limit-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Store::open(&root.join("state.db")).unwrap();
        store.lock().unwrap().execute_batch("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10001) INSERT INTO tasks(id,query,mode,status,created,updated) SELECT printf('%032x',x),'fixture','fast','completed',0,0 FROM n; INSERT INTO events(task_id,kind,source,payload,created) SELECT id,'worker.audit','fabric',json_object('format_version',1,'session_id',id,'module_id','inference.local','module_version','1.0.0','state','session_started'),0 FROM tasks;").unwrap();
        let error = store.recover_workers().unwrap_err().to_string();
        assert!(error.contains("10,000"), "{error}");
        let count: u32 = store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 10001);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn worker_reconciliation_rolls_back_on_audit_or_metadata_failure() {
        let root = std::env::temp_dir().join(format!(
            "fabric-worker-recovery-negative-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Store::open(&root.join("state.db")).unwrap();
        let task = store.create_task("unfinished", "fast").unwrap();
        let first = uuid::Uuid::new_v4().to_string();
        worker_start(&store, &task, &first);
        store.lock().unwrap().execute_batch("CREATE TRIGGER fail_worker_recovery BEFORE INSERT ON events WHEN json_extract(NEW.payload,'$.state')='session_interrupted' BEGIN SELECT RAISE(ABORT,'fixture recovery failure'); END;").unwrap();
        assert!(store.recover_workers().is_err());
        assert_eq!(
            store.task(&task).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_worker_recovery")
            .unwrap();
        worker_start(&store, &task, "malformed-session");
        assert!(store.recover_workers().is_err());
        assert_eq!(
            store.task(&task).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        // The progress callback is removed on failure; normal statements still work.
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row("SELECT 1", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            1
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn live_worker_lease_blocks_recovery_until_its_future_is_dropped() {
        use crate::{
            backend::InferenceBackend,
            fabric::{
                unix::{
                    worker::{self, Audit, Authority, Limits},
                    UnixInference,
                },
                ModuleDescriptor,
            },
        };
        let root = std::env::temp_dir().join(format!(
            "fabric-live-worker-recovery-{}",
            uuid::Uuid::new_v4()
        ));
        let store = std::sync::Arc::new(Store::open(&root.join("state.db")).unwrap());
        let task = store.create_task("unfinished", "fast").unwrap();
        let (host, socket) = tokio::net::UnixStream::pair().unwrap();
        let uid = socket.peer_cred().unwrap().uid();
        let descriptor = ModuleDescriptor {
            id: "inference.local".into(),
            version: "1.0.0".into(),
            roles: vec!["solver".into()],
        };
        let authority = Authority::new(
            descriptor.clone(),
            uid,
            std::process::id(),
            std::slice::from_ref(&task),
            Limits::default(),
        )
        .unwrap();
        let adapter = UnixInference::connected(host, descriptor, uid, std::process::id()).unwrap();
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        let (_shutdown, receiver) = tokio::sync::watch::channel(false);
        let worker = tokio::spawn(worker::serve(
            socket,
            authority,
            std::sync::Arc::new(PendingBackend(ready.clone())),
            receiver,
            Audit::new(store.clone(), &task).unwrap(),
        ));
        let requested = task.clone();
        let caller = tokio::spawn(async move {
            adapter
                .generate_for(&requested, "solver", "goal", 128)
                .await
        });
        ready.notified().await;
        assert!(store.recover_workers().is_err());
        worker.abort();
        assert!(worker.await.unwrap_err().is_cancelled());
        assert!(caller.await.unwrap().is_err());
        let reconciled = store.recover_workers().unwrap();
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].task_id, task);
        assert_eq!(store.task(&task).unwrap()["task"]["status"], "running");
        assert_eq!(store.recover_tasks().unwrap(), vec![task]);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    struct PendingBackend(std::sync::Arc<tokio::sync::Notify>);
    impl crate::backend::InferenceBackend for PendingBackend {
        fn generate<'a>(
            &'a self,
            _role: &'a str,
            _prompt: &'a str,
            _max_tokens: usize,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<crate::backend::Generation>> + Send + 'a>,
        > {
            Box::pin(async move {
                self.0.notify_one();
                std::future::pending().await
            })
        }
    }
    #[tokio::test]
    async fn engine_holds_lease_until_execution_future_ends() {
        let root = std::env::temp_dir().join(format!(
            "cogfabric-engine-recovery-{}",
            uuid::Uuid::new_v4()
        ));
        let store = std::sync::Arc::new(Store::open(&root.join("state.db")).unwrap());
        crate::store::bootstrap(&store).unwrap();
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        let cfg = crate::config::Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            ..Default::default()
        };
        let engine = crate::engine::Engine::with_backend(
            cfg,
            store.clone(),
            std::sync::Arc::new(PendingBackend(ready.clone())),
        );
        let task = tokio::spawn(async move { engine.run("Explain recovery", "fast").await });
        tokio::time::timeout(std::time::Duration::from_secs(2), ready.notified())
            .await
            .unwrap();
        assert!(store.recover_tasks().is_err());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let recovered = store.recover_tasks().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(
            store.task(&recovered[0]).unwrap()["task"]["status"],
            "interrupted"
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn live_leases_block_recovery_and_abandoned_tasks_are_audited() {
        let root =
            std::env::temp_dir().join(format!("cogfabric-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let store = Store::open(&root.join("state.db")).unwrap();
        let other = Store::open(&root.join("state.db")).unwrap();
        let abandoned = store.create_task("unfinished", "standard").unwrap();
        let mut observer = other.subscribe_progress(&abandoned).unwrap();
        let completed = store.create_task("calc: 2 + 3", "fast").unwrap();
        store.finish_task(&completed, "completed", "5").unwrap();
        let first = store.execution_lease().unwrap();
        let second = other.execution_lease().unwrap();
        assert!(other.recover_tasks().is_err());
        drop(first);
        assert!(store.recover_tasks().is_err());
        drop(second);
        assert_eq!(other.recover_tasks().unwrap(), vec![abandoned.clone()]);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let event = runtime.block_on(observer.receive()).unwrap().unwrap();
        assert_eq!(event.stage, crate::events::Stage::Recovery);
        assert_eq!(event.state, "interrupted");
        assert_eq!(
            store.progress_since(&abandoned, 0).unwrap().events[0].sequence,
            event.sequence
        );
        assert_eq!(
            store.task(&abandoned).unwrap()["task"]["status"],
            "interrupted"
        );
        assert_eq!(
            store.task(&abandoned).unwrap()["events"][0]["kind"],
            "recovery"
        );
        assert_eq!(store.task(&completed).unwrap()["task"]["answer"], "5");
        assert!(store.recover_tasks().unwrap().is_empty());
        assert!(store.execution_lease().is_ok());
        assert!(security::authorize("jev-plus", "task.recover").is_err());
        assert!(Store::in_memory().unwrap().recover_tasks().is_err());
        drop(other);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn task_recovery_rejects_invalid_ids_and_oversized_batches_atomically() {
        let root = std::env::temp_dir().join(format!(
            "fabric-task-recovery-bounds-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Store::open(&root.join("state.db")).unwrap();
        let valid = store.create_task("unfinished", "fast").unwrap();
        for invalid in ["invalid".to_string(), "x".repeat(1024)] {
            store.lock().unwrap().execute("INSERT INTO tasks(id,query,mode,status,created,updated) VALUES (?1,'fixture','fast','running',0,0)", [&invalid]).unwrap();
            assert!(store.recover_tasks().is_err());
            assert_eq!(store.task(&valid).unwrap()["task"]["status"], "running");
            store
                .lock()
                .unwrap()
                .execute("DELETE FROM tasks WHERE id=?1", [&invalid])
                .unwrap();
        }
        store.lock().unwrap().execute_batch("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10000) INSERT INTO tasks(id,query,mode,status,created,updated) SELECT printf('%032x',x),'fixture','fast','running',0,0 FROM n;").unwrap();
        assert!(store
            .recover_tasks()
            .unwrap_err()
            .to_string()
            .contains("10,000"));
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM tasks WHERE status='running'",
                    [],
                    |r| r.get::<_, u32>(0)
                )
                .unwrap(),
            10001
        );
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM events WHERE kind='recovery'",
                    [],
                    |r| r.get::<_, u32>(0)
                )
                .unwrap(),
            0
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn task_recovery_deadline_rolls_back_and_removes_progress_callback() {
        let root = std::env::temp_dir().join(format!(
            "fabric-task-recovery-deadline-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Store::open(&root.join("state.db")).unwrap();
        let task = store.create_task("unfinished", "fast").unwrap();
        store.lock().unwrap().execute_batch("CREATE TRIGGER slow_recovery BEFORE UPDATE ON tasks BEGIN SELECT (WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n); END;").unwrap();
        assert!(store.recover_tasks().is_err());
        // A callback left installed would interrupt this statement after 1,000 opcodes.
        let sum: u64 = store.lock().unwrap().query_row("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10000) SELECT sum(x) FROM n", [], |r| r.get(0)).unwrap();
        assert_eq!(sum, 50005000);
        assert_eq!(store.task(&task).unwrap()["task"]["status"], "running");
        assert_eq!(
            store.task(&task).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER slow_recovery")
            .unwrap();
        assert_eq!(store.recover_tasks().unwrap(), vec![task]);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_transactions_roll_back_and_lease_substitutions_fail() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = std::env::temp_dir().join(format!(
            "cogfabric-recovery-negative-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&root).unwrap();
        let store = Store::open(&root.join("state.db")).unwrap();
        let task = store.create_task("unfinished", "deep").unwrap();
        store.lock().unwrap().execute_batch("CREATE TRIGGER fail_recovery BEFORE INSERT ON events WHEN NEW.kind='recovery' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(store.recover_tasks().is_err());
        assert_eq!(store.task(&task).unwrap()["task"]["status"], "running");
        let lease = store.runtime_lock.as_ref().unwrap();
        std::fs::set_permissions(lease, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.execution_lease().is_err());
        assert!(store.recover_tasks().is_err());
        std::fs::remove_file(lease).unwrap();
        symlink(root.join("state.db"), lease).unwrap();
        assert!(store.execution_lease().is_err());
        assert!(store.recover_tasks().is_err());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
#[path = "recovery/lease_tests.rs"]
mod lease_tests;
