//! Bounded joint capability coverage over one pinned declaration snapshot.
use crate::{models::Registered, registry::Manifest, store::Skill};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Serialize)]
pub(crate) struct Binding {
    pub requested: String,
    pub provider: Option<String>,
    pub provider_version: Option<String>,
    pub capability_version: Option<String>,
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
    pub unavailable: Vec<String>,
    pub unselected: Vec<String>,
}
struct Candidate<'a> {
    skill: &'a Skill,
    manifest: Option<&'a Manifest>,
    mask: u16,
    available: bool,
}
pub(crate) fn resolve(
    skills: &[Skill],
    manifests: &[Manifest],
    models: &BTreeMap<String, Registered>,
    requested: &[String],
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
        let mask = requested
            .iter()
            .enumerate()
            .fold(0u16, |mask, (i, capability)| {
                if skill.id.as_str() == capability.as_str()
                    || manifest.is_some_and(|m| {
                        m.capabilities
                            .iter()
                            .any(|c| c.id.as_str() == capability.as_str())
                    })
                {
                    mask | (1u16 << i)
                } else {
                    mask
                }
            });
        let available = !manifest.is_some_and(|m| {
            m.kind == "workflow" && m.model.is_some() && !models.contains_key(&m.id)
        });
        candidates.insert(
            skill.id.clone(),
            Candidate {
                skill,
                manifest,
                mask,
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
    let mut covered = chosen
        .iter()
        .filter_map(|id| candidates.get(id))
        .filter(|c| c.available)
        .fold(0, |mask, c| mask | c.mask);
    while refine && chosen.len() < 4 {
        let next = candidates
            .iter()
            .filter(|(id, c)| c.available && !chosen.contains(id) && c.mask & !covered != 0)
            .max_by(|(aid, a), (bid, b)| {
                (a.mask & !covered)
                    .count_ones()
                    .cmp(&(b.mask & !covered).count_ones())
                    .then_with(|| {
                        requested
                            .iter()
                            .any(|r| r.as_str() == a.skill.id)
                            .cmp(&requested.iter().any(|r| r.as_str() == b.skill.id))
                    })
                    .then_with(|| bid.cmp(aid))
            });
        let Some((id, candidate)) = next else { break };
        chosen.push(id.clone());
        covered |= candidate.mask;
    }
    let mut result = Resolution {
        skills: chosen,
        bindings: Vec::new(),
        missing: Vec::new(),
        unavailable: Vec::new(),
        unselected: Vec::new(),
    };
    for (i, requested) in requested.into_iter().enumerate() {
        let bit = 1u16 << i;
        let known = candidates
            .values()
            .filter(|c| c.mask & bit != 0)
            .collect::<Vec<_>>();
        let available = known.iter().any(|c| c.available);
        let provider = result
            .skills
            .iter()
            .filter_map(|id| candidates.get(id))
            .find(|c| c.available && c.mask & bit != 0)
            .or_else(|| known.iter().copied().find(|c| c.available))
            .or_else(|| known.first().copied());
        let selected = provider.is_some_and(|c| result.skills.contains(&c.skill.id));
        if known.is_empty() {
            result.missing.push(requested.clone());
        } else if !available {
            result.unavailable.push(requested.clone());
        } else if !selected {
            result.unselected.push(requested.clone());
        }
        let version = provider.and_then(|c| c.manifest).and_then(|m| {
            if m.id == *requested {
                Some(m.version.clone())
            } else {
                m.capabilities
                    .iter()
                    .find(|c| c.id == *requested)
                    .map(|c| c.version.clone())
            }
        });
        result.bindings.push(Binding {
            requested: requested.clone(),
            provider: provider.map(|c| c.skill.id.clone()),
            provider_version: provider.and_then(|c| c.manifest).map(|m| m.version.clone()),
            capability_version: version,
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
