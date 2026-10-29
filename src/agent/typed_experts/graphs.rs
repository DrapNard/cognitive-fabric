//! Strict, bounded dataflow templates for typed expert operations.
//!
//! Templates only transfer JSON values between predeclared argument fields. They
//! do not select capabilities, providers, permissions, or executable code.

use super::{providers::Selection, Entry, Work};
use crate::{fabric::modules::Capability, registry::ProviderCapability};
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::{value::RawValue, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

const PREFIX: &str = "typed-graph:";
const MAX_GRAPH_BYTES: usize = 32_000;
const MAX_RESULT_BYTES: usize = 16_384;
const MAX_OPERATIONS: usize = super::MAX_TYPED_OPERATIONS;
const MAX_DEPENDENCIES: usize = MAX_OPERATIONS - 1;
const MAX_BINDINGS: usize = 8;
const MAX_POINTER_BYTES: usize = 256;
const MAX_POINTER_DEPTH: usize = 8;
const MAX_RESOLVED_BYTES: usize = 32_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphDocument {
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    id: String,
    capability: ProviderCapability,
    arguments: Value,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    bindings: Vec<BindingInput>,
    #[serde(default)]
    expert: Option<Selection>,
    #[serde(default)]
    capability_requirement: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingInput {
    target: String,
    source: String,
    #[serde(default)]
    pointer: String,
    #[serde(default)]
    mode: BindingMode,
}

#[derive(Clone, Copy, Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum BindingMode {
    #[default]
    Value,
    NumericExpression,
}

struct Binding {
    target: Vec<String>,
    source: String,
    pointer: Vec<String>,
    mode: BindingMode,
}

/// Immutable validated dataflow configuration retained with its operation.
pub(in crate::agent) struct Template {
    capability: Capability,
    arguments: Value,
    dependencies: Vec<String>,
    bindings: Vec<Binding>,
    template_sha256: String,
    translation_source_sha256: Option<String>,
    reasoning_arguments: bool,
}

impl Template {
    pub(in crate::agent) fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    pub(in crate::agent) fn binding_sources(&self) -> impl Iterator<Item = &str> {
        self.bindings.iter().map(|binding| binding.source.as_str())
    }

    /// Retain the provenance of a natural-language graph translation without
    /// retaining the original prose in the graph template.
    pub(in crate::agent) fn mark_language_translation(&mut self, original: &str) -> Result<()> {
        if Work::has_explicit_prefix(original) {
            bail!("Language translation cannot replace an explicit typed goal")
        }
        self.translation_source_sha256 = Some(crate::store::sha256(original.as_bytes()));
        self.reasoning_arguments = false;
        Ok(())
    }

    /// Bind candidate typed arguments to a root request selected by a reasoning
    /// model. This records provenance without claiming semantic fidelity.
    pub(in crate::agent) fn mark_reasoning_arguments_digest(&mut self, digest: &str) -> Result<()> {
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || self.translation_source_sha256.is_some()
        {
            bail!("Reasoning graph provenance is invalid or already assigned")
        }
        self.translation_source_sha256 = Some(digest.to_owned());
        self.reasoning_arguments = true;
        Ok(())
    }

    /// Resolve prior JSON results into existing argument fields and revalidate
    /// the resulting typed request through the ordinary work adapter.
    pub(in crate::agent) fn resolve(
        &self,
        outputs: &BTreeMap<String, String>,
    ) -> Result<(Work, Value)> {
        let mut resolved = self.arguments.clone();
        let mut source_hashes = BTreeMap::<String, String>::new();
        let mut parsed_outputs = HashMap::<String, String>::new();
        let mut expanded_bytes = 0usize;
        let mut copied_value_bytes = 0usize;

        for binding in &self.bindings {
            let raw_output = outputs
                .get(&binding.source)
                .ok_or_else(|| anyhow!("A required typed graph dependency output is missing"))?;
            if raw_output.len() > MAX_RESULT_BYTES {
                bail!("Typed graph dependency output exceeds its byte limit")
            }
            if !parsed_outputs.contains_key(&binding.source) {
                expanded_bytes = expanded_bytes.saturating_add(raw_output.len());
                if expanded_bytes > MAX_RESOLVED_BYTES {
                    bail!("Typed graph output expansion exceeds its aggregate bound")
                }
                crate::strict_json::value(raw_output.as_bytes(), MAX_RESULT_BYTES)?;
                source_hashes.insert(
                    binding.source.clone(),
                    crate::store::sha256(raw_output.as_bytes()),
                );
                parsed_outputs.insert(binding.source.clone(), raw_output.clone());
            }
            let raw_output = parsed_outputs
                .get(&binding.source)
                .ok_or_else(|| anyhow!("Typed graph dependency output was not retained"))?;
            let selected = raw_pointer(raw_output, &binding.pointer)?;
            let replacement = match binding.mode {
                BindingMode::Value => {
                    crate::strict_json::value(selected.as_bytes(), MAX_RESULT_BYTES)?
                }
                BindingMode::NumericExpression => {
                    if !is_supported_numeric_token(&selected) {
                        bail!("numeric_expression binding requires a plain JSON number")
                    }
                    Value::String(format!("({selected})"))
                }
            };
            copied_value_bytes =
                copied_value_bytes.saturating_add(serde_json::to_vec(&replacement)?.len());
            if copied_value_bytes > MAX_RESOLVED_BYTES {
                bail!("Typed graph copied binding values exceed their aggregate bound")
            }
            replace_existing(&mut resolved, &binding.target, replacement)?;
        }

        let encoded = serde_json::to_vec(&resolved)?;
        if encoded.len() > MAX_RESOLVED_BYTES {
            bail!("Resolved typed graph arguments exceed their byte limit")
        }
        let mut work = Work::from_arguments(self.capability, &resolved)?;
        if let Some(source_digest) = &self.translation_source_sha256 {
            work.mark_translated_digest(source_digest)?;
        }
        let audit = serde_json::json!({
            "template_sha256": self.template_sha256,
            "source_results": source_hashes,
            "resolved_criterion_sha256": work.criterion_digest(),
            "language_translated": work.translated() && !self.reasoning_arguments,
            "reasoning_model_arguments": self.reasoning_arguments,
            "original_source_sha256": self.translation_source_sha256,
            "permissions_granted": false,
        });
        if serde_json::to_vec(&audit)?.len() > 4_096 {
            bail!("Typed graph audit metadata exceeds its byte limit")
        }
        Ok((work, audit))
    }
}

/// Parse only the exact machine syntax. Ordinary user language remains
/// available to the caller's normal language path.
pub(in crate::agent) fn parse(query: &str) -> Result<Option<Vec<Entry>>> {
    let Some(encoded) = query.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    let value = crate::strict_json::value(encoded.as_bytes(), MAX_GRAPH_BYTES)?;
    let document: GraphDocument = serde_json::from_value(value)?;
    if document.operations.is_empty() || document.operations.len() > MAX_OPERATIONS {
        bail!("Typed graph must contain between one and eight operations")
    }

    let mut ids = HashSet::with_capacity(document.operations.len());
    for operation in &document.operations {
        validate_id(&operation.id)?;
        if !ids.insert(operation.id.clone()) {
            bail!("Typed graph operation IDs must be distinct")
        }
        if let Some(expert) = &operation.expert {
            expert.validate()?;
        }
        if let Some(requirement) = &operation.capability_requirement {
            validate_capability_requirement(requirement)?;
        }
        if operation.depends_on.len() > MAX_DEPENDENCIES {
            bail!("Typed graph operation has too many dependencies")
        }
        if operation.bindings.len() > MAX_BINDINGS {
            bail!("Typed graph operation has too many bindings")
        }
        if !operation.arguments.is_object() {
            bail!("Typed graph arguments must be a JSON object")
        }
    }

    let indices: HashMap<&str, usize> = document
        .operations
        .iter()
        .enumerate()
        .map(|(index, operation)| (operation.id.as_str(), index))
        .collect();
    for operation in &document.operations {
        let mut dependencies = HashSet::with_capacity(operation.depends_on.len());
        for dependency in &operation.depends_on {
            validate_id(dependency)?;
            if dependency == &operation.id {
                bail!("Typed graph operation cannot depend on itself")
            }
            if !dependencies.insert(dependency) {
                bail!("Typed graph dependencies must be distinct")
            }
            if !indices.contains_key(dependency.as_str()) {
                bail!("Typed graph dependency refers to an unknown operation")
            }
        }
        validate_bindings(operation)?;
    }
    validate_acyclic(&document.operations, &indices)?;

    let template_sha256 = crate::store::sha256(encoded.as_bytes());
    let mut entries = Vec::with_capacity(document.operations.len());
    for operation in document.operations {
        let capability = operation.capability.module_capability();
        let work = Work::from_arguments(capability, &operation.arguments)?;
        let template = Template {
            capability,
            arguments: operation.arguments,
            dependencies: operation.depends_on,
            bindings: operation
                .bindings
                .into_iter()
                .map(|binding| {
                    Ok(Binding {
                        target: parse_pointer(&binding.target)?,
                        source: binding.source,
                        pointer: parse_pointer(&binding.pointer)?,
                        mode: binding.mode,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            template_sha256: template_sha256.clone(),
            translation_source_sha256: None,
            reasoning_arguments: false,
        };
        entries.push(Entry {
            id: operation.id,
            work,
            expert: operation.expert,
            capability_requirement: operation.capability_requirement,
            template: Some(template),
            provider_route: None,
        });
    }
    Ok(Some(entries))
}

fn validate_bindings(operation: &Operation) -> Result<()> {
    let dependencies: HashSet<&str> = operation.depends_on.iter().map(String::as_str).collect();
    let mut targets = Vec::<Vec<String>>::with_capacity(operation.bindings.len());
    for binding in &operation.bindings {
        validate_id(&binding.source)?;
        if !dependencies.contains(binding.source.as_str()) {
            bail!("Every typed graph binding source must be a declared dependency")
        }
        let target = parse_pointer(&binding.target)?;
        if target.is_empty() {
            bail!("Typed graph binding cannot replace the argument root")
        }
        match operation.capability.module_capability() {
            Capability::SqlQuery if target.first().is_some_and(|field| field == "query") => {
                bail!("Typed graph result data cannot be bound into a SQL query")
            }
            Capability::MathEvaluate
                if target.len() == 1
                    && target[0] == "expression"
                    && !matches!(binding.mode, BindingMode::NumericExpression) =>
            {
                bail!("Arithmetic expression bindings require numeric_expression mode")
            }
            _ => {}
        }
        validate_existing_pointer(&operation.arguments, &target)?;
        for existing in &targets {
            if is_prefix(existing, &target) || is_prefix(&target, existing) {
                bail!("Typed graph binding targets cannot overlap")
            }
        }
        targets.push(target);
        parse_pointer(&binding.pointer)?;
    }
    Ok(())
}

fn validate_acyclic(operations: &[Operation], indices: &HashMap<&str, usize>) -> Result<()> {
    fn visit(
        index: usize,
        operations: &[Operation],
        indices: &HashMap<&str, usize>,
        states: &mut [u8],
    ) -> Result<()> {
        match states[index] {
            1 => bail!("Typed graph dependencies contain a cycle"),
            2 => return Ok(()),
            _ => {}
        }
        states[index] = 1;
        for dependency in &operations[index].depends_on {
            let next = *indices
                .get(dependency.as_str())
                .ok_or_else(|| anyhow!("Typed graph dependency refers to an unknown operation"))?;
            visit(next, operations, indices, states)?;
        }
        states[index] = 2;
        Ok(())
    }

    let mut states = vec![0; operations.len()];
    for index in 0..operations.len() {
        visit(index, operations, indices, &mut states)?;
    }
    Ok(())
}

fn parse_pointer(pointer: &str) -> Result<Vec<String>> {
    if pointer.len() > MAX_POINTER_BYTES {
        bail!("Typed graph JSON pointer exceeds its byte limit")
    }
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    if !pointer.starts_with('/') {
        bail!("Typed graph pointer must be an RFC 6901 JSON pointer")
    }
    let mut parts = Vec::new();
    for raw in pointer[1..].split('/') {
        let mut decoded = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(character) = chars.next() {
            if character == '~' {
                match chars.next() {
                    Some('0') => decoded.push('~'),
                    Some('1') => decoded.push('/'),
                    _ => bail!("Typed graph pointer contains invalid RFC 6901 escaping"),
                }
            } else {
                decoded.push(character);
            }
        }
        parts.push(decoded);
        if parts.len() > MAX_POINTER_DEPTH {
            bail!("Typed graph JSON pointer exceeds its nesting limit")
        }
    }
    Ok(parts)
}

pub(super) fn validate_source_pointer(pointer: &str) -> Result<()> {
    parse_pointer(pointer).map(|_| ())
}

pub(super) fn select_raw_output(raw: &str, pointer: &str) -> Result<String> {
    if raw.len() > MAX_RESULT_BYTES {
        bail!("Typed graph dependency output exceeds its byte limit")
    }
    let _ = crate::strict_json::value(raw.as_bytes(), MAX_RESULT_BYTES)?;
    let path = parse_pointer(pointer)?;
    raw_pointer(raw, &path)
}

fn validate_existing_pointer(root: &Value, path: &[String]) -> Result<()> {
    if path.len() > MAX_POINTER_DEPTH {
        bail!("Typed graph target exceeds its nesting limit")
    }
    let mut current = root;
    for part in path {
        current = match current {
            Value::Object(object) => object
                .get(part)
                .ok_or_else(|| anyhow!("Typed graph target must already exist in arguments"))?,
            Value::Array(array) => array
                .get(canonical_index(part)?)
                .ok_or_else(|| anyhow!("Typed graph target array index is out of bounds"))?,
            _ => bail!("Typed graph target traverses a scalar argument"),
        };
    }
    Ok(())
}

fn replace_existing(root: &mut Value, path: &[String], replacement: Value) -> Result<()> {
    let (last, prefix) = path
        .split_last()
        .ok_or_else(|| anyhow!("Typed graph cannot replace the argument root"))?;
    let mut current = root;
    for part in prefix {
        current = match current {
            Value::Object(object) => object
                .get_mut(part)
                .ok_or_else(|| anyhow!("Typed graph target disappeared during resolution"))?,
            Value::Array(array) => array
                .get_mut(canonical_index(part)?)
                .ok_or_else(|| anyhow!("Typed graph target array index is out of bounds"))?,
            _ => bail!("Typed graph target traverses a scalar argument"),
        };
    }
    match current {
        Value::Object(object) => {
            let slot = object
                .get_mut(last)
                .ok_or_else(|| anyhow!("Typed graph target disappeared during resolution"))?;
            *slot = replacement;
        }
        Value::Array(array) => {
            let slot = array
                .get_mut(canonical_index(last)?)
                .ok_or_else(|| anyhow!("Typed graph target array index is out of bounds"))?;
            *slot = replacement;
        }
        _ => bail!("Typed graph target traverses a scalar argument"),
    }
    Ok(())
}

fn raw_pointer(raw: &str, path: &[String]) -> Result<String> {
    let mut current = raw.to_owned();
    for part in path {
        current = if current.trim_start().starts_with('{') {
            let object: BTreeMap<String, Box<RawValue>> = serde_json::from_str(&current)?;
            object
                .get(part)
                .ok_or_else(|| anyhow!("Typed graph source pointer does not exist"))?
                .get()
                .to_owned()
        } else if current.trim_start().starts_with('[') {
            let array: Vec<Box<RawValue>> = serde_json::from_str(&current)?;
            array
                .get(canonical_index(part)?)
                .ok_or_else(|| anyhow!("Typed graph source array index is out of bounds"))?
                .get()
                .to_owned()
        } else {
            bail!("Typed graph source pointer traverses a scalar")
        };
    }
    Ok(current)
}

fn canonical_index(value: &str) -> Result<usize> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("Typed graph array index must be a canonical nonnegative integer")
    }
    value
        .parse::<usize>()
        .map_err(|_| anyhow!("Typed graph array index is out of range"))
}

fn is_prefix(left: &[String], right: &[String]) -> bool {
    left.len() <= right.len() && left.iter().zip(right).all(|(a, b)| a == b)
}

fn is_supported_numeric_token(raw: &str) -> bool {
    let token = raw.trim();
    if token.is_empty() || token.bytes().any(|byte| matches!(byte, b'e' | b'E')) {
        return false;
    }
    token
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'.'))
        && token.bytes().any(|byte| byte.is_ascii_digit())
}

fn validate_id(id: &str) -> Result<()> {
    let bytes = id.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 32
        || !bytes[0].is_ascii_alphabetic()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        bail!("Typed graph IDs must start with an ASCII letter and contain at most 32 ASCII letters, digits, or underscores")
    }
    Ok(())
}

fn validate_capability_requirement(requirement: &str) -> Result<()> {
    if requirement.is_empty() || requirement.len() > 64 {
        bail!("Capability requirement must contain between one and 64 bytes")
    }
    semver::VersionReq::parse(requirement)
        .map(|_| ())
        .map_err(|_| anyhow!("Capability requirement must be valid semantic-version syntax"))
}
