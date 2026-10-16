//! Immutable operator declarations for resident backbones and their consumers.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentConsumer {
    pub api_base: String,
    pub model: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentAllocation {
    pub id: String,
    pub version: String,
    /// Operator declaration, not a hash measured from a provider's loaded weights.
    pub weights_sha256: String,
    pub host_memory_bytes: u64,
    pub device_memory_bytes: u64,
    pub consumers: Vec<ResidentConsumer>,
}

pub(super) fn canonical(
    residents: &[ResidentAllocation],
    capacity: Allocation,
) -> Result<(Vec<ResidentAllocation>, Allocation)> {
    if residents.len() > 128 {
        bail!("Resident catalog exceeds bounds")
    }
    let mut raw_bytes = 0usize;
    for resident in residents {
        if resident.id.len() > 64
            || resident.version.len() > 64
            || resident.weights_sha256.len() != 64
            || resident.consumers.len() > 32
        {
            bail!("Resident declaration exceeds field bounds")
        }
        raw_bytes += resident.id.len() + resident.version.len() + resident.weights_sha256.len();
        for consumer in &resident.consumers {
            if consumer.api_base.len() > 1000 || consumer.model.len() > 256 {
                bail!("Resident consumer exceeds field bounds")
            }
            raw_bytes += consumer.api_base.len() + consumer.model.len();
        }
        if raw_bytes > 16_000 {
            bail!("Resident catalog exceeds byte bounds")
        }
    }
    let mut catalog = residents.to_vec();
    let mut identities = BTreeSet::new();
    let mut total = Allocation::default();
    let mut consumer_count = 0;
    for resident in &mut catalog {
        if resident.id.len() > 64
            || !crate::registry::identifier(&resident.id)
            || resident.version.len() > 64
            || semver::Version::parse(&resident.version)?.to_string() != resident.version
            || resident.weights_sha256.len() != 64
            || !resident
                .weights_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !identities.insert((resident.id.clone(), resident.version.clone()))
            || (resident.host_memory_bytes == 0 && resident.device_memory_bytes == 0)
            || resident.host_memory_bytes > capacity.host_memory_bytes
            || resident.device_memory_bytes > capacity.device_memory_bytes
            || resident.consumers.is_empty()
            || resident.consumers.len() > 32
        {
            bail!("Invalid resident backbone declaration")
        }
        consumer_count += resident.consumers.len();
        if consumer_count > 512 {
            bail!("Resident consumer capacity exhausted")
        }
        total.host_memory_bytes = total
            .host_memory_bytes
            .checked_add(resident.host_memory_bytes)
            .ok_or_else(|| anyhow!("Resident memory overflow"))?;
        total.device_memory_bytes = total
            .device_memory_bytes
            .checked_add(resident.device_memory_bytes)
            .ok_or_else(|| anyhow!("Resident memory overflow"))?;
        if !total.fits(capacity) {
            bail!("Resident allocations exceed pool capacity")
        }
        for consumer in &mut resident.consumers {
            if consumer.api_base.len() > 1000
                || consumer.model.trim().is_empty()
                || consumer.model.len() > 256
                || consumer.model.chars().any(char::is_control)
            {
                bail!("Invalid resident model consumer")
            }
            // Canonical literal-loopback base, without turning an observation into
            // provider contact. A trailing slash/localhost alias cannot evade dedup.
            let mut endpoint = crate::backend::local_endpoint(&consumer.api_base)?;
            let path = endpoint
                .path()
                .strip_suffix("/chat/completions")
                .unwrap()
                .to_string();
            endpoint.set_path(path.trim_end_matches('/'));
            consumer.api_base = endpoint.to_string().trim_end_matches('/').to_string();
        }
        resident.consumers.sort();
        if resident.consumers.windows(2).any(|pair| pair[0] == pair[1]) {
            bail!("Duplicate resident consumer")
        }
    }
    catalog.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
    if serde_json::to_vec(&catalog)?.len() > 16_000 {
        bail!("Resident catalog exceeds byte bounds")
    }
    Ok((catalog, total))
}

#[cfg(test)]
#[path = "resident_tests.rs"]
mod tests;
