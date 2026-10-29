//! Small task-bound memory for planner, explorer, and reflector handoffs.
//!
//! Model conclusions stay structured and advisory. Raw legacy prose is replaced
//! by a digest marker and never promoted into a claim or permission.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) const ROLE_BYTES: usize = 400;
const MAX_MEMORY_BYTES: usize = 2_000;
const FORMAT: &str = "cgf-reasoning-memory-v1";
const FORMAT_PREFIX: &str = "cgf-reasoning-memory-";
const TRUST: &str = "untrusted_model_advisory_never_permissions";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Memory {
    format: String,
    version: u8,
    root_sha256: String,
    references_sha256: String,
    trust: String,
    context: Value,
    roles: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    feedback: Option<Feedback>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Feedback {
    code: String,
    message: String,
}

/// Replace selected role records while retaining the original structured
/// context and other role records. `None` means the complete checkpoint exceeds
/// its existing byte cap; no truncation or record eviction is performed.
pub(super) fn combine(
    root: &str,
    references: &str,
    previous: &str,
    updates: &[(&str, Value)],
) -> Result<Option<String>> {
    let mut memory = load(root, references, previous)?;
    let mut seen = BTreeSet::new();
    for (role, record) in updates {
        if !valid_role(role) || !seen.insert(*role) {
            bail!("Invalid or duplicate reasoning memory role")
        }
        crate::agent::advisory_record::validate(record, ROLE_BYTES)?;
        memory.roles.insert((*role).to_owned(), record.clone());
    }
    encode(memory, references)
}

/// Check whether projected role records fit without publishing sizing values.
pub(super) fn projects(
    root: &str,
    references: &str,
    previous: &str,
    roles: &[&str],
) -> Result<bool> {
    let mut memory = load(root, references, previous)?;
    let mut seen = BTreeSet::new();
    for role in roles {
        if !valid_role(role) || !seen.insert(*role) {
            bail!("Invalid or duplicate reasoning memory role")
        }
        memory.roles.insert(
            (*role).to_owned(),
            crate::agent::advisory_record::sizing_value(ROLE_BYTES)?,
        );
    }
    Ok(serde_json::to_vec(&memory)?.len() <= MAX_MEMORY_BYTES)
}

/// Store the deterministic checker outcome as an enum plus bounded message.
pub(super) fn feedback(
    root: &str,
    references: &str,
    previous: &str,
    code: &str,
    message: &str,
) -> Result<Option<String>> {
    if code != "compiled_checker_rejection" || message.trim().is_empty() || message.len() > 256 {
        bail!("Invalid deterministic reasoning feedback")
    }
    let mut memory = load(root, references, previous)?;
    memory.feedback = Some(Feedback {
        code: code.to_owned(),
        message: message.to_owned(),
    });
    encode(memory, references)
}

/// Legacy checkpoints remain readable; a marked new envelope is fully checked.
pub(super) fn validate_checkpoint(raw: &str, references: &str) -> Result<()> {
    if raw.len() > MAX_MEMORY_BYTES {
        bail!("Reasoning memory checkpoint exceeds its byte limit")
    }
    match decode(raw)? {
        Some(memory) => {
            validate_memory(&memory, None, references)?;
            Ok(())
        }
        None => Ok(()),
    }
}

/// Bind a new-format checkpoint to the exact original request and references.
pub(super) fn validate_root(raw: &str, root: &str, references: &str) -> Result<()> {
    if raw.len() > MAX_MEMORY_BYTES {
        bail!("Reasoning memory checkpoint exceeds its byte limit")
    }
    if let Some(memory) = decode(raw)? {
        validate_memory(&memory, Some(root), references)?;
    }
    Ok(())
}

/// Recover only the original interpretation data for expert routing. A marked
/// envelope must match the same task and references before its context is read.
pub(super) fn interpretation(raw: &str, root: &str, references: &str) -> Result<Option<Value>> {
    if raw.len() > MAX_MEMORY_BYTES || root.is_empty() {
        bail!("Reasoning memory input exceeds its bound")
    }
    let context = match decode(raw)? {
        Some(memory) => {
            validate_memory(&memory, Some(root), references)?;
            memory.context
        }
        None => match crate::strict_json::value(raw.as_bytes(), MAX_MEMORY_BYTES) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        },
    };
    Ok(context.get("interpretation").cloned())
}

