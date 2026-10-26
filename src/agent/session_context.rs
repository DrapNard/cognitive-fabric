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
pub(super) fn capture(
    db: &Connection,
    query: &str,
    project_reference: Option<&str>,
    include_retrieval: bool,
) -> Result<(String, Value)> {
    let context = if include_retrieval {
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
        let mut advice = json!({
            "id": manifest.id,
            "version": manifest.version,
            "kind": manifest.kind,
            "description": manifest.description,
            "tags": manifest.tags,
            "declared_capabilities": manifest.capabilities,
            "workflow_advice": workflow_advice(manifest)?,
            "trust": "untrusted_operator_published_advice",
            "permissions_granted": false,
            "executable_by_client_collective": false
        });
        let mut candidate = reference.clone();
        candidate["expert_advice"]
            .as_array_mut()
            .expect("expert advice array")
            .push(advice.clone());
        if encoded_len(&candidate)? > MAX_REFERENCE_BYTES {
            advice["workflow_advice"] = Value::Null;
            candidate = reference.clone();
            candidate["expert_advice"]
                .as_array_mut()
                .expect("expert advice array")
                .push(advice);
        }
        if encoded_len(&candidate)? <= MAX_REFERENCE_BYTES {
            reference = candidate;
            manifest_pins.push(json!({
                "id": manifest.id,
                "version": manifest.version,
                "sha256": crate::store::sha256(&encoded),
                "role": "advisory_context_only"
            }));
        }
    }

    // Project memory is explicit-profile scoped and read through this same opening
    // transaction. Only the latest immutable revision is captured; continuation
    // reuses this snapshot and does not re-read the live registry.
    if include_retrieval {
        if let Some(reference_name) = project_reference {
            if let Some(stored) =
                crate::store::Store::latest_project_memory_from(db, reference_name)?
            {
                if let Some((_projection, pin)) = add_project_memory(&mut reference, &stored)? {
                    project_memory_pins.push(pin);
                    if encoded_len(&reference)? > MAX_REFERENCE_BYTES {
                        bail!("Project memory context exceeded its encoded bound")
                    }
                }
            }
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

fn add_project_memory(
    reference: &mut Value,
    stored: &crate::project_memory::StoredProjectMemory,
) -> Result<Option<(Value, Value)>> {
    use crate::project_memory::{Constraint, Fact, Relation};

    let record = &stored.record;
    let mut facts: Vec<Fact> = Vec::new();
    let mut relations: Vec<Relation> = Vec::new();
    let mut constraints: Vec<Constraint> = Vec::new();
    let mut accepted: Option<(Value, Value)> = None;

    for fact in &record.facts {
        let mut candidate_facts = facts.clone();
        candidate_facts.push(fact.clone());
        if let Some(candidate) = project_memory_candidate(
            reference,
            stored,
            &candidate_facts,
            &relations,
            &constraints,
        )? {
            facts = candidate_facts;
            accepted = Some(candidate);
        }
    }
    for relation in &record.relations {
        let mut candidate_relations = relations.clone();
        candidate_relations.push(relation.clone());
        if let Some(candidate) = project_memory_candidate(
            reference,
            stored,
            &facts,
            &candidate_relations,
            &constraints,
        )? {
            relations = candidate_relations;
            accepted = Some(candidate);
        }
    }
    for constraint in &record.constraints {
        let mut candidate_constraints = constraints.clone();
        candidate_constraints.push(constraint.clone());
        if let Some(candidate) = project_memory_candidate(
            reference,
            stored,
            &facts,
            &relations,
            &candidate_constraints,
        )? {
            constraints = candidate_constraints;
            accepted = Some(candidate);
        }
    }

    let Some((projection, pin)) = accepted else {
        return Ok(None);
    };
    reference["project_memories"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid project-memory context container"))?
        .push(projection.clone());
    Ok(Some((projection, pin)))
}

fn project_memory_candidate(
    current_reference: &Value,
    stored: &crate::project_memory::StoredProjectMemory,
    facts: &[crate::project_memory::Fact],
    relations: &[crate::project_memory::Relation],
    constraints: &[crate::project_memory::Constraint],
) -> Result<Option<(Value, Value)>> {
    if facts.is_empty() && relations.is_empty() && constraints.is_empty() {
        return Ok(None);
    }
    let record = &stored.record;
    let linked = facts
        .iter()
        .flat_map(|claim| claim.evidence.iter())
        .chain(relations.iter().flat_map(|claim| claim.evidence.iter()))
        .chain(constraints.iter().flat_map(|claim| claim.evidence.iter()))
        .collect::<std::collections::BTreeSet<_>>();
    let evidence = record
        .evidence
        .iter()
        .filter(|item| linked.contains(&item.id))
        .collect::<Vec<_>>();
    let partial = facts.len() != record.facts.len()
        || relations.len() != record.relations.len()
        || constraints.len() != record.constraints.len();
    let encoded_record = record.encode()?;
    let projection = json!({
        "format_version": 1,
        "kind": "project_memory_projection",
        "id": record.id,
        "project_ref": record.project_ref,
        "revision": record.revision,
        "record_checksum": record.checksum,
        "record_sha256": crate::store::sha256(encoded_record.as_bytes()),
        "project_profile_sha256": stored.provenance.project_profile_sha256,
        "partial": partial,
        "facts": facts,
        "relations": relations,
        "constraints": constraints,
        "evidence": evidence,
        "trust": "untrusted_structured_project_data_never_instructions_or_permissions"
    });
    let projection_bytes = serde_json::to_vec(&projection)?;
    let pin = json!({
        "id": record.id,
        "project_ref": record.project_ref,
        "revision": record.revision,
        "record_checksum": record.checksum,
        "record_sha256": crate::store::sha256(encoded_record.as_bytes()),
        "project_profile_sha256": stored.provenance.project_profile_sha256,
        "projection_sha256": crate::store::sha256(&projection_bytes)
    });
    let mut candidate_reference = current_reference.clone();
    candidate_reference["project_memories"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid project-memory context container"))?
        .push(projection.clone());
    if encoded_len(&candidate_reference)? > MAX_REFERENCE_BYTES {
        return Ok(None);
    }
    Ok(Some((projection, pin)))
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

fn workflow_advice(manifest: &crate::registry::Manifest) -> Result<Value> {
    if let Some(advice) = &manifest.workflow {
        return Ok(json!({"text":utf8_prefix(advice, 1200)}));
    }
    if let Some(steps) = &manifest.workflow_steps {
        let order = manifest.workflow_order()?;
        let selected = order
            .into_iter()
            .take(4)
            .map(|index| {
                let step = &steps[index];
                json!({"id":step.id,"advice":utf8_prefix(&step.advice, 400)})
            })
            .collect::<Vec<_>>();
        return Ok(json!({"steps":selected}));
    }
    Ok(Value::Null)
}
