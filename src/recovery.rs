//! Kernel-released execution leases distinguish live engines from abandoned tasks.
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
    fn acquire(path: &Path, exclusive: bool) -> Result<Self> {
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
            anyhow::anyhow!("Cannot recover while an engine holds a lease: {error}")
        })?;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let ids = tx
            .prepare("SELECT id FROM tasks WHERE status='running' ORDER BY created,id LIMIT 10001")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > 10000 {
            bail!("Recovery exceeds 10,000 tasks; inspect state offline")
        }
        let mut progress = Vec::new();
        for id in &ids {
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
        tx.commit()?;
        for event in &progress {
            self.events.publish(event)?;
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
