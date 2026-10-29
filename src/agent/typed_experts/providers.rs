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
use std::collections::{BTreeSet, HashMap, HashSet};

const MAX_MANIFESTS: usize = 256;
const MAX_CATALOG_BYTES: usize = 1024 * 1024;
const MAX_CATALOG_RECORD_BYTES: usize = MAX_CATALOG_BYTES;
const MAX_PLAN_OPERATIONS: usize = super::MAX_TYPED_OPERATIONS;
const MAX_PLAN_STATES: usize = 4096;
const MAX_ROUTE_AUDIT_BYTES: usize = 4096;

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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DeclaredCapability {
    id: String,
    version: String,
    #[serde(skip, default = "empty_version")]
    parsed_version: Version,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DeclaredDependency {
    id: String,
    requirement: String,
    #[serde(skip, default = "any_requirement")]
    parsed_requirement: VersionReq,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProviderDeclaration {
    module_id: String,
    module_version: String,
    capabilities: Vec<String>,
}

/// Safe-to-audit registry metadata. No workflow advice, descriptions, prompts,
/// bodies, credentials, or model weights are retained here.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    id: String,
    version: String,
    capabilities: Vec<DeclaredCapability>,
    dependencies: Vec<DeclaredDependency>,
    model_present: bool,
    provider: ProviderDeclaration,
    canonical_manifest_sha256: String,
}

struct Record {
    metadata: Metadata,
    parsed_version: Version,
    pin: Option<Pin>,
}

pub(in crate::agent) struct Catalog {
    records: Vec<Record>,
    digest: String,
}

#[derive(Clone)]
pub(in crate::agent) enum Route {
    Host,
    Bound { pin: Pin, audit: Value },
}

/// Host-only proof that a route was selected against the frozen catalog and
/// this exact task operation. It intentionally cannot be deserialized.
#[derive(Clone)]
pub(in crate::agent) struct PreparedRoute {
    catalog_digest: String,
    input_digest: String,
    route: Route,
    audit: Value,
}

impl PreparedRoute {
    pub(in crate::agent) fn audit_metadata(&self) -> Value {
        self.audit.clone()
    }
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
            let parsed_version = Version::parse(&manifest.version)?;
            let mut capabilities = Vec::with_capacity(manifest.capabilities.len());
            for capability in &manifest.capabilities {
                capabilities.push(DeclaredCapability {
                    id: capability.id.clone(),
                    version: capability.version.clone(),
                    parsed_version: Version::parse(&capability.version)?,
                });
            }
            let mut dependencies = Vec::with_capacity(manifest.dependencies.len());
            for dependency in &manifest.dependencies {
                dependencies.push(DeclaredDependency {
                    id: dependency.id.clone(),
                    requirement: dependency.requirement.clone(),
                    parsed_requirement: VersionReq::parse(&dependency.requirement)?,
                });
            }
            let metadata = Metadata {
                id: manifest.id.clone(),
                version: manifest.version.clone(),
                capabilities,
                dependencies,
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
            records.push(Record {
                metadata,
                parsed_version,
                pin,
            });
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

    /// Export the frozen provider declarations as bounded canonical metadata.
    /// Workflow bodies, descriptions, credentials, and model data are excluded.
    pub(in crate::agent) fn export_metadata(&self) -> Result<Vec<u8>> {
        let encoded = serde_json::to_vec(
            &self
                .records
                .iter()
                .map(|record| &record.metadata)
                .collect::<Vec<_>>(),
        )?;
        if encoded.len() > MAX_CATALOG_BYTES || sha256(&encoded) != self.digest {
            bail!("Frozen provider catalog metadata failed its integrity bound")
        }
        Ok(encoded)
    }

    /// Restore only the original frozen provider metadata and exact task pins.
    /// This never consults the active expert registry or chooses a replacement.
    pub(in crate::agent) fn restore_metadata(raw: &[u8], routes: &TaskRoutes) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_CATALOG_BYTES {
            bail!("Frozen provider catalog metadata exceeds its size limit")
        }
        let mut metadata: Vec<Metadata> = serde_json::from_slice(raw)?;
        if metadata.len() > MAX_MANIFESTS {
            bail!("Frozen provider catalog exceeds its record limit")
        }
        let canonical = serde_json::to_vec(&metadata)?;
        if canonical != raw || canonical.len() > MAX_CATALOG_RECORD_BYTES {
            bail!("Frozen provider catalog is not canonical")
        }
        let digest = sha256(&canonical);
        let mut records = Vec::with_capacity(metadata.len());
        let mut previous: Option<(String, String)> = None;
        for mut item in metadata.drain(..) {
            if !registry::identifier(&item.id)
                || item.version.len() > 64
                || Version::parse(&item.version)?.to_string() != item.version
                || previous
                    .as_ref()
                    .is_some_and(|key| key >= &(item.id.clone(), item.version.clone()))
            {
                bail!("Frozen provider catalog identity is invalid or unsorted")
            }
            previous = Some((item.id.clone(), item.version.clone()));
            let parsed_version = Version::parse(&item.version)?;
            if item.capabilities.len() > 16 {
                bail!("Frozen provider capability list is outside bounds")
            }
            let mut capability_ids = BTreeSet::new();
            for declared in &mut item.capabilities {
                if !registry::identifier(&declared.id)
                    || declared.id == item.id
                    || declared.version.is_empty()
                    || declared.version.len() > 64
                    || !capability_ids.insert(declared.id.clone())
                {
                    bail!("Frozen provider capability declaration is invalid")
                }
                declared.parsed_version = Version::parse(&declared.version)?;
                if declared.parsed_version.to_string() != declared.version {
                    bail!("Frozen provider capability version is not canonical")
                }
            }
            if item.dependencies.len() > 16 {
                bail!("Frozen provider dependencies exceed bounds")
            }
            let mut dependency_ids = BTreeSet::new();
            for dependency in &mut item.dependencies {
                if !registry::identifier(&dependency.id)
                    || dependency.requirement.is_empty()
                    || dependency.requirement.len() > 64
                    || !dependency_ids.insert(dependency.id.clone())
                {
                    bail!("Frozen provider dependency declaration is invalid")
                }
                dependency.parsed_requirement = VersionReq::parse(&dependency.requirement)?;
            }
            validate_sha256(&item.canonical_manifest_sha256)?;
            let provider = &item.provider;
            if !registry::identifier(&provider.module_id)
                || provider.module_version.len() > 64
                || Version::parse(&provider.module_version)?.to_string() != provider.module_version
                || provider.capabilities.is_empty()
                || provider.capabilities.len() > 8
            {
                bail!("Frozen provider module declaration is invalid")
            }
            let mut declared_provider_caps = BTreeSet::new();
            for name in &provider.capabilities {
                if !declared_provider_caps.insert(name.as_str())
                    || !item.capabilities.iter().any(|c| c.id == *name)
                {
                    bail!("Frozen provider capability binding is inconsistent")
                }
            }
            let pin = restore_exact_pin(&item, routes);
            records.push(Record {
                metadata: item,
                parsed_version,
                pin,
            });
        }
        Ok(Self { records, digest })
    }

    pub(in crate::agent) fn digest(&self) -> &str {
        &self.digest
    }

    pub(in crate::agent) fn len(&self) -> usize {
        self.records.len()
    }

    /// Resolve solely against the immutable catalog captured at task opening.
    /// This discovery API has no graph execution authority.
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

    /// Jointly bind every operation to one frozen route assignment. A manifest
    /// dependency can only be met by a directly declared graph prerequisite.
    pub(in crate::agent) fn bind_plan(
        &self,
        entries: &[&super::Entry],
        root_query_sha256: &str,
        task: &str,
    ) -> Result<Vec<PreparedRoute>> {
        validate_root_digest(root_query_sha256)?;
        let task_uuid = uuid::Uuid::parse_str(task)
            .map_err(|_| anyhow::anyhow!("Prepared provider routes require a task UUID"))?;
        let task = task_uuid.to_string();
        if entries.is_empty() || entries.len() > MAX_PLAN_OPERATIONS {
            bail!("Provider route plan must contain between one and eight operations")
        }

        let mut ids = HashMap::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            if entry.id.is_empty()
                || entry.id.len() > 64
                || ids.insert(entry.id.as_str(), index).is_some()
            {
                bail!("Provider route plan operation IDs must be valid and distinct")
            }
        }
        let direct_dependencies = entries
            .iter()
            .enumerate()
            .map(|(entry_index, entry)| {
                let dependencies = entry
                    .template
                    .as_ref()
                    .map(|template| template.dependencies())
                    .unwrap_or_default();
                let mut indexes = Vec::with_capacity(dependencies.len());
                if dependencies.len() > MAX_PLAN_OPERATIONS - 1 {
                    bail!("Provider route graph exceeds the direct dependency bound")
                }
                let mut unique = HashSet::with_capacity(dependencies.len());
                for dependency in dependencies {
                    if !unique.insert(dependency.as_str()) {
                        bail!("Provider route graph repeats a direct dependency")
                    }
                    let index = *ids.get(dependency.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("Provider route graph references an unknown dependency")
                    })?;
                    if index == entry_index {
                        bail!("Provider route graph contains a self dependency")
                    }
                    indexes.push(index);
                }
                Ok(indexes)
            })
            .collect::<Result<Vec<_>>>()?;
        let order = topological_order(&direct_dependencies)?;

        let mut pools = Vec::with_capacity(entries.len());
        for entry in entries {
            let capability = entry.work.capability();
            let req = entry
                .capability_requirement
                .as_deref()
                .map(parse_requirement)
                .transpose()?;
            if let Some(selection) = &entry.expert {
                selection.validate()?;
            }
            let declared = self
                .records
                .iter()
                .any(|record| record_covers(record, capability, req.as_ref()));
            let mut candidates = Vec::new();
            if let Some(selection) = &entry.expert {
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
                if !record_covers(record, capability, req.as_ref()) {
                    bail!("Selected expert does not declare a compatible capability version")
                }
                if eligible(record) {
                    candidates.push(Some(self.record_index(record)?));
                } else {
                    bail!(
                        "Selected provider requires a model binding or an unavailable exact module"
                    )
                }
            } else {
                for (record_index, record) in self.records.iter().enumerate() {
                    if record_covers(record, capability, req.as_ref()) && eligible(record) {
                        candidates.push(Some(record_index));
                    }
                }
                if candidates.is_empty() {
                    if req.is_some() {
                        bail!("No opening provider satisfies the requested capability version")
                    }
                    if declared {
                        bail!(
                            "Declared providers for this capability are unavailable or unsupported"
                        )
                    }
                    if entry.expert.is_none() && entry.capability_requirement.is_none() {
                        candidates.push(None);
                    } else {
                        bail!("An explicit provider selector cannot resolve to the host route")
                    }
                }
            }
            if candidates.len() > MAX_MANIFESTS {
                bail!("Provider candidate pool exceeds its manifest bound")
            }
            pools.push(candidates);
        }

        let mut assignment = vec![None; entries.len()];
        let mut state_count = 0usize;
        if !search_assignment(
            0,
            &order,
            &direct_dependencies,
            &pools,
            &self.records,
            &mut assignment,
            &mut state_count,
        )? {
            bail!("No bounded provider assignment satisfies the direct manifest dependencies")
        }

        let mut prepared = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let route = match assignment[index] {
                None => Route::Host,
                Some(record_index) => {
                    let record = &self.records[record_index];
                    let mut route = bound(record, entry.work.capability())?;
                    if let Route::Bound { audit, .. } = &mut route {
                        audit["dependency_operations"] = dependency_audit(
                            record,
                            &direct_dependencies[index],
                            entries,
                            &assignment,
                            &self.records,
                        )?;
                    }
                    route
                }
            };
            let input_digest = route_input_digest(&self.digest, &task, root_query_sha256, entry)?;
            let audit = match &route {
                Route::Host => json!({
                    "route": "host_registry",
                    "catalog_sha256": self.digest,
                    "permissions_granted": false,
                }),
                Route::Bound { audit, .. } => {
                    let mut value = audit.clone();
                    value["catalog_sha256"] = json!(self.digest);
                    value
                }
            };
            let mut audit = audit;
            audit["selection_search_states"] = json!(state_count);
            audit["selection_search_limit"] = json!(MAX_PLAN_STATES);
            if serde_json::to_vec(&audit)?.len() > MAX_ROUTE_AUDIT_BYTES {
                bail!("Prepared provider route audit exceeds its byte limit")
            }
            prepared.push(PreparedRoute {
                catalog_digest: self.digest.clone(),
                input_digest,
                route,
                audit,
            });
        }
        Ok(prepared)
    }

    /// Validate a prepared proof and return its frozen route without reselecting.
    pub(in crate::agent) fn route_for(
        &self,
        entry: &super::Entry,
        root_query_sha256: &str,
        task: &str,
        proof: &PreparedRoute,
    ) -> Result<Route> {
        validate_root_digest(root_query_sha256)?;
        let task_uuid = uuid::Uuid::parse_str(task)
            .map_err(|_| anyhow::anyhow!("Prepared provider routes require a task UUID"))?;
        if proof.catalog_digest != self.digest
            || proof.input_digest
                != route_input_digest(
                    &self.digest,
                    &task_uuid.to_string(),
                    root_query_sha256,
                    entry,
                )?
        {
            bail!("Prepared provider route proof does not match this catalog and operation")
        }
        if serde_json::to_vec(&proof.audit)?.len() > MAX_ROUTE_AUDIT_BYTES {
            bail!("Prepared provider route audit exceeds its byte limit")
        }
        Ok(proof.route.clone())
    }

    fn record_index(&self, needle: &Record) -> Result<usize> {
        self.records
            .iter()
            .position(|record| std::ptr::eq(record, needle))
            .ok_or_else(|| anyhow::anyhow!("Selected provider is absent from its frozen catalog"))
    }
}

