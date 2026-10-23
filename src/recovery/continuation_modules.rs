//! Never silently replace a task's host-registered module implementation on resume.
use anyhow::{bail, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format_version: u32,
    broker_id: String,
    modules: Vec<serde_json::Value>,
    permissions_granted: bool,
}

pub(super) fn validate(db: &rusqlite::Connection, task: &str) -> Result<()> {
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END, CASE WHEN length(CAST(payload AS BLOB))<=65536 THEN payload END
         FROM events WHERE task_id=?1 AND kind='typed_module_snapshot' ORDER BY id LIMIT 2",
    )?;
    let mut rows = statement.query([task])?;
    if let Some(row) = rows.next()? {
        let source: String = row.get(0)?;
        let raw: String = row.get(1)?;
        let snapshot: Snapshot =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 65536)?)?;
        if source != "fabric"
            || snapshot.format_version != 1
            || snapshot.permissions_granted
            || uuid::Uuid::parse_str(&snapshot.broker_id).is_err()
            || rows.next()?.is_some()
        {
            bail!("Invalid or duplicate typed module continuation snapshot")
        }
        if !snapshot.modules.is_empty() {
            bail!("Typed module continuation requires restoring its original host registry; default continuation cannot substitute implementations")
        }
    }
    let dispatched: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind IN
        ('module_dispatch_intent','module_dispatch_outcome','module_process_launch',
         'module_process_owned','module_process_dispatch_outcome','expert_provider_snapshot'))",
        [task],
        |row| row.get(0),
    )?;
    if dispatched {
        bail!("Typed module execution history requires exact host registry restoration before continuation")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use serde_json::json;

    #[test]
    fn empty_or_legacy_module_state_needs_no_new_runtime_authority() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("Legacy checkpoint", "standard").unwrap();
        assert!(validate(&store.lock().unwrap(), &task).is_ok());
        store
            .event(
                &task,
                "typed_module_snapshot",
                "fabric",
                json!({
                    "format_version":1,"broker_id":uuid::Uuid::new_v4().to_string(),
                    "modules":[],"permissions_granted":false
                }),
            )
            .unwrap();
        assert!(validate(&store.lock().unwrap(), &task).is_ok());
    }

    #[test]
    fn pinned_missing_duplicate_and_forged_module_histories_refuse_without_writes() {
        for scenario in 0..7 {
            let store = Store::in_memory().unwrap();
            let task = store
                .create_task("Refuse substituted modules", "standard")
                .unwrap();
            let mut snapshot = json!({"format_version":1,"broker_id":uuid::Uuid::new_v4().to_string(),
                "modules":[],"permissions_granted":false});
            match scenario {
                0 => snapshot["modules"] = json!([{"id":"host.custom","version":"1.0.0"}]),
                1 => snapshot["broker_id"] = json!("forged"),
                2 => snapshot["permissions_granted"] = json!(true),
                3 => snapshot["format_version"] = json!(2),
                _ => {}
            }
            if scenario != 6 {
                store
                    .event(&task, "typed_module_snapshot", "fabric", snapshot.clone())
                    .unwrap();
            }
            if scenario == 4 {
                store
                    .event(&task, "typed_module_snapshot", "fabric", snapshot)
                    .unwrap();
            }
            if scenario >= 5 {
                store
                    .event(
                        &task,
                        "module_dispatch_intent",
                        "jev-plus",
                        json!({"outcome":"unknown"}),
                    )
                    .unwrap();
            }
            let before = store.task(&task).unwrap();
            assert!(
                validate(&store.lock().unwrap(), &task).is_err(),
                "scenario {scenario}"
            );
            assert_eq!(store.task(&task).unwrap(), before);
        }
    }
}
