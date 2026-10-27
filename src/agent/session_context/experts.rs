//! Capture complete, bounded client expert dependency closures.
use anyhow::{bail, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::*;

mod catalog;
pub(in crate::agent) use catalog::ExpertCatalog;
pub(super) use catalog::{capture_catalog, select_from_catalog};

/// Append ranked expert advice and immutable pins as complete dependency
/// groups. A group is admitted only when its entire new closure fits both
/// durable envelopes.
pub(super) fn capture(
    db: &Connection,
    reference: &mut Value,
    pins: &mut Value,
    context: Option<&crate::store::TaskContext>,
    query: &str,
) -> Result<()> {
    let Some(context) = context else {
        ensure_manifest_array(reference, pins)?;
        return Ok(());
    };
    ensure_manifest_array(reference, pins)?;
    if context.manifests.len() > 4096 {
        bail!("Opening expert registry exceeds its bounded snapshot")
    }

    let mut manifests = BTreeMap::new();
    for manifest in &context.manifests {
        if manifests.insert(manifest.id.as_str(), manifest).is_some() {
            bail!("Opening expert snapshot contains duplicate manifest IDs")
        }
    }

    let query_terms = query_terms(query);
    let mut ranked = context
        .manifests
        .iter()
        .filter_map(|manifest| {
            let score = manifest_terms(manifest).intersection(&query_terms).count();
            (score > 0).then_some((score, manifest))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut selected = reference["expert_advice"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Expert advice reference is not an array"))?
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let pinned = pins["manifests"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Expert manifest pins are not an array"))?
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if selected.len() > MAX_EXPERT_ADVICE || selected != pinned {
        bail!("Existing expert advice and manifest pins do not match their bound")
    }
    // Runnable groups get the bounded slots before metadata-only advice.
    // Stable query ranking is preserved within each pass.
    for executable_pass in [true, false] {
        for &(_, root) in &ranked {
            if selected.len() >= MAX_EXPERT_ADVICE {
                break;
            }
            if selected.contains(&root.id) {
                continue;
            }
            let root_record = ExpertRecord::capture(db, root)?;
            let needs_executable = root_record.executable;
            if needs_executable != executable_pass {
                continue;
            }
            let closure = if needs_executable {
                let Some(closure) = workflow_closure(root, &manifests) else {
                    // Keep the snapshot honest: an incomplete workflow is not
                    // represented as executable advice.
                    continue;
                };
                closure
            } else {
                // Preserve the historical advisory metadata for non-workflows,
                // provider-backed workflows, and unavailable model bindings.
                vec![root]
            };

            let additions = closure
                .iter()
                .filter(|manifest| !selected.contains(&manifest.id))
                .map(|manifest| manifest.id.as_str())
                .collect::<BTreeSet<_>>();
            if selected.len() + additions.len() > MAX_EXPERT_ADVICE {
                continue;
            }

            let mut group = Vec::with_capacity(additions.len());
            let mut complete = true;
            for manifest in closure {
                if selected.contains(&manifest.id) {
                    if needs_executable
                        && !reference["expert_advice"]
                            .as_array()
                            .is_some_and(|entries| {
                                entries.iter().any(|entry| {
                                    entry["id"] == manifest.id
                                        && entry["executable_by_client_collective"] == true
                                })
                            })
                    {
                        complete = false;
                        break;
                    }
                    continue;
                }
                let record = if manifest.id == root.id {
                    root_record.clone()
                } else {
                    ExpertRecord::capture(db, manifest)?
                };
                if needs_executable && !record.executable {
                    complete = false;
                    break;
                }
                group.push((manifest.id.clone(), record));
            }
            if !complete || group.is_empty() {
                continue;
            }
            // Snapshot order is routing priority, not execution order. Keep the
            // query-matched root first so capability-free fallback cannot select
            // an incidental prerequisite instead. The runtime expands the DAG.
            group.sort_by_key(|(id, _)| usize::from(*id != root.id));

            let mut candidate_reference = reference.clone();
            let mut candidate_pins = pins.clone();
            for (_, record) in &group {
                candidate_reference["expert_advice"]
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("Expert advice reference is not an array"))?
                    .push(record.advice.clone());
                candidate_pins["manifests"]
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("Expert manifest pins are not an array"))?
                    .push(record.pin.clone());
            }
            if encoded_len(&candidate_reference)? > MAX_REFERENCE_BYTES
                || encoded_len(&candidate_pins)? > MAX_PIN_BYTES
            {
                continue;
            }

            *reference = candidate_reference;
            *pins = candidate_pins;
            for (id, _) in group {
                selected.insert(id);
            }
        }
    }
    Ok(())
}

fn ensure_manifest_array(reference: &mut Value, pins: &mut Value) -> Result<()> {
    if !reference["expert_advice"].is_array() {
        bail!("Expert advice reference is not an array")
    }
    if pins.get("manifests").is_none() {
        pins["manifests"] = json!([]);
    }
    if !pins["manifests"].is_array() {
        bail!("Expert manifest pins are not an array")
    }
    Ok(())
}

fn workflow_closure<'a>(
    root: &'a crate::registry::Manifest,
    manifests: &BTreeMap<&'a str, &'a crate::registry::Manifest>,
) -> Option<Vec<&'a crate::registry::Manifest>> {
    fn visit<'a>(
        manifest: &'a crate::registry::Manifest,
        manifests: &BTreeMap<&'a str, &'a crate::registry::Manifest>,
        visiting: &mut BTreeSet<String>,
        complete: &mut BTreeSet<String>,
        ordered: &mut Vec<&'a crate::registry::Manifest>,
    ) -> Option<()> {
        if complete.contains(&manifest.id) {
            return Some(());
        }
        if complete.len() + visiting.len() >= MAX_EXPERT_ADVICE
            || manifest.kind != "workflow"
            || manifest.provider.is_some()
            || !visiting.insert(manifest.id.clone())
        {
            return None;
        }
        for dependency in &manifest.dependencies {
            let prerequisite = *manifests.get(dependency.id.as_str())?;
            if prerequisite.kind != "workflow" || prerequisite.provider.is_some() {
                return None;
            }
            let version = semver::Version::parse(&prerequisite.version).ok()?;
            let requirement = semver::VersionReq::parse(&dependency.requirement).ok()?;
            if !requirement.matches(&version) {
                return None;
            }
            visit(prerequisite, manifests, visiting, complete, ordered)?;
            if ordered.len() > MAX_EXPERT_ADVICE {
                return None;
            }
        }
        visiting.remove(&manifest.id);
        complete.insert(manifest.id.clone());
        ordered.push(manifest);
        (ordered.len() <= MAX_EXPERT_ADVICE).then_some(())
    }

    let mut visiting = BTreeSet::new();
    let mut complete = BTreeSet::new();
    let mut ordered = Vec::new();
    visit(root, manifests, &mut visiting, &mut complete, &mut ordered)?;
    Some(ordered)
}

