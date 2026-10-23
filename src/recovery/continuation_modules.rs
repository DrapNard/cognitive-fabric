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

#[cfg(feature = "wasm-experts")]
use crate::fabric::modules::Module;
use crate::fabric::modules::{Broker, Capability, Descriptor, Limits, Pin};
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
    #[serde(default)]
    source_sha256: Option<String>,
    #[serde(default)]
    implementation: Option<Value>,
    #[serde(default)]
    limits: Option<Limits>,
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

/// Reconstruct only the fixed compiled worker, never a path or callback named
/// by persisted metadata. Actual continuation independently validates all gates.
pub(super) fn reconstruct_builtin(
    store: Arc<crate::store::Store>,
    task: &str,
    human_approve: bool,
) -> Result<Arc<Broker>> {
    reconstruct_builtin_owned(store, task, human_approve, None)
}

pub(super) fn reconstruct_builtin_owned(
    store: Arc<crate::store::Store>,
    task: &str,
    human_approve: bool,
    ownership: Option<Arc<crate::residency::lifecycle::Ownership>>,
) -> Result<Arc<Broker>> {
    crate::security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Builtin registry reconstruction requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    let path = store.runtime_lock.as_deref().ok_or_else(|| {
        anyhow::anyhow!("Builtin registry reconstruction requires a persistent store")
    })?;
    let _lease = if let Some(owner) = &ownership {
        owner.validate(&store)?;
        None
    } else {
        Some(crate::recovery::ExecutionLease::acquire(path, true)?)
    };
    let mut db = store.lock()?;
    let started = std::time::Instant::now();
    db.progress_handler(
        1000,
        Some(move || started.elapsed() > std::time::Duration::from_secs(5)),
    );
    let result = (|| -> Result<Arc<Broker>> {
        let tx = db.transaction()?;
        let (status, native): (String, bool) = tx.query_row(
            "SELECT status, EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='native_task_scope') FROM tasks WHERE id=?1",
            [task], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if !matches!(status.as_str(), "running" | "interrupted") || native != ownership.is_some() {
            bail!("Builtin reconstruction requires an unfinished task with matching native ownership scope")
        }
        let raw: String = tx.query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=65536 THEN payload END FROM events WHERE task_id=?1 AND kind='typed_module_snapshot' ORDER BY id LIMIT 1",
            [task], |row| row.get(0),
        )?;
        let snapshot: Snapshot =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 65536)?)?;
        if !matches!(snapshot.format_version, 2..=4) || snapshot.modules.len() != 1 {
            bail!("Builtin reconstruction requires one source-bound module snapshot")
        }
        let original: ModuleSnapshot = serde_json::from_value(snapshot.modules[0].clone())?;
        if original.source_sha256.as_deref()
            != Some(crate::escalation::empirical::runtime_sources_fingerprint())
        {
            bail!("Builtin worker sources differ from the original task")
        }
        let broker = Broker::reconstructed_builtin_owned(
            store.clone(),
            uuid::Uuid::parse_str(&snapshot.broker_id)?,
            ownership.clone(),
        )?;
        let compiled = broker.discover()?;
        if compiled.len() != 1
            || compiled[0].id != original.id
            || compiled[0].version != original.version
            || compiled[0]
                .capabilities
                .iter()
                .map(|c| c.as_str())
                .collect::<BTreeSet<_>>()
                != original
                    .capabilities
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
        {
            bail!("Original registry differs from the compiled builtin worker")
        }
        let restored = restore(&tx, task, Some(broker.clone()))?
            .ok_or_else(|| anyhow::anyhow!("Missing original builtin registry"))?;
        crate::storage::admit(&tx)?;
        let evidence = json!({"format_version":1,"source_sha256":original.source_sha256,
            "original_registry":restored.evidence(),"permissions_granted":false,
            "implementation_attested":false,"worker_dispatched":false,"resume_authorized":false});
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'builtin_registry_reconstruction','human-cli',?2,unixepoch())",
            rusqlite::params![task, evidence.to_string()])?;
        if started.elapsed() > std::time::Duration::from_secs(5) {
            bail!("Builtin registry reconstruction deadline exhausted")
        }
        tx.commit()?;
        Ok(broker)
    })();
    db.progress_handler(0, None::<fn() -> bool>);
    result
}

