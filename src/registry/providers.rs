//! Bind immutable expert manifest versions to exact host-registered typed modules.
//!
//! This adapter only selects and pins a descriptor. The host must still provide an
//! independent `modules::Grant` and the original `TaskPolicy` to dispatch work.
use super::{verified_manifest, Manifest, ProviderBinding, ProviderCapability};
use crate::{
    fabric::modules::{Broker, Descriptor, Pin},
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use serde_json::{json, Value};

fn stored_manifest_sha256(
    store: &Store,
    manifest: &Manifest,
    require_active: bool,
) -> Result<String> {
    manifest.validate()?;
    let db = store.lock()?;
    let (raw, checksum, active, enabled): (String, String, bool, bool) = db.query_row(
        "SELECT CASE WHEN length(CAST(m.manifest AS BLOB))<=64000 THEN m.manifest END, CASE WHEN length(CAST(m.checksum AS BLOB))=64 THEN m.checksum END, m.active, s.enabled FROM expert_manifests m JOIN skills s ON s.id=m.skill_id WHERE m.skill_id=?1 AND m.version=?2",
        rusqlite::params![manifest.id, manifest.version],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if require_active && (!active || !enabled) {
        bail!("Expert manifest version is not active and enabled for a new task pin")
    }
    let stored = verified_manifest(raw.clone(), checksum.clone())?;
    if serde_json::to_string(&stored)? != serde_json::to_string(manifest)? {
        bail!("Expert provider binding differs from its immutable stored manifest")
    }
    if sha256(raw.as_bytes()) != checksum {
        bail!("Expert manifest integrity check failed")
    }
    Ok(checksum)
}

fn require_running_task(store: &Store, task_id: &str) -> Result<()> {
    let status: String = store.lock()?.query_row(
        "SELECT CASE WHEN length(CAST(status AS BLOB))<=64 THEN status END FROM tasks WHERE id=?1",
        [task_id],
        |row| row.get(0),
    )?;
    if status != "running" {
        bail!("Typed expert providers can only be pinned for a running task")
    }
    Ok(())
}

fn binding_and_descriptor<'a>(
    manifest: &'a Manifest,
    descriptor: &'a Descriptor,
    required_capability: ProviderCapability,
) -> Result<&'a ProviderBinding> {
    let binding = manifest
        .provider
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Expert manifest has no typed provider binding"))?;
    if !binding.capabilities.contains(&required_capability) {
        bail!("Requested capability is not declared by this expert provider binding")
    }
    let declared = binding
        .capabilities
        .iter()
        .map(|capability| capability.module_capability())
        .collect::<std::collections::BTreeSet<_>>();
    if descriptor.id != binding.module_id
        || descriptor.version != binding.module_version
        || descriptor.capabilities != declared
        || !descriptor
            .capabilities
            .contains(&required_capability.module_capability())
    {
        bail!("Typed module descriptor does not match the immutable expert binding")
    }
    Ok(binding)
}

fn snapshot_event(store: &Store, task_id: &str, kind: &str, source: &str) -> Result<Value> {
    let db = store.lock()?;
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END, CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END FROM events WHERE task_id=?1 AND kind=?2 ORDER BY id LIMIT 2",
    )?;
    let mut rows = statement.query(rusqlite::params![task_id, kind])?;
    let first = rows
        .next()?
        .ok_or_else(|| anyhow::anyhow!("Required task snapshot audit is missing"))?;
    let raw: String = first.get(0)?;
    let observed_source: String = first.get(1)?;
    if observed_source != source || rows.next()?.is_some() {
        bail!("Task snapshot audit is duplicated or has an unexpected source")
    }
    Ok(serde_json::from_str(&raw)?)
}

fn verify_task_route(store: &Store, task_id: &str, manifest: &Manifest) -> Result<()> {
    let route = snapshot_event(store, task_id, "route", "jev-plus")?;
    let skills = route["skills"]
        .as_array()
        .filter(|skills| skills.len() <= crate::routing::MAX_ROUTING_SKILLS)
        .ok_or_else(|| anyhow::anyhow!("Task route audit has an invalid skill set"))?;
    if skills
        .iter()
        .filter(|id| id.as_str() == Some(&manifest.id))
        .count()
        != 1
    {
        bail!("Expert was not selected by the task's audited route")
    }
    let versions = route["versions"]
        .as_array()
        .filter(|versions| versions.len() <= crate::routing::MAX_ROUTING_SKILLS)
        .ok_or_else(|| anyhow::anyhow!("Task route audit has invalid pinned skill versions"))?;
    if versions
        .iter()
        .filter(|skill| {
            skill["id"].as_str() == Some(&manifest.id)
                && skill["manifest_version"].as_str() == Some(&manifest.version)
                && skill["enabled"].as_bool() == Some(true)
        })
        .count()
        != 1
    {
        bail!("Task route audit does not pin this expert manifest version")
    }
    let raw_manifest = serde_json::to_value(manifest)?;
    let manifests = route["manifests"]
        .as_array()
        .filter(|manifests| manifests.len() <= crate::routing::MAX_ROUTING_SKILLS)
        .ok_or_else(|| anyhow::anyhow!("Task route audit has invalid manifest snapshots"))?;
    if manifests
        .iter()
        .filter(|saved| *saved == &raw_manifest)
        .count()
        != 1
    {
        bail!("Task route audit does not contain this exact expert manifest")
    }
    Ok(())
}

