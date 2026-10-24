//! Bounded observation of recorded capability gaps; never grants capabilities.
use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Serialize)]
pub struct Preparation {
    pub task_id: String,
    pub capability: String,
    pub gap_event_id: i64,
    pub gap_checksum: String,
    pub available_now: bool,
    pub next_steps: Vec<&'static str>,
    pub candidate_created: bool,
    pub permissions_granted: bool,
}

fn inspect<T>(
    store: &Store,
    deadline: std::time::Instant,
    operation: impl FnOnce(&mut rusqlite::Connection) -> Result<T>,
) -> Result<T> {
    let mut db = store.lock()?;
    if std::time::Instant::now() >= deadline {
        bail!("Capability-gap inspection deadline exhausted")
    }
    db.progress_handler(1000, Some(move || std::time::Instant::now() >= deadline));
    let result = operation(&mut db);
    db.progress_handler(0, None::<fn() -> bool>);
    if result.is_ok() && std::time::Instant::now() >= deadline {
        bail!("Capability-gap inspection deadline exhausted")
    }
    result
}

pub fn prepare(store: &Store, task: &str, capability: &str) -> Result<Preparation> {
    prepare_before(
        store,
        task,
        capability,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
    )
}

pub(crate) fn prepare_before(
    store: &Store,
    task: &str,
    capability: &str,
    deadline: std::time::Instant,
) -> Result<Preparation> {
    security::authorize("human-cli", "candidate.prepare")?;
    uuid::Uuid::parse_str(task)?;
    if !crate::registry::identifier(capability) {
        bail!("Invalid capability identity")
    }
    inspect(store, deadline, |connection| {
        let db = connection.transaction()?;
        let mut statement = db.prepare("SELECT id,substr(payload,1,16001) FROM events WHERE task_id=?1 AND EXISTS(SELECT 1 FROM tasks WHERE id=?1) AND kind='capability_gap' AND source='jev-plus' ORDER BY id DESC LIMIT 65")?;
        let rows = statement
            .query_map([task], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        if rows.len() > 64 {
            bail!("Capability gap history exceeds inspection limit")
        }
        let mut selected = None;
        for (id, raw) in rows {
            if raw.len() > 16_000 {
                bail!("Capability gap event exceeds inspection limit")
            }
            let payload = crate::strict_json::value(raw.as_bytes(), 16_000)?;
            let missing = payload["missing"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid gap event"))?;
            if missing.is_empty() || missing.len() > 16 {
                bail!("Invalid gap capability count")
            }
            let mut unique = BTreeSet::new();
            for entry in missing {
                let entry = entry
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid gap identity"))?;
                if !crate::registry::identifier(entry) || !unique.insert(entry) {
                    bail!("Invalid or duplicate gap identity")
                }
            }
            if selected.is_none() && unique.contains(capability) {
                selected = Some((id, sha256(raw.as_bytes())));
            }
        }
        let (id, checksum) = selected
            .ok_or_else(|| anyhow::anyhow!("No recorded gap for this task and capability"))?;
        let available = crate::registry::provider(
            &Store::skills_from(&db)?,
            &crate::registry::snapshot_from(&db)?,
            capability,
        )
        .is_some();
        db.commit()?;
        Ok(Preparation {task_id:task.into(), capability:capability.into(), gap_event_id:id, gap_checksum:checksum,
        available_now:available, next_steps:vec!["Reuse an available reviewed skill before preparing new artifacts", "Review and register reference capability metadata if absent", "Select a reviewed source catalog and approve licensed source documents before preparing a quarantined RAG candidate", "Supply independent held-out retrieval cases and evaluate integrity, quality and historical regressions", "Review results before explicit human promotion"], candidate_created:false, permissions_granted:false})
    })
}

pub fn recorded(store: &Store, task: &str) -> Result<Value> {
    security::authorize("human-cli", "candidate.prepare")?;
    uuid::Uuid::parse_str(task)?;
    inspect(
        store,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        |db| {
            let mut statement = db.prepare("SELECT id,substr(payload,1,16001) FROM events WHERE task_id=?1 AND EXISTS(SELECT 1 FROM tasks WHERE id=?1) AND kind='capability_gap' AND source='jev-plus' ORDER BY id LIMIT 65")?;
            let rows = statement
                .query_map([task], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if rows.len() > 64 {
                bail!("Capability gap history exceeds inspection limit")
            }
            let mut result = Vec::new();
            for (id, raw) in rows {
                if raw.len() > 16_000 {
                    bail!("Capability gap event exceeds inspection limit")
                }
                let payload = crate::strict_json::value(raw.as_bytes(), 16_000)?;
                result.push(
                json!({"event_id":id,"checksum":sha256(raw.as_bytes()),"recorded_advice":payload}),
            );
            }
            Ok(json!({"task_id":task,"gaps":result,"trusted_permissions":false}))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_gap_keys_are_refused_and_inspection_hooks_are_cleared() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("Strict recorded gap", "fast").unwrap();
        store
            .event(
                &task,
                "capability_gap",
                "jev-plus",
                json!({"missing":["rust.s1"]}),
            )
            .unwrap();
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE events SET payload=?1 WHERE task_id=?2",
                [
                    "{\"missing\":[\"rust.s1\"],\"missing\":[\"sql.s1\"]}",
                    &task,
                ],
            )
            .unwrap();
        assert!(prepare(&store, &task, "rust.s1").is_err());
        assert!(recorded(&store, &task).is_err());
        let mut invoked = false;
        let expired: Result<()> = inspect(&store, std::time::Instant::now(), |_| {
            invoked = true;
            Ok(())
        });
        assert!(expired.is_err());
        assert!(!invoked);
        let interrupted: Result<u64> = inspect(
            &store,
            std::time::Instant::now() + std::time::Duration::from_millis(10),
            |db| {
                Ok(db.query_row("WITH RECURSIVE x(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM x WHERE n<1000000000) SELECT sum(n) FROM x",[],|row|row.get(0))?)
            },
        );
        assert!(interrupted.is_err());
        let total:u64=store.lock().unwrap().query_row("WITH RECURSIVE x(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM x WHERE n<10000) SELECT sum(n) FROM x",[],|row|row.get(0)).unwrap();
        assert_eq!(total, 50005000);
    }

    #[test]
    fn preparation_requires_recorded_gap_and_never_creates_candidates() {
        let store = Store::in_memory().unwrap();
        let task = store
            .create_task("Explain a missing capability", "standard")
            .unwrap();
        store
            .event(
                &task,
                "capability_gap",
                "jev-plus",
                json!({"missing":["rust.async.s1"],"action":"untrusted advice"}),
            )
            .unwrap();
        let prepared = prepare(&store, &task, "rust.async.s1").unwrap();
        assert!(!prepared.available_now);
        assert!(!prepared.permissions_granted);
        assert!(!prepared.candidate_created);
        assert_eq!(prepared.gap_checksum.len(), 64);
        assert!(store.candidates().unwrap().is_empty());
        assert!(prepare(&store, &task, "shell.execute").is_err());
        store
            .ensure_skill("rust.async.s1", "Reviewed reference", &["rust".into()])
            .unwrap();
        assert!(
            prepare(&store, &task, "rust.async.s1")
                .unwrap()
                .available_now
        );
        assert!(security::authorize("jev-r", "candidate.prepare").is_err());
        store
            .event(
                &task,
                "capability_gap",
                "jev-plus",
                json!({"missing":["../../escape"]}),
            )
            .unwrap();
        assert!(prepare(&store, &task, "rust.async.s1").is_err());
    }
}