/// Reconstruct a version-4 Wasm expert registry from already compiled,
/// operator supplied manifests. Persisted task data is used only for comparing
/// immutable identities; it never supplies a path, artifact, or executable.
#[cfg(feature = "wasm-experts")]
pub(super) fn reconstruct_wasm(
    store: Arc<crate::store::Store>,
    task: &str,
    human_approve: bool,
    modules: Vec<crate::fabric::modules::wasm::WasmModule>,
) -> Result<Arc<Broker>> {
    crate::security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Wasm registry reconstruction requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    if modules.is_empty() || modules.len() > 32 {
        bail!("Wasm registry reconstruction module count exceeds bounds")
    }
    let path = store.runtime_lock.as_deref().ok_or_else(|| {
        anyhow::anyhow!("Wasm registry reconstruction requires a persistent store")
    })?;
    let _lease = crate::recovery::ExecutionLease::acquire(path, true)?;
    let mut db = store.lock()?;
    let started = std::time::Instant::now();
    db.progress_handler(
        1000,
        Some(move || started.elapsed() > std::time::Duration::from_secs(5)),
    );
    let result = (|| -> Result<Arc<Broker>> {
        let tx = db.transaction()?;
        let (status, native): (String, bool) = tx.query_row(
            "SELECT status, EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='native_task_scope') FROM tasks WHERE id=?1",
            [task], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if !matches!(status.as_str(), "running" | "interrupted") || native {
            bail!("Wasm registry reconstruction requires an unfinished nonnative task")
        }
        let mut statement = tx.prepare(
            "SELECT CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,
                    CASE WHEN length(CAST(payload AS BLOB))<=65536 THEN payload END,id
             FROM events WHERE task_id=?1 AND kind='typed_module_snapshot' ORDER BY id LIMIT 2",
        )?;
        let mut rows = statement.query([task])?;
        let row = rows
            .next()?
            .ok_or_else(|| anyhow::anyhow!("Original Wasm registry snapshot is missing"))?;
        let source: String = row.get(0)?;
        let raw: String = row.get(1)?;
        let snapshot_id: i64 = row.get(2)?;
        if rows.next()?.is_some() {
            bail!("Wasm registry reconstruction refuses duplicate snapshots")
        }
        drop(rows);
        drop(statement);
        let snapshot: Snapshot =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 65536)?)?;
        if source != "fabric"
            || snapshot.format_version != 4
            || snapshot.permissions_granted
            || snapshot.modules.is_empty()
            || snapshot.modules.len() != modules.len()
            || started.elapsed() > std::time::Duration::from_secs(5)
        {
            bail!("Wasm registry reconstruction requires one exact version-4 snapshot")
        }
        let identity = uuid::Uuid::parse_str(&snapshot.broker_id)?;

        // Match the whole supplied registry to saved identities and limits before
        // constructing the Broker; persisted data never supplies executable paths.
        let mut expected = BTreeMap::<(String, String), ModuleSnapshot>::new();
        for value in &snapshot.modules {
            let module: ModuleSnapshot = serde_json::from_value(value.clone())?;
            if module.source_sha256.is_some()
                || module.implementation.is_none()
                || module.limits.is_none()
                || expected
                    .insert((module.id.clone(), module.version.clone()), module)
                    .is_some()
            {
                bail!("Invalid or duplicate original Wasm module identity")
            }
        }
        let mut reconstructed = Vec::with_capacity(modules.len());
        for module in modules {
            let descriptor = module.descriptor();
            let recorded = expected
                .remove(&(descriptor.id.clone(), descriptor.version.clone()))
                .ok_or_else(|| {
                    anyhow::anyhow!("Supplied Wasm module was not in original registry")
                })?;
            let capabilities = descriptor
                .capabilities
                .iter()
                .map(|capability| capability.as_str())
                .collect::<BTreeSet<_>>();
            if capabilities
                != recorded
                    .capabilities
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
                || capabilities.len() != recorded.capabilities.len()
                || recorded.implementation.as_ref() != Some(module.implementation_identity())
                || recorded
                    .implementation
                    .as_ref()
                    .and_then(|identity| identity.get("runtime"))
                    .and_then(Value::as_str)
                    != Some("wasmi-typed-expert/v1")
            {
                bail!("Supplied Wasm implementation differs from the original snapshot")
            }
            reconstructed.push((
                module,
                recorded
                    .limits
                    .ok_or_else(|| anyhow::anyhow!("Original Wasm limits are missing"))?,
            ));
        }
        if !expected.is_empty() {
            bail!("Supplied Wasm registry is missing an original module")
        }
        // This checks registration validity and preserves the recorded broker UUID.
        let broker = Broker::reconstructed_wasm(store.clone(), identity, reconstructed)?;

        let restored = restore(&tx, task, Some(broker.clone()))?
            .ok_or_else(|| anyhow::anyhow!("Missing original Wasm registry"))?;
        crate::storage::admit(&tx)?;
        let evidence = json!({"format_version":1,"broker_id":snapshot.broker_id,
            "snapshot_event_id":snapshot_id,"snapshot_sha256":crate::store::sha256(raw.as_bytes()),
            "original_registry":restored.evidence(),"permissions_granted":false,
            "worker_dispatched":false,"resume_authorized":false,"implementation_attested":false});
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'wasm_registry_reconstruction','human-cli',?2,unixepoch())",
            rusqlite::params![task, evidence.to_string()])?;
        if started.elapsed() > std::time::Duration::from_secs(5) {
            bail!("Wasm registry reconstruction deadline exhausted")
        }
        tx.commit()?;
        Ok(broker)
    })();
    db.progress_handler(0, None::<fn() -> bool>);
    result
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
        || !matches!(snapshot.format_version, 1..=4)
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
        if snapshot.format_version == 1 && module.source_sha256.is_some() {
            bail!("Legacy module snapshot contains unsupported source identity")
        }
        if matches!(snapshot.format_version, 2..=4)
            && pin.source_fingerprint() != module.source_sha256.as_deref()
        {
            bail!("Restored module source identity differs from original snapshot")
        }
        if (snapshot.format_version < 3 && module.implementation.is_some())
            || (snapshot.format_version >= 3
                && module.implementation.as_ref() != pin.implementation_identity())
        {
            bail!("Restored module implementation identity differs from original snapshot")
        }
        if (snapshot.format_version < 4 && module.limits.is_some())
            || (snapshot.format_version == 4 && module.limits != Some(pin.limits()))
        {
            bail!("Restored module execution limits differ from original snapshot")
        }
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

    fn builtin_snapshot(broker: &Broker) -> Value {
        let descriptor = broker.discover().unwrap().pop().unwrap();
        json!({"format_version":2,"broker_id":broker.identity_id().to_string(),
            "modules":[{"id":descriptor.id,"version":descriptor.version,
            "capabilities":descriptor.capabilities.iter().map(|c|c.as_str()).collect::<Vec<_>>(),
            "source_sha256":crate::escalation::empirical::runtime_sources_fingerprint()}],
            "permissions_granted":false})
    }

    #[test]
    fn builtin_registry_reconstructs_after_reopening_without_dispatch_or_grants() {
        let root =
            std::env::temp_dir().join(format!("fabric-builtin-restart-{}", uuid::Uuid::new_v4()));
        let path = root.join("fabric.sqlite3");
        let store = Arc::new(Store::open(&path).unwrap());
        let task = store
            .create_task("Restore compiled worker", "standard")
            .unwrap();
        let original = Broker::reconstructed_builtin(store.clone(), uuid::Uuid::new_v4()).unwrap();
        let identity = original.identity_id();
        store
            .event(
                &task,
                "typed_module_snapshot",
                "fabric",
                builtin_snapshot(&original),
            )
            .unwrap();
        drop(original);
        drop(store);
        let store = Arc::new(Store::open(&path).unwrap());
        let restored = reconstruct_builtin(store.clone(), &task, true).unwrap();
        assert_eq!(restored.identity_id(), identity);
        assert_eq!(restored.discover().unwrap().len(), 1);
        let state = restore(&store.lock().unwrap(), &task, Some(restored.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(state.pins.len(), 6);
        let trace = store.task(&task).unwrap();
        let events = trace["events"].as_array().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e["kind"] == "builtin_registry_reconstruction")
                .count(),
            1
        );
        assert!(!events.iter().any(|e| matches!(
            e["kind"].as_str(),
            Some("module_dispatch_intent" | "module_process_launch" | "tool_reservation")
        )));
        drop(state);
        drop(restored);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn builtin_reconstruction_denies_unsafe_or_ambiguous_state_atomically() {
        for scenario in 0..14 {
            let root =
                std::env::temp_dir().join(format!("fabric-builtin-deny-{}", uuid::Uuid::new_v4()));
            let store = Arc::new(Store::open(&root.join("fabric.sqlite3")).unwrap());
            let task = store
                .create_task("Reject substituted worker", "standard")
                .unwrap();
            let broker =
                Broker::reconstructed_builtin(store.clone(), uuid::Uuid::new_v4()).unwrap();
            let mut snapshot = builtin_snapshot(&broker);
            match scenario {
                0 => snapshot["modules"][0]["source_sha256"] = json!("0".repeat(64)),
                1 => snapshot["modules"][0]["id"] = json!("host.custom"),
                2 => snapshot["modules"][0]["version"] = json!("2.0.0"),
                3 => snapshot["format_version"] = json!(1),
                4 => snapshot["permissions_granted"] = json!(true),
                5 => snapshot["modules"][0]["capabilities"] = json!(["math.evaluate"]),
                12 => {
                    snapshot["format_version"] = json!(3);
                    snapshot["modules"][0]["implementation"] = json!({"runtime":"guest"});
                }
                13 => snapshot["modules"][0]["implementation"] = json!({"runtime":"guest"}),
                _ => {}
            }
            store
                .event(&task, "typed_module_snapshot", "fabric", snapshot.clone())
                .unwrap();
            if scenario == 6 {
                store
                    .event(&task, "typed_module_snapshot", "fabric", snapshot)
                    .unwrap();
            }
            if scenario == 7 {
                store
                    .event(&task, "native_task_scope", "security-kernel", json!({}))
                    .unwrap();
            }
            if scenario == 8 {
                store
                    .event(&task, "module_dispatch_intent", "jev-plus", json!({}))
                    .unwrap();
            }
            if scenario == 9 {
                store.lock().unwrap().execute_batch("CREATE TRIGGER deny_reconstruction BEFORE INSERT ON events WHEN NEW.kind='builtin_registry_reconstruction' BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
            }
            let lease = if scenario == 11 {
                store.execution_lease().unwrap()
            } else {
                None
            };
            let before = store.task(&task).unwrap();
            assert!(
                reconstruct_builtin(store.clone(), &task, scenario != 10).is_err(),
                "scenario {scenario}"
            );
            assert_eq!(store.task(&task).unwrap(), before, "scenario {scenario}");
            drop(lease);
            drop(broker);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

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
                3 => snapshot["format_version"] = json!(5),
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
