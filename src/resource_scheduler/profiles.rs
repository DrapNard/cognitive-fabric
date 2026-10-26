//! Immutable operator declarations for per-role model working allocations.
use super::*;
use std::collections::BTreeSet;

/// Operator-declared per-call admission for one role and local endpoint/model.
/// This is not measured physical usage or a provider/model identity attestation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkingProfile {
    pub id: String,
    pub target: crate::backend::ModelTarget,
    pub roles: Vec<String>,
    pub allocation: Allocation,
}

pub(super) fn canonical_working_profiles(
    profiles: &[WorkingProfile],
    capacity: Allocation,
    resident: Allocation,
) -> Result<Vec<WorkingProfile>> {
    if profiles.len() > 32 {
        bail!("Working profile catalog exceeds bounds")
    }
    let available = capacity.available(resident);
    let mut bytes = 0usize;
    for profile in profiles {
        if profile.id.len() > 64
            || profile.target.api_base.len() > 1000
            || profile.target.model.len() > 256
            || profile.roles.is_empty()
            || profile.roles.len() > crate::fabric::protocol::ROLES.len()
            || profile.roles.iter().any(|role| role.len() > 32)
        {
            bail!("Working profile exceeds field bounds")
        }
        bytes = bytes
            .checked_add(profile.id.len())
            .and_then(|value| value.checked_add(profile.target.api_base.len()))
            .and_then(|value| value.checked_add(profile.target.model.len()))
            .and_then(|value| {
                value.checked_add(profile.roles.iter().map(String::len).sum::<usize>())
            })
            .ok_or_else(|| anyhow!("Working profile catalog size overflow"))?;
        if bytes > 16_000 {
            bail!("Working profile catalog exceeds byte bounds")
        }
    }
    let mut canonical = profiles.to_vec();
    let mut ids = BTreeSet::new();
    let mut pairs = BTreeSet::new();
    for profile in &mut canonical {
        if !crate::registry::identifier(&profile.id) || !ids.insert(profile.id.clone()) {
            bail!("Invalid or duplicate working profile identity")
        }
        let target = profile.target.canonical()?;
        if target != profile.target {
            bail!("Working profile target must be canonical")
        }
        if profile.allocation.compute_slots == 0 || !profile.allocation.fits(available) {
            bail!("Working profile allocation cannot fit the resource pool")
        }
        profile.roles.sort();
        if profile.roles.windows(2).any(|pair| pair[0] == pair[1]) {
            bail!("Duplicate role in working profile")
        }
        for role in &profile.roles {
            crate::fabric::protocol::actor(role)?;
            if !pairs.insert((profile.target.clone(), role.clone())) {
                bail!("Overlapping working profile target and role")
            }
        }
    }
    canonical.sort_by(|left, right| left.id.cmp(&right.id));
    if serde_json::to_vec(&canonical)?.len() > 16_000 {
        bail!("Working profile catalog exceeds byte bounds")
    }
    Ok(canonical)
}

pub(super) fn select_profile(
    profiles: &[WorkingProfile],
    role: &str,
    target: &crate::backend::ModelTarget,
) -> Result<Option<WorkingProfile>> {
    let matches = profiles
        .iter()
        .filter(|profile| {
            profile.target == *target && profile.roles.iter().any(|item| item == role)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [profile] => Ok(Some((*profile).clone())),
        [] => Ok(None),
        _ => bail!("Ambiguous configured working profile"),
    }
}

impl ResourcePool {
    /// Conservative instantaneous declaration check, never a reservation or
    /// proof of provider concurrency. Dispatch must still acquire every lease.
    pub(crate) fn wave_fits(
        &self,
        configuration: &Configuration,
        routes: &[(&str, Option<crate::backend::InferenceTarget>)],
    ) -> Result<bool> {
        if routes.is_empty() || routes.len() > 4 || !self.matches(&self.store, configuration) {
            bail!("Invalid configured resource wave")
        }
        let mut total = Allocation::default();
        for (role, target) in routes {
            crate::fabric::protocol::actor(role)?;
            if !self.residents.is_empty() {
                self.resident_coverage(target.clone())?;
            }
            let allocation = if self.working_profiles.is_empty() {
                configuration.allocation
            } else {
                Self::profile_for(configuration, role, target.as_ref())?
                    .ok_or_else(|| anyhow!("Wave target has no configured working profile"))?
                    .allocation
            };
            total.compute_slots = total
                .compute_slots
                .checked_add(allocation.compute_slots)
                .ok_or_else(|| anyhow!("Wave compute allocation overflow"))?;
            total.host_memory_bytes = total
                .host_memory_bytes
                .checked_add(allocation.host_memory_bytes)
                .ok_or_else(|| anyhow!("Wave host allocation overflow"))?;
            total.device_memory_bytes = total
                .device_memory_bytes
                .checked_add(allocation.device_memory_bytes)
                .ok_or_else(|| anyhow!("Wave device allocation overflow"))?;
        }
        let state = self.lock()?;
        Ok(!state.closed
            && state.queue.is_empty()
            && total.fits(self.settings.capacity.available(state.used)))
    }

    pub(crate) fn profile_for(
        configuration: &Configuration,
        role: &str,
        target: Option<&crate::backend::InferenceTarget>,
    ) -> Result<Option<WorkingProfile>> {
        let (_, resident) =
            resident::canonical(&configuration.residents, configuration.settings.capacity)?;
        let profiles = canonical_working_profiles(
            &configuration.working_profiles,
            configuration.settings.capacity,
            resident,
        )?;
        if profiles.is_empty() {
            return Ok(None);
        }
        let target = match target {
            Some(crate::backend::InferenceTarget::LocalHttp(target)) => target.canonical()?,
            Some(crate::backend::InferenceTarget::Native(target)) => target.resource_target()?,
            Some(crate::backend::InferenceTarget::ManagedNative(target)) => {
                target.resource_target()?
            }
            Some(crate::backend::InferenceTarget::Deterministic) | None => {
                bail!("Configured working profiles require a known local model target")
            }
        };
        select_profile(&profiles, role, &target)
    }

    pub(super) fn find_working_profile(
        &self,
        role: &str,
        target: &crate::backend::ModelTarget,
    ) -> Result<WorkingProfile> {
        select_profile(&self.working_profiles, role, target)?
            .ok_or_else(|| anyhow!("No configured working profile covers this role and target"))
    }
}
