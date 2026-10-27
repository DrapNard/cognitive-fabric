//! Frozen expert-to-module routing for typed operations.
//!
//! This catalog contains only compact declarations and task-opening module pins.
//! It does not load code, grant permissions, inspect weights, or dispatch work.

use crate::{
    fabric::modules::{Capability, Pin, TaskRoutes},
    registry::{self, Manifest, ProviderCapability},
    store::sha256,
};
use anyhow::{bail, Result};
use rusqlite::Connection;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const MAX_MANIFESTS: usize = 256;
const MAX_CATALOG_BYTES: usize = 1024 * 1024;

/// A caller-selected, immutable expert manifest version.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::agent) struct Selection {
    pub id: String,
    pub version: String,
}

impl Selection {
    pub(in crate::agent) fn validate(&self) -> Result<()> {
        if !registry::identifier(&self.id) {
            bail!("Expert selection has an invalid registry identifier")
        }
        if self.version.len() > 64 || Version::parse(&self.version)?.to_string() != self.version {
            bail!("Expert selection requires a canonical semantic version")
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
struct DeclaredCapability {
    id: String,
    version: String,
}

#[derive(Clone, Debug, Serialize)]
struct ProviderDeclaration {
    module_id: String,
    module_version: String,
    capabilities: Vec<String>,
}

/// Safe-to-audit registry metadata. No workflow advice, descriptions, prompts,
/// bodies, credentials, or model weights are retained here.
#[derive(Clone, Debug, Serialize)]
struct Metadata {
    id: String,
    version: String,
    capabilities: Vec<DeclaredCapability>,
    dependencies_present: bool,
    model_present: bool,
    provider: ProviderDeclaration,
    canonical_manifest_sha256: String,
}

struct Record {
    metadata: Metadata,
    pin: Option<Pin>,
}

pub(in crate::agent) struct Catalog {
    records: Vec<Record>,
    digest: String,
}

pub(in crate::agent) enum Route {
    Host,
    Bound { pin: Pin, audit: Value },
}

impl Catalog {
    /// Capture active manifest declarations and match them to exact modules from
    /// the same task-opening route snapshot. Resolution after capture is DB-free.
    pub(in crate::agent) fn capture(db: &Connection, routes: &TaskRoutes) -> Result<Self> {
        let mut manifests = registry::snapshot_from(db)?;
        if manifests.len() > MAX_MANIFESTS {
            bail!("Opening expert registry snapshot exceeds the manifest limit")
        }
        manifests.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));

        let mut records = Vec::new();
        for manifest in manifests
            .into_iter()
            .filter(|manifest| manifest.provider.is_some())
        {
            let Some(provider) = manifest.provider.as_ref() else {
                continue;
            };
            let canonical = serde_json::to_vec(&manifest)?;
            let metadata = Metadata {
                id: manifest.id.clone(),
                version: manifest.version.clone(),
                capabilities: manifest
                    .capabilities
                    .iter()
                    .map(|capability| DeclaredCapability {
                        id: capability.id.clone(),
                        version: capability.version.clone(),
                    })
                    .collect(),
                dependencies_present: !manifest.dependencies.is_empty(),
                model_present: manifest.model.is_some(),
                provider: ProviderDeclaration {
                    module_id: provider.module_id.clone(),
                    module_version: provider.module_version.clone(),
                    capabilities: provider
                        .capabilities
                        .iter()
                        .map(|capability| capability.as_str().to_owned())
                        .collect(),
                },
                canonical_manifest_sha256: sha256(&canonical),
            };
            let pin = exact_opening_pin(&manifest, provider.capabilities.first().copied(), routes);
            records.push(Record { metadata, pin });
        }

        let encoded = records
            .iter()
            .map(|record| &record.metadata)
            .collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&encoded)?;
        if bytes.len() > MAX_CATALOG_BYTES {
            bail!("Opening provider catalog metadata exceeds its size limit")
        }
        let digest = sha256(&bytes);
        Ok(Self { records, digest })
    }

    pub(in crate::agent) fn digest(&self) -> &str {
        &self.digest
    }

    pub(in crate::agent) fn len(&self) -> usize {
        self.records.len()
    }