#[derive(Clone)]
struct ExpertRecord {
    id: String,
    version: String,
    capabilities: Vec<crate::registry::Capability>,
    dependencies: Vec<crate::registry::Dependency>,
    advice: Value,
    pin: Value,
    executable: bool,
}

impl ExpertRecord {
    fn capture(db: &Connection, manifest: &crate::registry::Manifest) -> Result<Self> {
        let encoded = serde_json::to_vec(manifest)?;
        let registered_model = manifest
            .model
            .as_ref()
            .map(|selection| crate::models::registered_from(db, selection))
            .transpose()?
            .flatten();
        let registered_model_enabled = registered_model
            .as_ref()
            .is_some_and(|registered| registered.enabled);
        let model_binding_is_usable = manifest.model.is_none()
            || registered_model_enabled
                && registered_model
                    .as_ref()
                    .is_some_and(|registered| registered.manifest.max_output_tokens >= 128);
        let registered_model_value = registered_model
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?
            .unwrap_or(Value::Null);
        let routing_declarations_sha256 = crate::store::sha256(&serde_json::to_vec(&json!({
            "capabilities": manifest.capabilities,
            "dependencies": manifest.dependencies
        }))?);
        let plan = workflow_plan(manifest)?;
        let plan_sha256 = crate::store::sha256(&serde_json::to_vec(&plan)?);
        let executable =
            manifest.kind == "workflow" && model_binding_is_usable && manifest.provider.is_none();
        let advice = json!({
            "id": manifest.id,
            "version": manifest.version,
            "kind": manifest.kind,
            "description": manifest.description,
            "tags": manifest.tags,
            "declared_capabilities": manifest.capabilities,
            "declared_dependencies": manifest.dependencies,
            "routing_declarations_sha256": routing_declarations_sha256,
            "workflow_plan": plan,
            "model_selection": manifest.model,
            "registered_model": registered_model_value,
            "trust": "untrusted_operator_published_advice",
            "permissions_granted": false,
            "executable_by_client_collective": executable,
            "execution_status": if manifest.kind != "workflow" {
                "metadata_only"
            } else if manifest.provider.is_some() {
                "advisory_only_provider_backend_unsupported"
            } else if manifest.model.is_some() && !model_binding_is_usable {
                "advisory_only_registered_model_unavailable"
            } else if manifest.model.is_some() {
                "bounded_registered_expert_model"
            } else {
                "bounded_shared_solver_model"
            },
            "manifest_sha256": crate::store::sha256(&encoded),
            "workflow_plan_sha256": plan_sha256
        });
        let pin = json!({
            "id": manifest.id,
            "version": manifest.version,
            "sha256": crate::store::sha256(&encoded),
            "workflow_plan_sha256": plan_sha256,
            "routing_declarations_sha256": routing_declarations_sha256,
            "registered_model_sha256": if registered_model_value.is_null() {
                Value::Null
            } else {
                json!(crate::store::sha256(&serde_json::to_vec(&registered_model_value)?))
            },
            "role": "advisory_context_only"
        });
        Ok(Self {
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            capabilities: manifest.capabilities.clone(),
            dependencies: manifest.dependencies.clone(),
            advice,
            pin,
            executable,
        })
    }
}