fn validate_root_digest(digest: &str) -> Result<()> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("Prepared provider routes require a lowercase SHA-256 root query digest")
    }
    Ok(())
}

fn route_input_digest(
    catalog_digest: &str,
    task: &str,
    root_query_sha256: &str,
    entry: &super::Entry,
) -> Result<String> {
    let dependencies = entry
        .template
        .as_ref()
        .map(|template| template.dependencies())
        .unwrap_or_default();
    let value = json!({
        "catalog_sha256": catalog_digest,
        "task": task,
        "root_query_sha256": root_query_sha256,
        "operation_id": entry.id,
        "capability": provider_capability(entry.work.capability()).as_str(),
        "expert_selector": entry.expert,
        "capability_requirement": entry.capability_requirement,
        "direct_dependency_ids": dependencies,
        "translated": entry.work.translated(),
    });
    Ok(sha256(&serde_json::to_vec(&value)?))
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
    requirement.is_none_or(|req| req.matches(&declared.parsed_version))
}

fn eligible(record: &Record) -> bool {
    !record.metadata.model_present && record.pin.is_some()
}

fn bound(record: &Record, capability: Capability) -> Result<Route> {
    if !eligible(record) {
        bail!("Selected provider requires a model binding or an unavailable exact module")
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
            "permissions_granted": false,
        }),
    })
}

