//! Bounded joint capability coverage over one pinned declaration snapshot.
use crate::{models::Registered, registry::Manifest, store::Skill};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
mod cover;
#[derive(Debug, Serialize)]
pub(crate) struct Binding {
    pub requested: String,
    pub provider: Option<String>,
    pub provider_version: Option<String>,
    pub capability_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_requirement: Option<String>,
    pub version_compatible: bool,
    pub selected: bool,
    pub available: bool,
    pub declaration_only: bool,
    pub permissions_granted: bool,
}
#[derive(Debug, Serialize)]
pub(crate) struct Resolution {
    pub skills: Vec<String>,
    pub bindings: Vec<Binding>,
    pub missing: Vec<String>,
    pub incompatible: Vec<String>,
    pub excluded_version_routes: Vec<String>,
    pub unavailable: Vec<String>,
    pub unselected: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_search: Option<cover::CoverageSearch>,
}
struct Candidate<'a> {
    skill: &'a Skill,
    manifest: Option<&'a Manifest>,
    mask: u16,
    declared: u16,
    available: bool,
}
pub(crate) fn version_requirements(
    ids: &[String],
    versions: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, semver::VersionReq>> {
    if versions.len() > 16 || versions.keys().any(|id| !ids.contains(id)) {
        bail!("Version requirements must reference requested capabilities")
    }
    versions
        .iter()
        .map(|(id, requirement)| {
            if requirement.is_empty() || requirement.len() > 64 {
                bail!("Capability version requirement exceeds bounds")
            }
            Ok((id.clone(), semver::VersionReq::parse(requirement)?))
        })
        .collect()
}
fn offered_version<'a>(manifest: Option<&'a Manifest>, id: &str) -> Option<&'a str> {
    manifest.and_then(|m| {
        if m.id == id {
            Some(m.version.as_str())
        } else {
            m.capabilities
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.version.as_str())
        }
    })
}
pub(crate) fn resolve(
    skills: &[Skill],
    manifests: &[Manifest],
    models: &BTreeMap<String, Registered>,
    requested: &[String],
    versions: &BTreeMap<String, String>,
    selected: &[String],
    refine: bool,
) -> Result<Resolution> {
    if skills.len() > crate::store::MAX_SKILLS
        || manifests.len() > crate::store::MAX_SKILLS
        || requested.len() > 16
        || selected.len() > 4
    {
        bail!("Capability resolution exceeds snapshot bounds")
    }
    let requested_set = requested.iter().collect::<BTreeSet<_>>();
    if requested_set.len() != requested.len()
        || requested.iter().any(|id| !crate::registry::identifier(id))
    {
        bail!("Invalid or duplicate capability requirement")
    }
    let versions_parsed = version_requirements(requested, versions)?;
    let requested = requested_set.into_iter().collect::<Vec<_>>();
    let manifest_count = manifests.len();
    let manifests = manifests
        .iter()
        .map(|m| (&m.id, m))
        .collect::<BTreeMap<_, _>>();
    if manifests.len() != manifest_count {
        bail!("Duplicate pinned manifest identity")
    }
    let mut candidates = BTreeMap::new();
    let mut all_ids = BTreeSet::new();
    for skill in skills {
        if !all_ids.insert(&skill.id) {
            bail!("Duplicate pinned skill identity")
        }
        if !skill.enabled {
            continue;
        }
        let manifest = manifests.get(&skill.id).copied();
        let mut mask = 0u16;
        let mut declared = 0u16;
        for (i, capability) in requested.iter().enumerate() {
            if skill.id.as_str() == capability.as_str()
                || manifest.is_some_and(|m| {
                    m.capabilities
                        .iter()
                        .any(|c| c.id.as_str() == capability.as_str())
                })
            {
                declared |= 1u16 << i;
                let compatible = match versions_parsed.get(capability.as_str()) {
                    None => true,
                    Some(requirement) => offered_version(manifest, capability)
                        .map(semver::Version::parse)
                        .transpose()?
                        .is_some_and(|v| requirement.matches(&v)),
                };
                if compatible {
                    mask |= 1u16 << i;
                }
            }
        }
        let available = !manifest.is_some_and(|m| {
            m.kind == "workflow" && m.model.is_some() && !models.contains_key(&m.id)
        });
        candidates.insert(
            skill.id.clone(),
            Candidate {
                skill,
                manifest,
                mask,
                declared,
                available,
            },
        );
    }
    let mut chosen = selected.to_vec();
    if chosen.iter().collect::<BTreeSet<_>>().len() != chosen.len()
        || chosen.iter().any(|id| !candidates.contains_key(id))
    {
        bail!("Invalid initial capability routes")
    }
    let mut excluded_version_routes = Vec::new();
    chosen.retain(|id| {
        let candidate = &candidates[id];
        if candidate.declared != 0 && candidate.mask == 0 {
            excluded_version_routes.push(id.clone());
            false
        } else {
            true
        }
    });
    let covered = chosen
        .iter()
        .filter_map(|id| candidates.get(id))
        .filter(|c| c.available)
        .fold(0, |mask, c| mask | c.mask);
    let coverage_search = if refine {
        Some(cover::select(&candidates, &requested, &mut chosen, covered))
    } else {
        None
    };
    let mut result = Resolution {
        skills: chosen,
        bindings: Vec::new(),
        missing: Vec::new(),
        incompatible: Vec::new(),
        excluded_version_routes,
        unavailable: Vec::new(),
        unselected: Vec::new(),
        coverage_search,
    };
    for (i, requested) in requested.into_iter().enumerate() {
        let bit = 1u16 << i;
        let known = candidates
            .values()
            .filter(|c| c.declared & bit != 0)
            .collect::<Vec<_>>();
        let compatible = known
            .iter()
            .copied()
            .filter(|c| c.mask & bit != 0)
            .collect::<Vec<_>>();
        let available = compatible.iter().any(|c| c.available);
        let provider = result
            .skills
            .iter()
            .filter_map(|id| candidates.get(id))
            .find(|c| c.available && c.mask & bit != 0)
            .or_else(|| compatible.iter().copied().find(|c| c.available))
            .or_else(|| compatible.first().copied())
            .or_else(|| known.first().copied());
        let version_compatible = provider.is_some_and(|c| c.mask & bit != 0);
        let selected =
            provider.is_some_and(|c| version_compatible && result.skills.contains(&c.skill.id));
        if known.is_empty() {
            result.missing.push(requested.clone());
        } else if compatible.is_empty() {
            result.incompatible.push(requested.clone());
        } else if !available {
            result.unavailable.push(requested.clone());
        } else if !selected {
            result.unselected.push(requested.clone());
        }
        let version =
            offered_version(provider.and_then(|c| c.manifest), requested).map(str::to_owned);
        result.bindings.push(Binding {
            requested: requested.clone(),
            provider: provider.map(|c| c.skill.id.clone()),
            provider_version: provider.and_then(|c| c.manifest).map(|m| m.version.clone()),
            capability_version: version,
            version_requirement: versions.get(requested.as_str()).cloned(),
            version_compatible,
            selected,
            available,
            declaration_only: true,
            permissions_granted: false,
        });
    }
    Ok(result)
}
#[cfg(test)]
mod tests;
