//! Frozen full-registry catalog selection for the initial language interpretation.
use anyhow::{bail, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use super::*;

const MAX_EXPERT_CATALOG_ENTRIES: usize = 256;
const MAX_EXPERT_CATALOG_BYTES: usize = 16 * 1024 * 1024;

/// Opening-snapshot registry metadata retained only by the active client
/// session. The catalog itself is never serialized into the prompt or state.
pub(in crate::agent) struct ExpertCatalog {
    digest: String,
    entries: Vec<ExpertRecord>,
}

impl ExpertCatalog {
    pub(in crate::agent) fn digest(&self) -> &str {
        &self.digest
    }

    pub(in crate::agent) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Freeze the full active manifest catalog and exact registered-model
/// snapshots using the caller's opening database transaction.
pub(in crate::agent) fn capture_catalog(
    db: &Connection,
    context: Option<&crate::store::TaskContext>,
) -> Result<Option<ExpertCatalog>> {
    let Some(context) = context else {
        return Ok(None);
    };
    let mut manifests = context.manifests.iter().collect::<Vec<_>>();
    manifests.sort_by(|left, right| left.id.cmp(&right.id));
    if manifests.windows(2).any(|pair| pair[0].id == pair[1].id) {
        bail!("Opening expert catalog contains duplicate manifest IDs")
    }
    if manifests.len() > MAX_EXPERT_CATALOG_ENTRIES {
        return Ok(None);
    }

    let mut entries = Vec::with_capacity(manifests.len());
    let mut encoded_bytes = 2usize;
    let mut digest = Sha256::new();
    digest.update(b"[");
    for manifest in manifests {
        let record = ExpertRecord::capture(db, manifest)?;
        let encoded_record = serde_json::to_vec(&json!({
            "advice": record.advice,
            "pin": record.pin
        }))?;
        encoded_bytes = encoded_bytes
            .saturating_add(encoded_record.len())
            .saturating_add(if entries.is_empty() { 0 } else { 1 });
        if encoded_bytes > MAX_EXPERT_CATALOG_BYTES {
            return Ok(None);
        }
        if !entries.is_empty() {
            digest.update(b",");
        }
        digest.update(&encoded_record);
        entries.push(record);
    }
    digest.update(b"]");
    let digest = hex::encode(digest.finalize());
    Ok(Some(ExpertCatalog { digest, entries }))
}

/// Select a supported, complete closure entirely from the frozen catalog.
pub(in crate::agent) fn select_from_catalog(
    catalog: &ExpertCatalog,
    interpretation: &crate::language::Interpretation,
    references: &str,
    pins: &Value,
    cfg: &crate::config::Config,
    native_policy: Option<&crate::residency::lifecycle::NativeLifecyclePolicy>,
) -> Result<Option<(String, Value)>> {
    if catalog.entries.len() > MAX_EXPERT_CATALOG_ENTRIES {
        bail!("Frozen expert catalog exceeds its entry bound")
    }
    let expected_digest = pins
        .get("expert_catalog_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Saved opening context has no frozen expert catalog pin"))?;
    // Fields are private, immutable and contain no interior mutability. The
    // opening digest binds this one-use host object without reserializing it.
    if expected_digest != catalog.digest {
        bail!("Frozen expert catalog differs from its opening identity pin")
    }
    validate_sha256(&catalog.digest)?;
    validate_checkpoint(references, pins)?;
    if interpretation.required_capabilities.is_empty()
        || interpretation.required_capabilities.len() > 16
    {
        return Ok(None);
    }
    let requirements = crate::capabilities::version_requirements(
        &interpretation.required_capabilities,
        &interpretation.capability_versions,
    )?;
    let by_id = catalog
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    if by_id.len() != catalog.entries.len() {
        bail!("Frozen expert catalog contains duplicate identities")
    }
    let allowed = catalog
        .entries
        .iter()
        .map(|record| record.executable && registered_candidate_allowed(record, cfg, native_policy))
        .collect::<Vec<_>>();

    let root_limit = cfg.expert_parallelism.unwrap_or(1).min(4);
    if root_limit == 0 {
        bail!("Configured expert parallelism is outside its bound")
    }
    let mut next_reference = crate::strict_json::value(references.as_bytes(), MAX_REFERENCE_BYTES)?;
    let mut next_pins = pins.clone();
    let optional = take_optional_pairs(&mut next_reference, &mut next_pins)?;
    next_reference["expert_advice"] = json!([]);
    next_pins["manifests"] = json!([]);
    let mut selected = Vec::<ExpertRecord>::new();
    let mut selected_ids = BTreeSet::new();
    let mut selected_roots = BTreeSet::new();
    let mut selected_capabilities = BTreeSet::new();
    for capability_id in &interpretation.required_capabilities {
        if !selected_capabilities.insert(capability_id.clone())
            || selected_roots.len() >= root_limit
        {
            continue;
        }
        let requirement = requirements.get(capability_id);
        let mut candidates = catalog
            .entries
            .iter()
            .enumerate()
            .filter(|(index, record)| {
                allowed[*index]
                    && record.advice["kind"] == "workflow"
                    && !selected_roots.contains(&record.id)
                    && record.capabilities.iter().any(|capability| {
                        capability.id.as_str() == capability_id.as_str()
                            && requirement.is_none_or(|requirement| {
                                semver::Version::parse(&capability.version)
                                    .is_ok_and(|version| requirement.matches(&version))
                            })
                    })
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(_, left), (_, right)| left.id.cmp(&right.id));

        for (root_index, root) in candidates {
            let Some(closure) = catalog_closure(root_index, &catalog.entries, &by_id) else {
                continue;
            };
            if closure.iter().any(|index| !allowed[*index]) {
                continue;
            }
            let additions = closure
                .iter()
                .filter(|index| !selected_ids.contains(&catalog.entries[**index].id))
                .count();
            if selected.len() + additions > MAX_EXPERT_ADVICE {
                continue;
            }

            // Root first preserves the requested entrypoint; its dependency
            // declarations still carry the exact execution ordering constraints.
            let mut proposed = selected.clone();
            let mut proposed_ids = selected_ids.clone();
            if proposed_ids.insert(root.id.clone()) {
                proposed.push((*root).clone());
            }
            for index in closure {
                let record = &catalog.entries[index];
                if proposed_ids.insert(record.id.clone()) {
                    proposed.push(record.clone());
                }
            }
            let mut candidate_reference = next_reference.clone();
            let mut candidate_pins = next_pins.clone();
            candidate_reference["expert_advice"] = json!(proposed
                .iter()
                .map(|record| record.advice.clone())
                .collect::<Vec<_>>());
            candidate_pins["manifests"] = json!(proposed
                .iter()
                .map(|record| record.pin.clone())
                .collect::<Vec<_>>());
            if encoded_len(&candidate_reference)? > MAX_REFERENCE_BYTES
                || encoded_len(&candidate_pins)? > MAX_PIN_BYTES
            {
                // A large first candidate must not hide a later fitting root.
                continue;
            }
            selected = proposed;
            selected_ids = proposed_ids;
            next_reference = candidate_reference;
            next_pins = candidate_pins;
            selected_roots.insert(root.id.clone());
            break;
        }
    }
    if selected_roots.is_empty() {
        return Ok(None);
    }

    if encoded_len(&next_reference)? > MAX_REFERENCE_BYTES
        || encoded_len(&next_pins)? > MAX_PIN_BYTES
    {
        return Ok(None);
    }

    restore_optional_pairs(&mut next_reference, &mut next_pins, optional)?;
    let encoded_reference = serde_json::to_string(&next_reference)?;
    if encoded_reference.len() > MAX_REFERENCE_BYTES || encoded_len(&next_pins)? > MAX_PIN_BYTES {
        return Ok(None);
    }
    if next_pins["expert_catalog_sha256"].as_str() != Some(catalog.digest.as_str()) {
        bail!("Expert catalog pin changed while selecting its frozen closure")
    }
    validate_checkpoint(&encoded_reference, &next_pins)?;
    Ok(Some((encoded_reference, next_pins)))
}

fn catalog_closure(
    root: usize,
    entries: &[ExpertRecord],
    by_id: &BTreeMap<&str, usize>,
) -> Option<Vec<usize>> {
    fn visit(
        index: usize,
        entries: &[ExpertRecord],
        by_id: &BTreeMap<&str, usize>,
        visiting: &mut BTreeSet<String>,
        complete: &mut BTreeSet<String>,
        ordered: &mut Vec<usize>,
    ) -> Option<()> {
        let entry = entries.get(index)?;
        if complete.contains(&entry.id) {
            return Some(());
        }
        if complete.len() + visiting.len() >= MAX_EXPERT_ADVICE
            || !entry.executable
            || entry.advice["kind"] != "workflow"
            || !visiting.insert(entry.id.clone())
        {
            return None;
        }
        for dependency in &entry.dependencies {
            let prerequisite_index = *by_id.get(dependency.id.as_str())?;
            let prerequisite = entries.get(prerequisite_index)?;
            let version = semver::Version::parse(&prerequisite.version).ok()?;
            let requirement = semver::VersionReq::parse(&dependency.requirement).ok()?;
            if !requirement.matches(&version) {
                return None;
            }
            visit(
                prerequisite_index,
                entries,
                by_id,
                visiting,
                complete,
                ordered,
            )?;
        }
        visiting.remove(&entry.id);
        complete.insert(entry.id.clone());
        ordered.push(index);
        (ordered.len() <= MAX_EXPERT_ADVICE).then_some(())
    }
    let mut visiting = BTreeSet::new();
    let mut complete = BTreeSet::new();
    let mut ordered = Vec::new();
    visit(
        root,
        entries,
        by_id,
        &mut visiting,
        &mut complete,
        &mut ordered,
    )?;
    Some(ordered)
}

fn registered_candidate_allowed(
    record: &ExpertRecord,
    cfg: &crate::config::Config,
    native_policy: Option<&crate::residency::lifecycle::NativeLifecyclePolicy>,
) -> bool {
    if record.advice["registered_model"].is_null() {
        if !record.advice["model_selection"].is_null() {
            return false;
        }
        let Ok((model, _)) = cfg.model_settings("solver") else {
            return false;
        };
        return configured_target_allowed(model, &cfg.api_base, cfg, native_policy);
    }
    let Ok(registration) = serde_json::from_value::<crate::models::Registered>(
        record.advice["registered_model"].clone(),
    ) else {
        return false;
    };
    if !registration.enabled
        || registration.manifest.max_output_tokens < 128
        || cfg.language_model() == Some(registration.manifest.model.as_str())
    {
        return false;
    }
    configured_target_allowed(
        &registration.manifest.model,
        &registration.manifest.api_base,
        cfg,
        native_policy,
    )
}

/// Discovery cannot make an unconfigured model eligible for later admission.
/// This checks declarations only; dispatch retains live resource/lifecycle gates.
fn configured_target_allowed(
    model: &str,
    api_base: &str,
    cfg: &crate::config::Config,
    native_policy: Option<&crate::residency::lifecycle::NativeLifecyclePolicy>,
) -> bool {
    if cfg.language_model() == Some(model) {
        return false;
    }
    let resource_api_base = if let Some(policy) = native_policy {
        let (Ok(mut approved), Ok(candidate)) =
            (url::Url::parse(&policy.api_base), url::Url::parse(api_base))
        else {
            return false;
        };
        if api_base != cfg.api_base
            || approved.origin() != candidate.origin()
            || !policy.models.iter().any(|allowed| allowed.model == model)
        {
            return false;
        }
        approved.set_path("/api/v1");
        approved.to_string()
    } else {
        api_base.to_owned()
    };
    let Ok(target) = (crate::backend::ModelTarget {
        api_base: resource_api_base,
        model: model.to_owned(),
    })
    .canonical() else {
        return false;
    };
    let Some(resources) = &cfg.resource_admission else {
        return true;
    };
    if !resources.working_profiles.is_empty()
        && !resources.working_profiles.iter().any(|profile| {
            profile.target == target && profile.roles.iter().any(|role| role == "solver")
        })
    {
        return false;
    }
    resources.residents.is_empty()
        || resources.residents.iter().any(|resident| {
            resident.consumers.iter().any(|consumer| {
                consumer
                    .canonical()
                    .is_ok_and(|consumer| consumer == target)
            })
        })
}

struct OptionalPairs {
    memories: Vec<(Value, Value)>,
    documents: Vec<(Value, Value)>,
    skills: Vec<(Value, Value)>,
}

fn take_optional_pairs(reference: &mut Value, pins: &mut Value) -> Result<OptionalPairs> {
    fn take(reference: &mut Value, pins: &mut Value, key: &str) -> Result<Vec<(Value, Value)>> {
        let ref_values = reference
            .get_mut(key)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow::anyhow!("Saved optional {key} reference is not an array"))?;
        let pin_values = pins
            .get_mut(key)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow::anyhow!("Saved optional {key} pins are not an array"))?;
        if ref_values.len() != pin_values.len() {
            bail!("Saved optional {key} references and pins do not match")
        }
        Ok(ref_values.drain(..).zip(pin_values.drain(..)).collect())
    }
    Ok(OptionalPairs {
        memories: take(reference, pins, "memories")?,
        documents: take(reference, pins, "documents")?,
        skills: take(reference, pins, "skills")?,
    })
}

fn restore_optional_pairs(
    reference: &mut Value,
    pins: &mut Value,
    optional: OptionalPairs,
) -> Result<()> {
    for (key, pairs) in [
        ("memories", optional.memories),
        ("documents", optional.documents),
        ("skills", optional.skills),
    ] {
        for (item, pin) in pairs {
            let mut candidate_reference = reference.clone();
            let mut candidate_pins = pins.clone();
            candidate_reference[key]
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("Optional {key} reference is not an array"))?
                .push(item);
            candidate_pins[key]
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("Optional {key} pins are not an array"))?
                .push(pin);
            if encoded_len(&candidate_reference)? <= MAX_REFERENCE_BYTES
                && encoded_len(&candidate_pins)? <= MAX_PIN_BYTES
            {
                *reference = candidate_reference;
                *pins = candidate_pins;
            }
        }
    }
    Ok(())
}
