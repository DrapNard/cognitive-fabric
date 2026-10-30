//! Logical database quotas and explicit reviewed task-history retention.
pub(crate) mod wal_vfs;

use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

pub const DEFAULT_DATABASE_BYTES: u64 = 256_000_000;
pub(crate) const SCHEMA: &str = r#"
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE TABLE retention_audit (
 id TEXT PRIMARY KEY, plan_checksum TEXT NOT NULL, task_ids TEXT NOT NULL,
 deleted_tasks INTEGER NOT NULL, deleted_events INTEGER NOT NULL, created REAL NOT NULL
);
CREATE INDEX event_conversation_task ON events(json_extract(payload,'$.task_id'))
 WHERE kind='conversation_reference' AND json_valid(payload);
PRAGMA user_version=6;
"#;
pub(crate) fn configure(db: &Connection, bytes: u64) -> Result<()> {
    if !(64_000_000..=2_000_000_000).contains(&bytes) {
        bail!("Database limit must be 64 MB to 2 GB")
    }
    let page_size: u64 = db.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let requested = bytes / page_size;
    let actual: u64 =
        db.pragma_update_and_check(None, "max_page_count", requested, |row| row.get(0))?;
    if actual > requested {
        bail!("Existing database exceeds configured limit; preserve data and raise the limit")
    }
    db.pragma_update(None, "wal_autocheckpoint", 1000)?;
    db.pragma_update(None, "journal_size_limit", 16_000_000)?;
    Ok(())
}
pub(crate) fn admit(db: &Connection) -> Result<()> {
    let pages: u64 = db.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let free: u64 = db.pragma_query_value(None, "freelist_count", |row| row.get(0))?;
    let maximum: u64 = db.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
    let size: u64 = db.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let active: u64 = db.query_row(
        "SELECT count(*) FROM tasks WHERE status IN ('running','waiting_client')",
        [],
        |row| row.get(0),
    )?;
    if maximum
        .saturating_sub(pages.saturating_sub(free))
        .saturating_mul(size)
        < 16_000_000
        || active >= 8
    {
        bail!("Task admission denied: storage headroom or active-task capacity exhausted")
    }
    Ok(())
}
#[derive(Serialize)]
pub struct StorageStats {
    pub allocated_bytes: u64,
    pub live_page_bytes: u64,
    pub maximum_bytes: u64,
    pub wal_bytes: Option<u64>,
    pub wal_hard_limit: bool,
    pub wal_limit_bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPlan {
    pub before_unix: u64,
    pub limit: u32,
    pub task_ids: Vec<String>,
    pub event_count: u64,
    pub checksum: String,
}
impl RetentionPlan {
    fn digest(&self) -> Result<String> {
        Ok(sha256(&serde_json::to_vec(&(
            self.before_unix,
            self.limit,
            &self.task_ids,
            self.event_count,
        ))?))
    }
    fn validate(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        if self.before_unix > now
            || !(1..=1000).contains(&self.limit)
            || self.task_ids.len() > self.limit as usize
            || self.checksum != self.digest()?
        {
            bail!("Invalid retention plan")
        }
        for task in &self.task_ids {
            uuid::Uuid::parse_str(task)?;
        }
        Ok(())
    }
}
fn preview(db: &Connection, before: u64, limit: u32) -> Result<RetentionPlan> {
    let mut plan = RetentionPlan {
        before_unix: before,
        limit,
        task_ids: Vec::new(),
        event_count: 0,
        checksum: String::new(),
    };
    plan.checksum = plan.digest()?;
    plan.validate()?;
    plan.task_ids=db.prepare("SELECT t.id FROM tasks t WHERE t.updated<?1 AND t.status IN ('completed','unverified','demo','failed','rejected','cancelled','exhausted','needs_clarification','interrupted') AND NOT EXISTS(SELECT 1 FROM events ge WHERE ge.task_id=t.id AND ge.kind IN ('gap_candidate','candidate_preparation','residency_plan','native_task_lifecycle','native_task_scope','agent_native_scope','agent_native_activation')) AND NOT EXISTS(SELECT 1 FROM memories m WHERE m.task_id=t.id) AND NOT EXISTS(SELECT 1 FROM events e WHERE e.kind='conversation_reference' AND json_valid(e.payload) AND json_extract(e.payload,'$.task_id')=t.id) AND NOT EXISTS(SELECT 1 FROM events pe WHERE pe.kind='residency_plan_step' AND json_valid(pe.payload) AND json_extract(pe.payload,'$.operation_task_id')=t.id) AND NOT EXISTS(SELECT 1 FROM managed_residency_receipts mr WHERE mr.load_task_id=t.id OR mr.release_task_id=t.id) AND NOT EXISTS(SELECT 1 FROM managed_residency_receipt_archive ma WHERE ma.load_task_id=t.id OR ma.release_task_id=t.id OR ma.retirement_task_id=t.id) AND NOT EXISTS(SELECT 1 FROM managed_residency_resolutions mx WHERE mx.original_task_id=t.id OR mx.resolver_task_id=t.id) AND NOT EXISTS(SELECT 1 FROM events me WHERE me.task_id=t.id AND me.kind='model_residency' AND CASE WHEN length(CAST(me.payload AS BLOB))<=4096 AND json_valid(me.payload) THEN json_type(me.payload,'$.managed') IS NOT NULL ELSE 1 END) ORDER BY t.updated,t.id LIMIT ?2")?
        .query_map(params![before,limit],|row|row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    let candidates = std::mem::take(&mut plan.task_ids);
    for id in candidates {
        if crate::resource_scheduler::ResourcePool::retention_safe(db, &id)? {
            plan.task_ids.push(id);
        }
    }
    for id in &plan.task_ids {
        plan.event_count += db.query_row(
            "SELECT count(*) FROM events WHERE task_id=?1",
            [id],
            |row| row.get::<_, u64>(0),
        )?;
    }
    plan.checksum = plan.digest()?;
    plan.validate()?;
    Ok(plan)
}
impl Store {
    pub fn storage_stats(&self) -> Result<StorageStats> {
        let db = self.lock()?;
        let pages: u64 = db.pragma_query_value(None, "page_count", |row| row.get(0))?;
        let free: u64 = db.pragma_query_value(None, "freelist_count", |row| row.get(0))?;
        let maximum: u64 = db.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
        let size: u64 = db.pragma_query_value(None, "page_size", |row| row.get(0))?;
        let wal_bytes = db
            .path()
            .filter(|path| !path.is_empty())
            .map(|path| match std::fs::metadata(format!("{path}-wal")) {
                Ok(metadata) => Ok(metadata.len()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
                Err(error) => Err(error),
            })
            .transpose()?;
        Ok(StorageStats {
            allocated_bytes: pages * size,
            live_page_bytes: pages.saturating_sub(free) * size,
            maximum_bytes: maximum * size,
            wal_bytes,
            wal_hard_limit: wal_vfs::enforced_limit(&db).is_some(),
            wal_limit_bytes: wal_vfs::enforced_limit(&db),
        })
    }
    /// Explicit operator maintenance; busy readers are reported, never evicted.
    pub fn checkpoint_wal(&self) -> Result<serde_json::Value> {
        security::authorize("human-cli", "storage.checkpoint")?;
        let db = self.lock()?;
        let previous: u64 = db.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
        db.busy_timeout(std::time::Duration::from_millis(250))?;
        let result = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        });
        let restored = db.busy_timeout(std::time::Duration::from_millis(previous));
        let (busy, log_frames, checkpointed_frames) = result?;
        restored?;
        Ok(serde_json::json!({"completed":busy==0,"busy":busy!=0,
            "log_frames":log_frames,"checkpointed_frames":checkpointed_frames,
            "mode":"truncate","tasks_replayed":false,"readers_evicted":false}))
    }

    pub fn retention_preview(&self, before: u64, limit: u32) -> Result<RetentionPlan> {
        security::authorize("human-cli", "storage.retention")?;
        let mut db = self.lock()?;
        let tx = db.transaction()?;
        let plan = preview(&tx, before, limit)?;
        tx.commit()?;
        Ok(plan)
    }
    pub fn apply_retention(&self, plan: &RetentionPlan, human_approve: bool) -> Result<String> {
        security::authorize("human-cli", "storage.retention")?;
        if !human_approve {
            bail!("Retention requires explicit --human-approve")
        }
        let _resource_lease = self
            .runtime_lock
            .as_ref()
            .map(|path| {
                let mut resource_path = path.as_os_str().to_os_string();
                resource_path.push(".resources");
                crate::recovery::ExecutionLease::acquire(std::path::Path::new(&resource_path), true)
            })
            .transpose()?;
        plan.validate()?;
        if plan.task_ids.is_empty() {
            bail!("Retention plan is empty")
        }
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if preview(&tx, plan.before_unix, plan.limit)?.checksum != plan.checksum {
            bail!("Retention preview is stale; review a fresh plan")
        }
        let count: u64 =
            tx.query_row("SELECT count(*) FROM retention_audit", [], |row| row.get(0))?;
        if count >= 10000 {
            bail!("Retention audit quota exhausted")
        }
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO retention_audit VALUES (?1,?2,?3,?4,?5,unixepoch())",
            params![
                id,
                plan.checksum,
                serde_json::to_string(&plan.task_ids)?,
                plan.task_ids.len() as u64,
                plan.event_count
            ],
        )?;
        for task in &plan.task_ids {
            tx.execute("DELETE FROM events WHERE task_id=?1", [task])?;
            if tx.execute("DELETE FROM tasks WHERE id=?1", [task])? != 1 {
                bail!("Retention task changed")
            }
        }
        tx.commit()?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn old_task(store: &Store, query: &str) -> String {
        let id = store.create_task(query, "fast").unwrap();
        store.finish_task(&id, "completed", "result").unwrap();
        store
            .lock()
            .unwrap()
            .execute("UPDATE tasks SET updated=0 WHERE id=?1", [&id])
            .unwrap();
        id
    }
    #[test]
    fn retention_requires_review_and_preserves_referenced_and_running_tasks() {
        let store = Store::in_memory().unwrap();
        let disposable = old_task(&store, "disposable");
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE tasks SET status='rejected' WHERE id=?1",
                [&disposable],
            )
            .unwrap();
        let memory_source = store.create_task("calc: 1 + 2", "fast").unwrap();
        store.finish_task(&memory_source, "completed", "3").unwrap();
        store
            .lock()
            .unwrap()
            .execute("UPDATE tasks SET updated=0 WHERE id=?1", [&memory_source])
            .unwrap();
        store
            .publish_memory(&memory_source, "verified_arithmetic", "reviewed")
            .unwrap();
        let referenced = old_task(&store, "referenced source");
        let running = store.create_task("running", "standard").unwrap();
        store
            .event(
                &running,
                "conversation_reference",
                "interface",
                serde_json::json!({"task_id":referenced}),
            )
            .unwrap();
        store
            .event(&disposable, "intent", "interface", serde_json::json!({}))
            .unwrap();
        let plan = store.retention_preview(1, 100).unwrap();
        assert_eq!(plan.task_ids, vec![disposable.clone()]);
        assert_eq!(plan.event_count, 1);
        assert!(store.apply_retention(&plan, false).is_err());
        store.apply_retention(&plan, true).unwrap();
        assert!(store.task(&disposable).unwrap()["task"].is_null());
        for id in [&memory_source, &referenced, &running] {
            assert!(!store.task(id).unwrap()["task"].is_null());
        }
        assert!(store.apply_retention(&plan, true).is_err());
        assert!(store.retention_preview(u64::MAX, 100).is_err());
        assert!(store.retention_preview(1, 1001).is_err());
        assert!(security::authorize("jev-plus", "storage.retention").is_err());
    }