fn topological_order(dependencies: &[Vec<usize>]) -> Result<Vec<usize>> {
    let mut state = vec![0u8; dependencies.len()];
    let mut order = Vec::with_capacity(dependencies.len());
    fn visit(
        node: usize,
        dependencies: &[Vec<usize>],
        state: &mut [u8],
        order: &mut Vec<usize>,
    ) -> Result<()> {
        if state[node] == 1 {
            bail!("Provider route graph contains a dependency cycle")
        }
        if state[node] == 2 {
            return Ok(());
        }
        state[node] = 1;
        for &dependency in &dependencies[node] {
            if dependency >= dependencies.len() || dependency == node {
                bail!("Provider route graph contains an invalid dependency")
            }
            visit(dependency, dependencies, state, order)?;
        }
        state[node] = 2;
        order.push(node);
        Ok(())
    }
    for node in 0..dependencies.len() {
        visit(node, dependencies, &mut state, &mut order)?;
    }
    Ok(order)
}

fn search_assignment(
    depth: usize,
    order: &[usize],
    direct_dependencies: &[Vec<usize>],
    pools: &[Vec<Option<usize>>],
    records: &[Record],
    assignment: &mut [Option<usize>],
    state_count: &mut usize,
) -> Result<bool> {
    if depth == order.len() {
        return Ok(true);
    }
    let operation = order[depth];
    for &candidate in &pools[operation] {
        *state_count = (*state_count).saturating_add(1);
        if *state_count > MAX_PLAN_STATES {
            bail!("Provider route search exceeded its 4096-state safety bound")
        }
        if let Some(record_index) = candidate {
            let record = &records[record_index];
            if !dependencies_satisfied(
                record,
                &direct_dependencies[operation],
                assignment,
                records,
            )? {
                continue;
            }
        }
        assignment[operation] = candidate;
        if search_assignment(
            depth + 1,
            order,
            direct_dependencies,
            pools,
            records,
            assignment,
            state_count,
        )? {
            return Ok(true);
        }
        assignment[operation] = None;
    }
    Ok(false)
}