fn load(root: &str, references: &str, previous: &str) -> Result<Memory> {
    if root.is_empty() || root.len() > 1_000_000 || previous.len() > MAX_MEMORY_BYTES {
        bail!("Reasoning memory input exceeds its bound")
    }
    let root_sha256 = crate::store::sha256(root.as_bytes());
    let references_sha256 = crate::store::sha256(references.as_bytes());
    let (context, roles, feedback) = match decode(previous)? {
        Some(memory) => {
            validate_memory(&memory, Some(root), references)?;
            (memory.context, memory.roles, memory.feedback)
        }
        None => (legacy_context(previous)?, BTreeMap::new(), None),
    };
    Ok(Memory {
        format: FORMAT.to_owned(),
        version: 1,
        root_sha256,
        references_sha256,
        trust: TRUST.to_owned(),
        context,
        roles,
        feedback,
    })
}

fn legacy_context(raw: &str) -> Result<Value> {
    if raw.is_empty() {
        return Ok(Value::Null);
    }
    match crate::strict_json::value(raw.as_bytes(), MAX_MEMORY_BYTES) {
        Ok(Value::String(_)) | Err(_) => Ok(serde_json::json!({
            "legacy_advice_sha256":crate::store::sha256(raw.as_bytes()),
            "legacy_text_omitted":true
        })),
        Ok(value) => Ok(value),
    }
}

fn decode(raw: &str) -> Result<Option<Memory>> {
    if raw.len() > MAX_MEMORY_BYTES {
        bail!("Reasoning memory checkpoint exceeds its byte limit")
    }
    if raw.trim_start().starts_with(FORMAT_PREFIX) {
        bail!("Reasoning memory schema version is unsupported")
    }
    let value = match crate::strict_json::value(raw.as_bytes(), MAX_MEMORY_BYTES) {
        Ok(value) => value,
        Err(_) if raw.contains(FORMAT_PREFIX) => bail!("Malformed marked reasoning memory"),
        Err(_) => return Ok(None),
    };
    let looks_like_memory = ["root_sha256", "references_sha256", "roles"]
        .iter()
        .all(|field| value.get(*field).is_some());
    let format = match value.get("format").and_then(Value::as_str) {
        Some(format) => format,
        None if looks_like_memory => bail!("Reasoning memory has no valid format marker"),
        None => return Ok(None),
    };
    if !format.starts_with(FORMAT_PREFIX) {
        if looks_like_memory {
            bail!("Reasoning memory format marker was replaced")
        }
        return Ok(None);
    }
    if format != FORMAT {
        bail!("Reasoning memory schema version is unsupported")
    }
    let memory: Memory = serde_json::from_value(value)?;
    if serde_json::to_vec(&memory)?.len() > MAX_MEMORY_BYTES {
        bail!("Canonical reasoning memory exceeds its byte limit")
    }
    Ok(Some(memory))
}

fn validate_memory(memory: &Memory, root: Option<&str>, references: &str) -> Result<()> {
    if memory.format != FORMAT
        || memory.version != 1
        || memory.trust != TRUST
        || !valid_sha256(&memory.root_sha256)
        || !valid_sha256(&memory.references_sha256)
        || memory.references_sha256 != crate::store::sha256(references.as_bytes())
        || memory.roles.len() > 3
        || root.is_some_and(|root| memory.root_sha256 != crate::store::sha256(root.as_bytes()))
    {
        bail!("Reasoning memory identity, schema or size is invalid")
    }
    for (role, record) in &memory.roles {
        if !valid_role(role) {
            bail!("Reasoning memory contains an unknown role")
        }
        crate::agent::advisory_record::validate(record, ROLE_BYTES)?;
    }
    if let Some(feedback) = &memory.feedback {
        if feedback.code != "compiled_checker_rejection"
            || feedback.message.trim().is_empty()
            || feedback.message.len() > 256
        {
            bail!("Reasoning memory feedback is invalid")
        }
    }
    Ok(())
}

fn encode(memory: Memory, references: &str) -> Result<Option<String>> {
    validate_memory(&memory, None, references)?;
    let encoded = serde_json::to_string(&memory)?;
    if encoded.len() > MAX_MEMORY_BYTES {
        return Ok(None);
    }
    Ok(Some(encoded))
}

fn valid_role(role: &str) -> bool {
    matches!(role, "planner" | "explorer" | "reflector")
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