    #[test]
    fn waiting_client_sessions_consume_active_task_capacity() {
        let store = Store::in_memory().unwrap();
        let tasks = (0..8)
            .map(|index| {
                store
                    .create_task(&format!("active {index}"), "fast")
                    .unwrap()
            })
            .collect::<Vec<_>>();
        store.finish_task(&tasks[0], "waiting_client", "").unwrap();
        {
            let db = store.lock().unwrap();
            assert!(admit(&db).is_err());
            db.execute(
                "UPDATE tasks SET status='completed' WHERE id=?1",
                [&tasks[0]],
            )
            .unwrap();
            admit(&db).unwrap();
        }
    }

    #[test]
    fn retention_preserves_managed_receipt_tasks_and_unresolved_managed_operations() {
        let store = Store::in_memory().unwrap();
        let disposable = old_task(&store, "ordinary old task");
        let load_task = old_task(&store, "managed load source");
        let release_task = old_task(&store, "managed release source");
        let unknown_task = old_task(&store, "managed unknown operation");
        let malformed_task = old_task(&store, "malformed residency audit");

        let receipt_json = r#"{"format_version":1}"#;
        let receipt_hash = sha256(receipt_json.as_bytes());
        let inventory_hash = sha256(b"inventory");
        store.lock().unwrap().execute(
            "INSERT INTO managed_residency_receipts(receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,current_inventory_sha256,state,load_task_id,release_task_id,created,updated) VALUES ('receipt-retention','http://127.0.0.1:1234/api/v1','instance-retention',?1,?2,?3,'released',?4,?5,1,1)",
            params![receipt_json, receipt_hash, inventory_hash, load_task, release_task],
        ).unwrap();
        store.event(
            &unknown_task,
            "model_residency",
            "human-cli",
            serde_json::json!({"format_version":1,"managed":true,"state":"outcome_unknown","endpoint":"http://127.0.0.1:1234/api/v1"}),
        ).unwrap();
        // Malformed operation history must be retained conservatively because it
        // cannot be proven to describe an unmanaged mutation.
        store.lock().unwrap().execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'model_residency','human-cli','{',1)",
            [&malformed_task],
        ).unwrap();

        for marker in [
            serde_json::json!("true"),
            serde_json::json!(1),
            serde_json::Value::Null,
            serde_json::json!(false),
        ] {
            let task = old_task(&store, "ambiguous managed marker");
            store
                .event(
                    &task,
                    "model_residency",
                    "human-cli",
                    serde_json::json!({"managed":marker,"state":"requested"}),
                )
                .unwrap();
        }
        let plan = store.retention_preview(1, 100).unwrap();
        assert_eq!(plan.task_ids, vec![disposable]);
        assert_eq!(plan.event_count, 0);
    }
    #[test]
    fn stale_plans_and_delete_failures_cannot_partially_remove_history() {
        let store = Store::in_memory().unwrap();
        let task = old_task(&store, "old");
        let stale = store.retention_preview(1, 1).unwrap();
        store
            .event(
                &task,
                "policy",
                "security-kernel",
                serde_json::json!({"allowed":false}),
            )
            .unwrap();
        assert!(store
            .apply_retention(&stale, true)
            .unwrap_err()
            .to_string()
            .contains("stale"));
        let plan = store.retention_preview(1, 1).unwrap();
        store.lock().unwrap().execute_batch("CREATE TRIGGER fail_retention BEFORE DELETE ON tasks BEGIN SELECT RAISE(ABORT,'injected deletion failure'); END;").unwrap();
        assert!(store.apply_retention(&plan, true).is_err());
        assert_eq!(
            store.task(&task).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let count: u64 = store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM retention_audit", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_retention")
            .unwrap();
        store.apply_retention(&plan, true).unwrap();
        let audit: (u64, u64) = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT deleted_tasks,deleted_events FROM retention_audit",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(audit, (1, 1));
    }
    #[test]
    fn pinned_reader_cannot_grow_runtime_wal_past_the_write_boundary() {
        let root = std::env::temp_dir().join(format!("fabric-wal-bound-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("state.db");
        let store = Store::open(&path).unwrap();
        {
            let db = store.lock().unwrap();
            db.execute_batch(
                "CREATE TABLE wal_bound_fixture(body BLOB); PRAGMA wal_checkpoint(TRUNCATE);",
            )
            .unwrap();
        }
        let initial = store.storage_stats().unwrap();
        assert!(initial.wal_hard_limit);
        let limit = initial.wal_limit_bytes.unwrap();
        let reader = wal_vfs::open(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let _: u64 = reader
            .query_row("SELECT count(*) FROM wal_bound_fixture", [], |r| r.get(0))
            .unwrap();
        let second_writer =
            wal_vfs::open(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        let mut committed = 0u64;
        let mut refused = false;
        for index in 0..80 {
            let db = store.lock().unwrap();
            let writer = if index % 2 == 0 { &*db } else { &second_writer };
            match writer.execute("INSERT INTO wal_bound_fixture VALUES(zeroblob(500000))", []) {
                Ok(_) => committed += 1,
                Err(error) => {
                    assert_eq!(
                        error.sqlite_error_code(),
                        Some(rusqlite::ErrorCode::DiskFull)
                    );
                    refused = true;
                    break;
                }
            }
            assert!(
                std::fs::metadata(format!("{}-wal", path.display()))
                    .unwrap()
                    .len()
                    <= limit
            );
        }
        assert!(refused && committed > 0);
        assert!(store.storage_stats().unwrap().wal_bytes.unwrap() <= limit);
        assert_eq!(store.checkpoint_wal().unwrap()["busy"], true);
        reader.execute_batch("ROLLBACK").unwrap();
        drop(reader);
        drop(second_writer);
        assert_eq!(store.checkpoint_wal().unwrap()["completed"], true);
        {
            let db = store.lock().unwrap();
            db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
            assert_eq!(
                db.query_row("SELECT count(*) FROM wal_bound_fixture", [], |r| r
                    .get::<_, u64>(0))
                    .unwrap(),
                committed
            );
            assert_eq!(
                db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
                    .unwrap(),
                "ok"
            );
            db.execute("INSERT INTO wal_bound_fixture VALUES(zeroblob(500000))", [])
                .unwrap();
        }
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM wal_bound_fixture", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            committed + 1
        );
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_external_wal_is_preserved_and_can_be_checkpointed() {
        let root = std::env::temp_dir().join(format!("fabric-wal-legacy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("state.db");
        drop(Store::open(&path).unwrap());
        // An external, unwrapped writer deliberately bypasses the runtime VFS.
        let writer = Connection::open(&path).unwrap();
        writer.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE legacy_wal_fixture(body BLOB); PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        let reader = Connection::open(&path).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let _: u64 = reader
            .query_row("SELECT count(*) FROM legacy_wal_fixture", [], |r| r.get(0))
            .unwrap();
        for _ in 0..36 {
            writer
                .execute(
                    "INSERT INTO legacy_wal_fixture VALUES(zeroblob(500000))",
                    [],
                )
                .unwrap();
        }
        let before = std::fs::metadata(format!("{}-wal", path.display()))
            .unwrap()
            .len();
        let bounded = wal_vfs::open(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        assert!(before > wal_vfs::enforced_limit(&bounded).unwrap());
        assert_eq!(
            bounded
                .query_row("SELECT count(*) FROM legacy_wal_fixture", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            36
        );
        let error = bounded
            .execute("INSERT INTO legacy_wal_fixture VALUES(zeroblob(1))", [])
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DiskFull)
        );
        assert_eq!(
            std::fs::metadata(format!("{}-wal", path.display()))
                .unwrap()
                .len(),
            before
        );
        reader.execute_batch("ROLLBACK").unwrap();
        drop(reader);
        drop(writer);
        bounded
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        bounded
            .execute("INSERT INTO legacy_wal_fixture VALUES(zeroblob(1))", [])
            .unwrap();
        assert_eq!(
            bounded
                .query_row("SELECT count(*) FROM legacy_wal_fixture", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            37
        );
        drop(bounded);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quotas_enforce_admission_and_sqlite_page_growth() {
        let store = Store::in_memory().unwrap();
        let stats = store.storage_stats().unwrap();
        assert!(stats.maximum_bytes <= DEFAULT_DATABASE_BYTES);
        assert!(!stats.wal_hard_limit);
        for _ in 0..8 {
            store.create_task("pending", "fast").unwrap();
        }
        assert!(store.create_task("ninth", "fast").is_err());
        let db = store.lock().unwrap();
        db.execute_batch("CREATE TABLE quota_fixture(body BLOB)")
            .unwrap();
        let pages: u64 = db
            .pragma_query_value(None, "page_count", |row| row.get(0))
            .unwrap();
        db.pragma_update(None, "max_page_count", pages + 2).unwrap();
        assert!(admit(&db).is_err());
        let error = db
            .execute("INSERT INTO quota_fixture VALUES (zeroblob(1000000))", [])
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DiskFull)
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM quota_fixture", [], |row| row
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
        assert!(configure(&db, 1000).is_err());
    }
}