fn dependencies_satisfied(
    record: &Record,
    direct_dependencies: &[usize],
    assignment: &[Option<usize>],
    records: &[Record],
) -> Result<bool> {
    for required in &record.metadata.dependencies {
        let matched = direct_dependencies.iter().copied().any(|operation| {
            let Some(prerequisite_record) = assignment[operation] else {
                return false;
            };
            let prerequisite = &records[prerequisite_record];
            prerequisite.metadata.id == required.id
                && required
                    .parsed_requirement
                    .matches(&prerequisite.parsed_version)
        });
        if !matched {
            return Ok(false);
        }
    }
    Ok(true)
}

fn dependency_audit(
    record: &Record,
    direct_dependencies: &[usize],
    entries: &[&super::Entry],
    assignment: &[Option<usize>],
    records: &[Record],
) -> Result<Value> {
    let mut result = Vec::with_capacity(record.metadata.dependencies.len());
    for required in &record.metadata.dependencies {
        let operation = direct_dependencies
            .iter()
            .copied()
            .find(|operation| {
                let Some(prerequisite_record) = assignment[*operation] else {
                    return false;
                };
                let prerequisite = &records[prerequisite_record];
                prerequisite.metadata.id == required.id
                    && required
                        .parsed_requirement
                        .matches(&prerequisite.parsed_version)
            })
            .ok_or_else(|| anyhow::anyhow!("Selected provider dependency proof is missing"))?;
        let selected = assignment[operation]
            .ok_or_else(|| anyhow::anyhow!("Selected provider dependency has no expert route"))?;
        let prerequisite = &records[selected];
        result.push(json!({
            "operation_id": entries[operation].id,
            "expert_id": prerequisite.metadata.id,
            "expert_version": prerequisite.metadata.version,
            "required_version": required.requirement,
            "canonical_manifest_sha256": prerequisite.metadata.canonical_manifest_sha256,
        }));
    }
    Ok(Value::Array(result))
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

fn empty_version() -> Version {
    Version::new(0, 0, 0)
}

fn any_requirement() -> VersionReq {
    VersionReq::STAR
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("Frozen provider manifest digest is invalid")
    }
    Ok(())
}