fn verify_task_module_snapshot(
    store: &Store,
    task_id: &str,
    broker: &Broker,
    binding: &ProviderBinding,
    pin: &Pin,
) -> Result<()> {
    let descriptor = pin.descriptor();
    let snapshot = snapshot_event(store, task_id, "typed_module_snapshot", "fabric")?;
    let broker_id = broker.identity_id().to_string();
    if !matches!(snapshot["format_version"].as_u64(), Some(1 | 2))
        || snapshot["permissions_granted"].as_bool() != Some(false)
        || snapshot["broker_id"].as_str() != Some(broker_id.as_str())
    {
        bail!("Typed module snapshot audit has an invalid version, permission marker, or Broker identity")
    }
    let modules = snapshot["modules"]
        .as_array()
        .filter(|modules| modules.len() <= 32)
        .ok_or_else(|| anyhow::anyhow!("Typed module snapshot audit exceeds bounds"))?;
    let matches = modules
        .iter()
        .filter(|module| {
            module["id"].as_str() == Some(&binding.module_id)
                && module["version"].as_str() == Some(&binding.module_version)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!("Task module snapshot does not contain the exact provider version")
    }
    if snapshot["format_version"] == 2
        && matches[0]["source_sha256"] != serde_json::to_value(pin.source_fingerprint())?
    {
        bail!("Pinned provider source identity differs from task snapshot")
    }
    let capabilities = matches[0]["capabilities"]
        .as_array()
        .filter(|capabilities| !capabilities.is_empty() && capabilities.len() <= 8)
        .ok_or_else(|| anyhow::anyhow!("Task module snapshot has invalid provider capabilities"))?;
    let observed = capabilities
        .iter()
        .map(|capability| {
            let name = capability
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Task module snapshot capability is invalid"))?;
            match name {
                "math.evaluate" => Ok(ProviderCapability::MathEvaluate),
                "sql.query" => Ok(ProviderCapability::SqlQuery),
                "sorting.verify" => Ok(ProviderCapability::SortingVerify),
                "ordering.verify" => Ok(ProviderCapability::OrderingVerify),
                "path.verify" => Ok(ProviderCapability::PathVerify),
                "constraints.solve" => Ok(ProviderCapability::ConstraintsSolve),
                _ => bail!("Task module snapshot contains an unknown capability"),
            }
        })
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    let declared = binding
        .capabilities
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if observed.len() != capabilities.len() || observed != declared {
        bail!("Task module snapshot capability set differs from the expert binding")
    }
    if descriptor.id != binding.module_id || descriptor.version != binding.module_version {
        bail!("Pinned module differs from the task module snapshot")
    }
    Ok(())
}

fn record_provider_snapshot(
    store: &Store,
    manifest: &Manifest,
    manifest_sha256: &str,
    broker: &Broker,
    pin: &Pin,
    required_capability: ProviderCapability,
) -> Result<()> {
    let descriptor = pin.descriptor();
    let capabilities = descriptor
        .capabilities
        .iter()
        .map(|capability| capability.as_str())
        .collect::<Vec<_>>();
    store.event(
        pin.task(),
        "expert_provider_snapshot",
        "jev-plus",
        json!({
            "format_version":1,
            "expert_id":manifest.id,
            "expert_version":manifest.version,
            "manifest_sha256":manifest_sha256,
            "module_id":descriptor.id,
            "module_version":descriptor.version,
            "broker_id":broker.identity_id().to_string(),
            "capabilities":capabilities,
            "required_capability":required_capability.as_str(),
            "provider_permissions_granted":false,
            "task_grant_created":false
        }),
    )?;
    Ok(())
}

/// Pin the typed provider declared by one immutable expert manifest version.
/// The exact provider capability set and the stored manifest contents are checked
/// before a bounded snapshot event is committed. A failed audit returns no pin.
pub fn pin_provider(
    store: &Store,
    manifest: &Manifest,
    broker: &Broker,
    task_id: &str,
    required_capability: ProviderCapability,
) -> Result<Pin> {
    uuid::Uuid::parse_str(task_id)?;
    require_running_task(store, task_id)?;
    let manifest_sha256 = stored_manifest_sha256(store, manifest, true)?;
    let binding = manifest
        .provider
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Expert manifest has no typed provider binding"))?;

    // Broker::pin resolves only an exact active ID/version. The returned handle
    // retains its implementation across later route replacement or deactivation.
    let pin = broker.pin(task_id, &binding.module_id, &binding.module_version)?;
    binding_and_descriptor(manifest, pin.descriptor(), required_capability)?;
    record_provider_snapshot(
        store,
        manifest,
        &manifest_sha256,
        broker,
        &pin,
        required_capability,
    )?;
    Ok(pin)
}

/// Pin a provider from a manifest already present in the task's route snapshot.
/// Unlike `pin_provider`, this preserves a task's admitted expert/module versions
/// across subsequent operator swaps. The snapshot can select versions only: the
/// broker still requires a separate task grant and policy at dispatch.
pub fn pin_task_snapshot_provider(
    store: &Store,
    manifest: &Manifest,
    broker: &Broker,
    pin: &Pin,
    required_capability: ProviderCapability,
) -> Result<Pin> {
    if !broker.owns_pin(pin) {
        bail!("Task provider pin was issued by another Broker")
    }
    uuid::Uuid::parse_str(pin.task())?;
    require_running_task(store, pin.task())?;
    let manifest_sha256 = stored_manifest_sha256(store, manifest, false)?;
    let binding = binding_and_descriptor(manifest, pin.descriptor(), required_capability)?;
    verify_task_route(store, pin.task(), manifest)?;
    verify_task_module_snapshot(store, pin.task(), broker, binding, pin)?;
    record_provider_snapshot(
        store,
        manifest,
        &manifest_sha256,
        broker,
        pin,
        required_capability,
    )?;
    Ok(pin.clone())
}
