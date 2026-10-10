//! Logical database quotas and explicit reviewed task-history retention.
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
    let running: u64 = db.query_row(
        "SELECT count(*) FROM tasks WHERE status='running'",
        [],
        |row| row.get(0),
    )?;
    if maximum
        .saturating_sub(pages.saturating_sub(free))
        .saturating_mul(size)
        < 16_000_000
        || running >= 8
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
    plan.task_ids=db.prepare("SELECT t.id FROM tasks t WHERE t.updated<?1 AND t.status IN ('completed','unverified','demo','failed','cancelled','exhausted','needs_clarification','interrupted') AND NOT EXISTS(SELECT 1 FROM memories m WHERE m.task_id=t.id) AND NOT EXISTS(SELECT 1 FROM events e WHERE e.kind='conversation_reference' AND json_valid(e.payload) AND json_extract(e.payload,'$.task_id')=t.id) ORDER BY t.updated,t.id LIMIT ?2")?
        .query_map(params![before,limit],|row|row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
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
            wal_hard_limit: false,
        })
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
        let memory_source = old_task(&store, "published source");
        store
            .publish_memory(&memory_source, "episode", "reviewed")
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