fn parse_provider_capability(value: &str) -> Result<ProviderCapability> {
    match value {
        "math.evaluate" => Ok(ProviderCapability::MathEvaluate),
        "sql.query" => Ok(ProviderCapability::SqlQuery),
        "sorting.verify" => Ok(ProviderCapability::SortingVerify),
        "ordering.verify" => Ok(ProviderCapability::OrderingVerify),
        "path.verify" => Ok(ProviderCapability::PathVerify),
        "constraints.solve" => Ok(ProviderCapability::ConstraintsSolve),
        _ => bail!("Frozen provider capability is unsupported"),
    }
}

fn descriptor_capabilities(pin: &Pin) -> BTreeSet<String> {
    pin.descriptor()
        .capabilities
        .iter()
        .map(|capability| capability.as_str().to_owned())
        .collect()
}

fn declared_provider_capabilities(provider: &ProviderDeclaration) -> BTreeSet<String> {
    provider.capabilities.iter().cloned().collect()
}

fn restore_exact_pin(metadata: &Metadata, routes: &TaskRoutes) -> Option<Pin> {
    let provider = &metadata.provider;
    let expected = declared_provider_capabilities(provider);
    let mut selected: Option<Pin> = None;
    for name in &provider.capabilities {
        let capability = parse_provider_capability(name).ok()?;
        let pin = routes
            .get(&capability.module_capability())?
            .exact(&provider.module_id, &provider.module_version)?;
        if descriptor_capabilities(pin) != expected {
            return None;
        }
        if selected
            .as_ref()
            .is_some_and(|previous| previous.descriptor() != pin.descriptor())
        {
            return None;
        }
        selected = Some(pin.clone());
    }
    selected
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