    /// Resolve solely against the immutable catalog captured at task opening.
    /// A declared but unavailable or unsupported provider never falls back to a
    /// host implementation.
    pub(in crate::agent) fn resolve(
        &self,
        capability: Capability,
        expert: Option<&Selection>,
        requirement: Option<&str>,
    ) -> Result<Route> {
        if let Some(selection) = expert {
            selection.validate()?;
        }
        let requirement = requirement.map(parse_requirement).transpose()?;

        if let Some(selection) = expert {
            let record = self
                .records
                .iter()
                .find(|record| {
                    record.metadata.id == selection.id
                        && record.metadata.version == selection.version
                })
                .ok_or_else(|| {
                    anyhow::anyhow!("Selected expert version is not in the opening catalog")
                })?;
            if !record_covers(record, capability, requirement.as_ref()) {
                bail!("Selected expert does not declare a compatible capability version")
            }
            return bound(record, capability);
        }

        let declared = self
            .records
            .iter()
            .any(|record| record_covers(record, capability, requirement.as_ref()));
        let chosen = self.records.iter().find(|record| {
            record_covers(record, capability, requirement.as_ref()) && eligible(record)
        });

        match (requirement, declared, chosen) {
            (Some(_), _, Some(record)) => bound(record, capability),
            (Some(_), _, None) => {
                bail!("No opening provider satisfies the requested capability version")
            }
            (None, true, Some(record)) => bound(record, capability),
            (None, true, None) => {
                bail!("Declared providers for this capability are unavailable or unsupported")
            }
            (None, false, _) => Ok(Route::Host),
        }
    }
}

fn parse_requirement(requirement: &str) -> Result<VersionReq> {
    if requirement.is_empty() || requirement.len() > 64 {
        bail!("Capability version requirement must contain 1 to 64 bytes")
    }
    Ok(VersionReq::parse(requirement)?)
}

fn record_covers(
    record: &Record,
    capability: Capability,
    requirement: Option<&VersionReq>,
) -> bool {
    let cap_name = provider_capability(capability).as_str();
    if !record
        .metadata
        .provider
        .capabilities
        .iter()
        .any(|name| name == cap_name)
    {
        return false;
    }
    let Some(declared) = record
        .metadata
        .capabilities
        .iter()
        .find(|declared| declared.id == cap_name)
    else {
        return false;
    };
    requirement.is_none_or(|req| {
        Version::parse(&declared.version).is_ok_and(|version| req.matches(&version))
    })
}

fn eligible(record: &Record) -> bool {
    !record.metadata.dependencies_present && !record.metadata.model_present && record.pin.is_some()
}

fn bound(record: &Record, capability: Capability) -> Result<Route> {
    if !eligible(record) {
        bail!("Selected provider requires unsupported dependencies, a model binding, or an unavailable exact module")
    }
    let pin = record
        .pin
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Eligible provider has no retained exact module"))?;
    let provider_cap = provider_capability(capability);
    let capability_version = record
        .metadata
        .capabilities
        .iter()
        .find(|declared| declared.id == provider_cap.as_str())
        .map(|declared| declared.version.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("Provider capability version is missing from frozen metadata")
        })?;
    Ok(Route::Bound {
        pin: pin.clone(),
        audit: json!({
            "expert_id": record.metadata.id,
            "expert_version": record.metadata.version,
            "canonical_manifest_sha256": record.metadata.canonical_manifest_sha256,
            "capability": provider_cap.as_str(),
            "capability_version": capability_version,
            "provider_module_id": record.metadata.provider.module_id,
            "provider_module_version": record.metadata.provider.module_version,
            "permissions_granted": false
        }),
    })
}

fn exact_opening_pin(
    manifest: &Manifest,
    required: Option<ProviderCapability>,
    routes: &TaskRoutes,
) -> Option<Pin> {
    let provider = manifest.provider.as_ref()?;
    if provider
        .capabilities
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .len()
        != provider.capabilities.len()
    {
        return None;
    }
    let required = required?;
    let pin = routes
        .get(&required.module_capability())?
        .exact(&provider.module_id, &provider.module_version)?;
    if registry::providers::binding_and_descriptor(manifest, pin.descriptor(), required).is_err() {
        return None;
    }
    for declared in provider.capabilities.iter().copied() {
        let other = routes
            .get(&declared.module_capability())
            .and_then(|set| set.exact(&provider.module_id, &provider.module_version))?;
        if other.descriptor() != pin.descriptor() {
            return None;
        }
    }
    Some(pin.clone())
}

fn provider_capability(capability: Capability) -> ProviderCapability {
    match capability {
        Capability::MathEvaluate => ProviderCapability::MathEvaluate,
        Capability::SqlQuery => ProviderCapability::SqlQuery,
        Capability::SortingVerify => ProviderCapability::SortingVerify,
        Capability::OrderingVerify => ProviderCapability::OrderingVerify,
        Capability::PathVerify => ProviderCapability::PathVerify,
        Capability::ConstraintsSolve => ProviderCapability::ConstraintsSolve,
    }
}
