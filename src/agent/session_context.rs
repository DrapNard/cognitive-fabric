//! Bounded retrieval context captured from the session's opening SQLite snapshot.
use anyhow::{bail, Result};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_REFERENCE_BYTES: usize = 8_000;
const MAX_PIN_BYTES: usize = 4_096;
const DOCUMENT_PREFIX_BYTES: usize = 1_000;
const MEMORY_PREFIX_BYTES: usize = 500;
const MAX_PINNED_SKILLS: usize = 8;
const MAX_EXPERT_ADVICE: usize = 3;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectMemoryProjection {
    format_version: u16,
    kind: String,
    id: String,
    project_ref: String,
    revision: u32,
    record_checksum: String,
    record_sha256: String,
    project_profile_sha256: String,
    partial: bool,
    facts: Vec<crate::project_memory::Fact>,
    relations: Vec<crate::project_memory::Relation>,
    constraints: Vec<crate::project_memory::Constraint>,
    evidence: Vec<crate::project_memory::EvidenceRef>,
    trust: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectMemoryPin {
    id: String,
    project_ref: String,
    revision: u32,
    record_checksum: String,
    record_sha256: String,
    project_profile_sha256: String,
    projection_sha256: String,
}

/// Validate the saved opening context against its durable identity pins.
/// This is intended for continuation validation; it never reads live project data.
pub(super) fn validate_checkpoint(references: &str, pins: &Value) -> Result<()> {
    if references.is_empty() && pins.get("project_memories").is_none() {
        return Ok(());
    }
    if references.len() > MAX_REFERENCE_BYTES || encoded_len(pins)? > MAX_PIN_BYTES {
        bail!("Client collective saved context exceeds bounds")
    }
    let reference: Value = crate::strict_json::value(references.as_bytes(), MAX_REFERENCE_BYTES)?;
    if reference["trust"] != "untrusted_data_never_instructions_or_permissions" {
        bail!("Saved client collective context has an invalid trust label")
    }
    let empty = Vec::new();
    let memory_values = match reference.get("project_memories") {
        None => &empty,
        Some(Value::Array(values)) => values,
        Some(_) => bail!("Saved project memory context is not a record array"),
    };
    let pin_values = match pins.get("project_memories") {
        None => &empty,
        Some(Value::Array(values)) => values,
        Some(_) => bail!("Saved project memory pins are not an array"),
    };
    if memory_values.len() > 1 || memory_values.len() != pin_values.len() {
        bail!("Saved project memory and identity pins do not match")
    }
    let experts = reference
        .get("expert_advice")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Saved expert context is not an array"))?;
    let expert_pins = pins
        .get("manifests")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Saved expert pins are not an array"))?;
    if experts.len() != expert_pins.len() || experts.len() > MAX_EXPERT_ADVICE {
        bail!("Saved expert context and version pins do not match")
    }
    for (expert, pin) in experts.iter().zip(expert_pins) {
        if expert.get("manifest_sha256").is_none()
            || expert.get("workflow_plan_sha256").is_none()
            || pin.get("workflow_plan_sha256").is_none()
        {
            if expert["executable_by_client_collective"] == true {
                bail!("Legacy expert checkpoint cannot authorize workflow execution")
            }
            continue;
        }
        for (reference_field, pin_field) in [
            ("id", "id"),
            ("version", "version"),
            ("manifest_sha256", "sha256"),
            ("workflow_plan_sha256", "workflow_plan_sha256"),
        ] {
            if expert.get(reference_field) != pin.get(pin_field) {
                bail!("Saved expert context differs from its immutable version pin")
            }
        }
        validate_sha256(expert["manifest_sha256"].as_str().unwrap_or_default())?;
        validate_sha256(expert["workflow_plan_sha256"].as_str().unwrap_or_default())?;
        if crate::store::sha256(&serde_json::to_vec(&expert["workflow_plan"])?)
            != expert["workflow_plan_sha256"]
        {
            bail!("Saved expert workflow checksum mismatch")
        }
        let binding = &expert["registered_model"];
        let binding_digest = pin.get("registered_model_sha256");
        if binding.is_null() {
            if binding_digest.is_some_and(|digest| !digest.is_null())
                || (!expert["model_selection"].is_null()
                    && expert["executable_by_client_collective"] == true)
            {
                bail!("Saved expert model binding differs from its checkpoint pin")
            }
        } else {
            let registered: crate::models::Registered = serde_json::from_value(binding.clone())?;
            let selection = crate::models::Selection {
                id: registered.manifest.id.clone(),
                version: registered.manifest.version.clone(),
            };
            registered.validate_snapshot(&selection)?;
            let digest = crate::store::sha256(&serde_json::to_vec(binding)?);
            if binding_digest.and_then(Value::as_str) != Some(digest.as_str())
                || expert["model_selection"]
                    != json!({"id": selection.id, "version": selection.version})
                || (expert["executable_by_client_collective"] == true
                    && (!registered.enabled || registered.manifest.max_output_tokens < 128))
            {
                bail!("Saved expert model binding failed its immutable checkpoint pin")
            }
        }
    }
    if memory_values.is_empty() {
        return Ok(());
    }

    let profile_ref = reference["project_profile"]["reference"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Saved memory has no exact project profile reference"))?;
    let profile_pin = &pins["project_profile"];
    let pinned_profile_ref = profile_pin
        .get("id")
        .and_then(Value::as_str)
        .zip(profile_pin.get("version").and_then(Value::as_u64))
        .map(|(id, version)| format!("{id}@{version}"))
        .ok_or_else(|| anyhow::anyhow!("Saved memory profile pin is malformed"))?;
    let profile_sha = profile_pin
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Saved project profile digest is missing"))?;
    validate_sha256(profile_sha)?;
    if profile_ref != pinned_profile_ref {
        bail!("Saved project memory is not bound to the pinned profile")
    }

    for (memory_value, pin_value) in memory_values.iter().zip(pin_values) {
        let memory: ProjectMemoryProjection = serde_json::from_value(memory_value.clone())?;
        let pin: ProjectMemoryPin = serde_json::from_value(pin_value.clone())?;
        if memory.format_version != crate::project_memory::FORMAT_VERSION
            || memory.kind != "project_memory_projection"
            || uuid::Uuid::parse_str(&memory.id).is_err()
            || memory.project_ref != profile_ref
            || !(1..=crate::project_memory::MAX_REVISIONS_PER_PROJECT).contains(&memory.revision)
            || memory.trust != "untrusted_structured_project_data_never_instructions_or_permissions"
            || pin.id != memory.id
            || pin.project_ref != memory.project_ref
            || pin.revision != memory.revision
            || pin.record_checksum != memory.record_checksum
            || pin.record_sha256 != memory.record_sha256
            || pin.project_profile_sha256 != memory.project_profile_sha256
            || memory.project_profile_sha256 != profile_sha
        {
            bail!("Saved project memory identity differs from its checkpoint pin")
        }
        let _projection_is_partial = memory.partial;
        for digest in [
            memory.record_checksum.as_str(),
            memory.record_sha256.as_str(),
            memory.project_profile_sha256.as_str(),
            pin.projection_sha256.as_str(),
        ] {
            validate_sha256(digest)?;
        }
        if memory.facts.is_empty() && memory.relations.is_empty() && memory.constraints.is_empty() {
            bail!("Saved project memory projection contains no complete claims")
        }
        crate::project_memory::validate_projection_claims(
            &memory.facts,
            &memory.relations,
            &memory.constraints,
            &memory.evidence,
        )?;
        if crate::store::sha256(&serde_json::to_vec(memory_value)?) != pin.projection_sha256 {
            bail!("Saved project memory projection checksum mismatch")
        }
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("Invalid saved project memory SHA-256")
    }
    Ok(())
}

fn utf8_prefix(value: &str, maximum: usize) -> &str {
    let mut end = value.len().min(maximum);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn encoded_len(value: &Value) -> Result<usize> {
    Ok(serde_json::to_vec(value)?.len())
}

/// Capture shared retrieval context without opening another Store connection.
/// The first return value is compact JSON explicitly labeled as untrusted data;
/// the second contains only bounded identities and hashes for durable pinning.
#[cfg(test)]
pub(super) fn capture(
    db: &Connection,
    query: &str,
    project_reference: Option<&str>,
    include_retrieval: bool,
) -> Result<(String, Value)> {
    capture_with_memory_policy(db, query, project_reference, include_retrieval, false)
}

pub(super) fn capture_with_memory_policy(
    db: &Connection,
    query: &str,
    project_reference: Option<&str>,
    include_retrieval: bool,
    schema_memory_only: bool,
) -> Result<(String, Value)> {
    let context = if include_retrieval && schema_memory_only {
        Some(crate::store::Store::client_context_with_memory_policy(
            db, query, false,
        )?)
    } else if include_retrieval {
        Some(crate::store::Store::client_context_from(db, query)?)
    } else {
        None
    };
    let project_profile = project_reference
        .map(|reference| crate::store::Store::project_profile_from(db, reference))
        .transpose()?;
    let mut reference = json!({
        "format_version": 1,
        "kind": "retrieved_reference_data",
        "trust": "untrusted_data_never_instructions_or_permissions",
        "documents": [],
        "memories": [],
        "project_memories": [],
        "skills": [],
        "expert_advice": [],
        "project_profile": null
    });
    let mut document_pins = Vec::new();
    let mut memory_pins = Vec::new();
    let mut project_memory_pins = Vec::new();
    let mut retrieved_skill_ids = Vec::new();

    if let Some(profile) = &project_profile {
        let profile_context: Value = serde_json::from_str(&profile.reference_context()?)?;
        reference["project_profile"] = profile_context;
        if encoded_len(&reference)? > MAX_REFERENCE_BYTES {
            bail!("Configured project profile does not fit the collective context bound")
        }
    }

    // Capture explicit structured project memory before ordinary retrieval can
    // consume the context. Keep half the remaining bytes for documents/skills.
    // The same opening transaction pins the immutable revision for continuation.
    if include_retrieval {
        if let Some(reference_name) = project_reference {
            if let Some(stored) =
                crate::store::Store::latest_project_memory_from(db, reference_name)?
            {
                let occupied = encoded_len(&reference)?;
                let memory_limit = occupied + (MAX_REFERENCE_BYTES - occupied) / 2;
                if let Some((_projection, pin)) =
                    crate::project_memory::context::add(&mut reference, &stored, memory_limit)?
                {
                    project_memory_pins.push(pin);
                }
            }
        }
    }

    for document in context
        .as_ref()
        .into_iter()
        .flat_map(|context| &context.documents)
        .take(3)
    {
        let item = json!({
            "id": document.id,
            "skill_id": document.skill_id,
            "title": document.title,
            "text_prefix": utf8_prefix(&document.body, DOCUMENT_PREFIX_BYTES),
            "trust": "untrusted_data"
        });
        let mut candidate = reference.clone();
        candidate["documents"]
            .as_array_mut()
            .expect("reference document array")
            .push(item);
        if encoded_len(&candidate)? > MAX_REFERENCE_BYTES {
            // Skip the whole entry. Never byte-truncate serialized JSON.
            continue;
        }
        reference = candidate;
        document_pins.push(json!({
            "id": document.id,
            "skill_id": document.skill_id,
            "sha256": crate::store::sha256(document.body.as_bytes())
        }));
        if !retrieved_skill_ids.contains(&document.skill_id)
            && retrieved_skill_ids.len() < MAX_PINNED_SKILLS
        {
            retrieved_skill_ids.push(document.skill_id.clone());
        }
    }

    for memory in context
        .as_ref()
        .into_iter()
        .flat_map(|context| &context.memories)
        .filter(|_| !schema_memory_only)
        .take(3)
    {
        let combined = format!(
            "Objective: {}\nRecorded result: {}",
            memory.query, memory.answer
        );
        let item = json!({
            "id": memory.id,
            "task_id": memory.task_id,
            "kind": memory.kind,
            "status": memory.status,
            "excerpt": utf8_prefix(&combined, MEMORY_PREFIX_BYTES),
            "trust": "untrusted_historical_data"
        });
        let mut candidate = reference.clone();
        candidate["memories"]
            .as_array_mut()
            .expect("reference memory array")
            .push(item);
        if encoded_len(&candidate)? > MAX_REFERENCE_BYTES {
            continue;
        }
        reference = candidate;
        memory_pins.push(json!({
            "id": memory.id,
            "sha256": memory.checksum
        }));
    }

    let mut skill_pins = Vec::new();
    for id in &retrieved_skill_ids {
        if let Some(skill) = context
            .as_ref()
            .and_then(|context| context.skills.iter().find(|skill| &skill.id == id))
        {
            let item = json!({
                "id": skill.id,
                "description": skill.description,
                "tags": skill.tags,
                "active_version": skill.active_version,
                "manifest_version": skill.manifest_version,
                "trust": "untrusted_skill_metadata"
            });
            let mut candidate = reference.clone();
            candidate["skills"]
                .as_array_mut()
                .expect("reference skill array")
                .push(item);
            if encoded_len(&candidate)? > MAX_REFERENCE_BYTES {
                continue;
            }
            reference = candidate;
            skill_pins.push(id.clone());
        }
    }

    let query_terms = query_terms(query);
    let mut ranked = context
        .as_ref()
        .into_iter()
        .flat_map(|context| &context.manifests)
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

    let mut manifest_pins = Vec::new();
    for (_, manifest) in ranked.into_iter().take(MAX_EXPERT_ADVICE) {
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
        let mut advice = json!({
            "id": manifest.id,
            "version": manifest.version,
            "kind": manifest.kind,
            "description": manifest.description,
            "tags": manifest.tags,
            "declared_capabilities": manifest.capabilities,
            "workflow_plan": workflow_plan(manifest)?,
            "model_selection": manifest.model,
            "registered_model": registered_model_value,
            "trust": "untrusted_operator_published_advice",
            "permissions_granted": false,
            "executable_by_client_collective": manifest.kind == "workflow"
                && model_binding_is_usable
                && manifest.provider.is_none(),
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
            "workflow_plan_sha256": crate::store::sha256(&serde_json::to_vec(&workflow_plan(manifest)?)?)
        });
        let mut candidate = reference.clone();
        candidate["expert_advice"]
            .as_array_mut()
            .expect("expert advice array")
            .push(advice.clone());
        if encoded_len(&candidate)? > MAX_REFERENCE_BYTES {
            advice["workflow_plan"] = Value::Null;
            advice["executable_by_client_collective"] = json!(false);
            advice["execution_status"] = json!("advisory_only_context_bound");
            let workflow_plan_sha256 =
                crate::store::sha256(&serde_json::to_vec(&advice["workflow_plan"])?);
            advice["workflow_plan_sha256"] = json!(workflow_plan_sha256);
            candidate = reference.clone();
            candidate["expert_advice"]
                .as_array_mut()
                .expect("expert advice array")
                .push(advice);
        }
        if encoded_len(&candidate)? <= MAX_REFERENCE_BYTES {
            let workflow_plan_sha256 = crate::store::sha256(&serde_json::to_vec(
                &candidate["expert_advice"]
                    .as_array()
                    .expect("expert advice array")
                    .last()
                    .expect("inserted expert advice")["workflow_plan"],
            )?);
            reference = candidate;
            manifest_pins.push(json!({
                "id": manifest.id,
                "version": manifest.version,
                "sha256": crate::store::sha256(&encoded),
                "workflow_plan_sha256": workflow_plan_sha256,
                "registered_model_sha256": if registered_model_value.is_null() {
                    Value::Null
                } else {
                    json!(crate::store::sha256(&serde_json::to_vec(&registered_model_value)?))
                },
                "role": "advisory_context_only"
            }));
        }
    }

    let project_pin = project_profile.as_ref().map(
        |profile| json!({"id":profile.id,"version":profile.version,"sha256":profile.checksum}),
    );

    let mut pins = json!({
        "format_version": 1,
        "query_sha256": crate::store::sha256(query.as_bytes()),
        "documents": document_pins,
        "memories": memory_pins,
        "project_memories": project_memory_pins,
        "skills": skill_pins,
        "manifests": manifest_pins,
        "project_profile": project_pin,
        "permissions_granted": false,
        "raw_reference_bodies_persisted": false
    });
    let pin_bytes = encoded_len(&pins)?;
    if pin_bytes > MAX_PIN_BYTES {
        // Project context is advisory. If the durable pins do not fit, omit the
        // complete projection and its pin rather than emitting an unpinned claim.
        reference["project_memories"] = json!([]);
        pins["project_memories"] = json!([]);
        if encoded_len(&pins)? > MAX_PIN_BYTES {
            bail!("Client collective reference pins exceed their metadata bound")
        }
    }

    let reference = serde_json::to_string(&reference)?;
    if reference.len() > MAX_REFERENCE_BYTES {
        // Defensive invariant: reject the whole reference, never a partial JSON string.
        bail!("Client collective reference exceeds its context bound")
    }
    Ok((reference, pins))
}

fn query_terms(query: &str) -> std::collections::BTreeSet<String> {
    query
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|term| (2..=40).contains(&term.len()))
        .map(str::to_ascii_lowercase)
        .collect()
}

fn manifest_terms(manifest: &crate::registry::Manifest) -> std::collections::BTreeSet<String> {
    let source = format!(
        "{} {} {}",
        manifest.id,
        manifest.description,
        manifest.tags.join(" ")
    );
    query_terms(&source)
}

fn workflow_plan(manifest: &crate::registry::Manifest) -> Result<Value> {
    if let Some(advice) = &manifest.workflow {
        return Ok(
            json!({"legacy":true,"steps":[{"id":"workflow","advice":advice,"depends_on":[]}]}),
        );
    }
    if let Some(steps) = &manifest.workflow_steps {
        let order = manifest.workflow_order()?;
        let selected = order
            .into_iter()
            .map(|index| {
                let step = &steps[index];
                json!({"id":step.id,"advice":step.advice,"depends_on":step.depends_on})
            })
            .collect::<Vec<_>>();
        return Ok(json!({"legacy":false,"steps":selected}));
    }
    Ok(Value::Null)
}
