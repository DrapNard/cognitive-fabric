//! Operator-owned experimental module audit contexts. No guest executor or model grants.
use crate::{
    config::Config, execution_clock::ExecutionStart, recovery::ExecutionLease, store::Store,
};
use anyhow::{bail, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub version: String,
    pub checksum: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Allocated,
    DispatchReserved,
    Returned,
    NotReturned,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format_version: u32,
    artifact: Identity,
    slot: usize,
    phase: Phase,
}
/// Trusted Rust embedding must authenticate operator access. This type cannot attach
/// to an existing task or restore a fresh allowance after a crash.
pub struct Context {
    store: Arc<Store>,
    task: String,
    deadline: Instant,
    maximum: usize,
    allowed: Vec<Identity>,
    _lease: Option<ExecutionLease>,
}
impl Context {
    pub fn operator(
        store: Arc<Store>,
        cfg: &Config,
        actor: &str,
        maximum: usize,
        seconds: u64,
        allowed: Vec<Identity>,
    ) -> Result<Self> {
        crate::security::authorize(actor, "expert.module.evaluate")?;
        if maximum == 0
            || maximum > 8
            || maximum > cfg.max_tool_calls as usize
            || seconds == 0
            || seconds > 30
            || seconds > cfg.max_wall_seconds
            || allowed.is_empty()
            || allowed.len() > 8
        {
            bail!("Invalid module task bounds")
        }
        let mut ids = std::collections::BTreeSet::new();
        for identity in &allowed {
            if !crate::registry::identifier(&identity.id)
                || !ids.insert(&identity.id)
                || identity.version.len() > 64
                || semver::Version::parse(&identity.version)?.to_string() != identity.version
                || identity.checksum.len() != 64
                || !identity
                    .checksum
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                bail!("Invalid module artifact identity")
            }
        }
        let lease = store.execution_lease()?;
        let start = ExecutionStart::new(seconds);
        let task = store.create_task(
            "Operator experimental module evaluation",
            "module-experiment",
        )?;
        if let Err(error) = store.event(&task, "module_scope", "security", serde_json::json!({"format_version":1,"maximum":maximum,"seconds":seconds,"allowed":allowed,"execution_clock":start.clock})) {
            store.finish_task(&task, "failed", "Module scope audit failed")?;
            return Err(error);
        }
        Ok(Self {
            store,
            task,
            deadline: start.deadline,
            maximum,
            allowed,
            _lease: lease,
        })
    }
    /// Close conservatively; a scalar return is never proof of domain correctness.
    pub fn cancel(&self) -> Result<()> {
        let mut db = self.store.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if tx.execute("UPDATE tasks SET status='cancelled',updated=unixepoch('subsec') WHERE id=?1 AND status='running'",[&self.task])? != 1 { bail!("Module task is inactive") }
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'module_cancelled','security','{}',unixepoch('subsec'))",[&self.task])?;
        tx.commit()?;
        Ok(())
    }
    pub fn task(&self) -> &str {
        &self.task
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    /// Atomically acknowledge a bounded, ordered record before guest dispatch.
    /// Scalar values and guest diagnostics are deliberately absent.
    pub fn record(&self, task: &str, artifact: &Identity, slot: usize, phase: Phase) -> Result<()> {
        if task != self.task || !self.allowed.contains(artifact) || slot == 0 || slot > self.maximum
        {
            bail!("Module audit outside task scope")
        }
        let mut db = self.store.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let status: String =
            tx.query_row("SELECT status FROM tasks WHERE id=?1", [&self.task], |r| {
                r.get(0)
            })?;
        if status != "running" || Instant::now() >= self.deadline {
            bail!("Module task is inactive or expired")
        }
        let mut slots: Vec<(Identity, Phase)> = Vec::new();
        {
            let mut stmt = tx.prepare("SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload ELSE NULL END FROM events WHERE task_id=?1 AND kind='module_invocation' ORDER BY id LIMIT 25")?;
            let rows = stmt.query_map([&self.task], |r| r.get::<_, Option<String>>(0))?;
            for (index, row) in rows.enumerate() {
                if index >= self.maximum * 3 {
                    bail!("Module audit capacity exceeded")
                }
                let raw = row?.ok_or_else(|| anyhow::anyhow!("Oversized module audit"))?;
                let record: Record =
                    serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 4096)?)?;
                if record.format_version != 1 || !self.allowed.contains(&record.artifact) {
                    bail!("Invalid module audit record")
                }
                transition(
                    &mut slots,
                    &record.artifact,
                    record.slot,
                    record.phase,
                    self.maximum,
                )?;
            }
        }
        transition(&mut slots, artifact, slot, phase, self.maximum)?;
        let payload = serde_json::to_string(&Record {
            format_version: 1,
            artifact: artifact.clone(),
            slot,
            phase,
        })?;
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'module_invocation','security',?2,unixepoch('subsec'))", params![self.task,payload])?;
        tx.commit()?;
        Ok(())
    }
}
fn transition(
    slots: &mut Vec<(Identity, Phase)>,
    artifact: &Identity,
    slot: usize,
    phase: Phase,
    maximum: usize,
) -> Result<()> {
    if phase == Phase::Allocated {
        if slot != slots.len() + 1 || slot > maximum {
            bail!("Module allocation is duplicated or exhausted")
        }
        slots.push((artifact.clone(), phase));
    } else {
        let previous = slot
            .checked_sub(1)
            .and_then(|i| slots.get_mut(i))
            .ok_or_else(|| anyhow::anyhow!("Missing module allocation"))?;
        if previous.0 != *artifact
            || !matches!(
                (previous.1, phase),
                (Phase::Allocated, Phase::DispatchReserved)
                    | (
                        Phase::DispatchReserved,
                        Phase::Returned | Phase::NotReturned
                    )
            )
        {
            bail!("Invalid module audit transition")
        }
        previous.1 = phase;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity() -> Identity {
        Identity {
            id: "math.demo.s1".into(),
            version: "1.0.0".into(),
            checksum: "a".repeat(64),
        }
    }
    fn context(store: Arc<Store>) -> Context {
        let cfg = Config::default();
        Context::operator(store, &cfg, "human-cli", 2, 10, vec![identity()]).unwrap()
    }
    #[test]
    fn transactional_sequence_denies_duplicates_replacements_and_inactive_tasks() {
        let store = Arc::new(Store::in_memory().unwrap());
        let ctx = context(store.clone());
        let id = identity();
        let task = ctx.task();
        assert!(ctx.record(task, &id, 1, Phase::Returned).is_err());
        ctx.record(task, &id, 1, Phase::Allocated).unwrap();
        assert!(ctx.record(task, &id, 1, Phase::Allocated).is_err());
        let mut replacement = id.clone();
        replacement.checksum = "b".repeat(64);
        assert!(ctx
            .record(task, &replacement, 1, Phase::DispatchReserved)
            .is_err());
        ctx.record(task, &id, 1, Phase::DispatchReserved).unwrap();
        ctx.record(task, &id, 1, Phase::Returned).unwrap();
        assert!(ctx.record(task, &id, 1, Phase::NotReturned).is_err());
        ctx.cancel().unwrap();
        assert!(ctx.cancel().is_err());
        assert!(ctx.record(task, &id, 2, Phase::Allocated).is_err());
        let count: usize = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE kind='module_invocation'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    }
    #[test]
    fn persisted_audits_survive_reopen_and_live_context_blocks_recovery() {
        let root =
            std::env::temp_dir().join(format!("cogfabric-module-audit-{}", uuid::Uuid::new_v4()));
        let path = root.join("state.db");
        let store = Arc::new(Store::open(&path).unwrap());
        let ctx = context(store.clone());
        let task = ctx.task().to_string();
        let id = identity();
        ctx.record(&task, &id, 1, Phase::Allocated).unwrap();
        ctx.record(&task, &id, 1, Phase::DispatchReserved).unwrap();
        let other = Store::open(&path).unwrap();
        assert!(other.recover_tasks().is_err());
        let count: usize = other
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='module_invocation'",
                [&task],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        drop(ctx);
        drop(store);
        drop(other);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.recover_tasks().unwrap(), vec![task.clone()]);
        assert_eq!(
            reopened.task(&task).unwrap()["task"]["status"],
            "interrupted"
        );
        let payload:String=reopened.lock().unwrap().query_row("SELECT payload FROM events WHERE task_id=?1 AND kind='module_invocation' ORDER BY id DESC LIMIT 1",[task],|r|r.get(0)).unwrap();
        let record: Record = serde_json::from_str(&payload).unwrap();
        assert_eq!(record.phase, Phase::DispatchReserved);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_allocations_share_the_sqlite_reservation() {
        let ctx = Arc::new(context(Arc::new(Store::in_memory().unwrap())));
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let ctx = ctx.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    ctx.record(ctx.task(), &identity(), 1, Phase::Allocated)
                        .is_ok()
                })
            })
            .collect();
        barrier.wait();
        assert_eq!(
            handles
                .into_iter()
                .map(|h| usize::from(h.join().unwrap()))
                .sum::<usize>(),
            1
        );
    }
    #[test]
    fn sqlite_failure_does_not_acknowledge_dispatch() {
        let store = Arc::new(Store::in_memory().unwrap());
        let ctx = context(store.clone());
        ctx.record(ctx.task(), &identity(), 1, Phase::Allocated)
            .unwrap();
        store.lock().unwrap().execute_batch("CREATE TRIGGER reject_module_dispatch BEFORE INSERT ON events WHEN NEW.kind='module_invocation' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        assert!(ctx
            .record(ctx.task(), &identity(), 1, Phase::DispatchReserved)
            .is_err());
        let count: usize = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE kind='module_invocation'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn corrupt_records_and_expiry_fail_closed() {
        let store = Arc::new(Store::in_memory().unwrap());
        let mut ctx = context(store.clone());
        let id = identity();
        store
            .event(
                ctx.task(),
                "module_invocation",
                "security",
                serde_json::json!({"bad":"record"}),
            )
            .unwrap();
        assert!(ctx.record(ctx.task(), &id, 1, Phase::Allocated).is_err());
        ctx.deadline = Instant::now();
        assert!(ctx.record(ctx.task(), &id, 1, Phase::Allocated).is_err());
        for actor in ["interface", "jev-r", "jev-plus", "unknown"] {
            assert!(crate::security::authorize(actor, "expert.module.evaluate").is_err());
        }
    }
}
