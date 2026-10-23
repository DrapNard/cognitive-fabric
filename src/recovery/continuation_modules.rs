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

use crate::fabric::modules::{Broker, Capability, Descriptor, Pin};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(super) type Pins = BTreeMap<Capability, Pin>;
#[derive(Clone)]
pub(super) struct Restored {
    pub broker: Arc<Broker>,
    pub pins: Pins,
    evidence: Value,
}
impl Restored {
    pub fn evidence(&self) -> Value {
        self.evidence.clone()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleSnapshot {
    id: String,
    version: String,
    capabilities: Vec<String>,
}

fn capability(name: &str) -> Result<Capability> {
    Ok(match name {
        "math.evaluate" => Capability::MathEvaluate,
        "sql.query" => Capability::SqlQuery,
        "sorting.verify" => Capability::SortingVerify,
        "ordering.verify" => Capability::OrderingVerify,
        "path.verify" => Capability::PathVerify,
        "constraints.solve" => Capability::ConstraintsSolve,
        _ => bail!("Unknown original typed module capability"),
    })
}

#[cfg(test)]
pub(super) fn validate(db: &rusqlite::Connection, task: &str) -> Result<()> {
    restore(db, task, None).map(|_| ())
}

pub(super) fn restore(
    db: &rusqlite::Connection,
    task: &str,
    broker: Option<Arc<Broker>>,
) -> Result<Option<Restored>> {
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END, CASE WHEN length(CAST(payload AS BLOB))<=65536 THEN payload END,id
         FROM events WHERE task_id=?1 AND kind='typed_module_snapshot' ORDER BY id LIMIT 2",
    )?;
    let mut rows = statement.query([task])?;
    let Some(row) = rows.next()? else {
        let related: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind IN
             ('module_dispatch_intent','module_dispatch_outcome','module_process_launch',
              'module_process_owned','module_process_stopped','module_process_dispatch_intent',
              'module_process_dispatch_outcome','expert_provider_snapshot'))",
            [task],
            |row| row.get(0),
        )?;
        if related || broker.is_some() {
            bail!("Typed registry restoration has no original task snapshot")
        }
        return Ok(None);
    };
    let source: String = row.get(0)?;
    let raw: String = row.get(1)?;
    let snapshot_id: i64 = row.get(2)?;
    let snapshot: Snapshot =
        serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 65536)?)?;
    if source != "fabric"
        || snapshot.format_version != 1
        || snapshot.permissions_granted
        || uuid::Uuid::parse_str(&snapshot.broker_id).is_err()
        || rows.next()?.is_some()
        || snapshot.modules.len() > 32
    {
        bail!("Invalid or duplicate typed module continuation snapshot")
    }
    let Some(broker) = broker else {
        if !snapshot.modules.is_empty() {
            bail!("Typed module continuation requires restoring its original host registry; default continuation cannot substitute implementations")
        }
        super::module_dispatch_history::validate(
            db,
            task,
            &Pins::new(),
            &snapshot.broker_id,
            snapshot_id,
        )?;
        return Ok(None);
    };
    if broker.identity_id().to_string() != snapshot.broker_id {
        bail!("Restored typed registry is not the original Broker instance")
    }
    let mut pins = Pins::new();
    let mut ids = BTreeSet::new();
    for value in snapshot.modules {
        let module: ModuleSnapshot = serde_json::from_value(value)?;
        if !ids.insert(module.id.clone())
            || module.capabilities.is_empty()
            || module.capabilities.len() > 6
        {
            bail!("Invalid original module identity or capability count")
        }
        let capabilities = module
            .capabilities
            .iter()
            .map(|name| capability(name))
            .collect::<Result<BTreeSet<_>>>()?;
        if capabilities.len() != module.capabilities.len() {
            bail!("Duplicate original module capability")
        }
        let descriptor = Descriptor {
            id: module.id,
            version: module.version,
            capabilities,
        };
        let pin = broker.pin_retained(task, &descriptor)?;
        for cap in descriptor.capabilities {
            if pins.insert(cap, pin.clone()).is_some() {
                bail!("Conflicting original module capability pins")
            }
        }
    }
    let completed = super::module_dispatch_history::validate(
        db,
        task,
        &pins,
        &snapshot.broker_id,
        snapshot_id,
    )?;
    let evidence = json!({"format_version":1,"broker_id":snapshot.broker_id,
        "snapshot_event_id":snapshot_id,"snapshot_sha256":crate::store::sha256(raw.as_bytes()),
        "completed_dispatches":completed,"restored_capabilities":pins.keys().map(|cap|cap.as_str()).collect::<Vec<_>>(),
        "original_versions_preserved":true,"original_budget_preserved":true,
        "permissions_granted":false,"automatic_replay":false,"implementation_attested":false});
    Ok(Some(Restored {
        broker,
        pins,
        evidence,
    }))
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
