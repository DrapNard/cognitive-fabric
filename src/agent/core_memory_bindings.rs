//! Bounded references from new core arguments to prior checked results.
//!
//! This module only projects JSON values already stored in checked task memory.
//! It does not interpret memory as instructions or grant a capability.

use crate::{fabric::modules::Capability, strict_json};
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MAX_ARGUMENTS: usize = 32_000;
const MAX_INPUTS: usize = 8;
const MAX_POINTER_BYTES: usize = 256;
const MAX_POINTER_DEPTH: usize = 8;
const MAX_COPY_BYTES: usize = 32_000;
const MAX_SOURCE_BYTES: usize = 16_384;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    target: String,
    operation_id: String,
    #[serde(default)]
    pointer: String,
    #[serde(default)]
    mode: Mode,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    #[default]
    Value,
    NumericExpression,
}

/// Resolve the reserved `_cgf_inputs` selectors against prior checked outputs.
/// Returns the validated arguments and bounded provenance hashes for auditing.
pub(super) fn resolve(
    capability: Capability,
    arguments: &Value,
    outputs: &BTreeMap<String, String>,
) -> Result<(Value, Value)> {
    if serde_json::to_vec(arguments)?.len() > MAX_ARGUMENTS {
        bail!("Core arguments exceed the input size limit")
    }
    let object = arguments
        .as_object()
        .ok_or_else(|| anyhow!("Core arguments must be an object"))?;
    let raw_inputs = object.get("_cgf_inputs");
    let mut resolved = arguments.clone();
    let resolved_object = resolved
        .as_object_mut()
        .ok_or_else(|| anyhow!("Core arguments must be an object"))?;
    resolved_object.remove("_cgf_inputs");
    let Some(raw_inputs) = raw_inputs else {
        return Ok((resolved, Value::Null));
    };
    let inputs: Vec<Input> = serde_json::from_value(raw_inputs.clone())?;
    if inputs.is_empty() || inputs.len() > MAX_INPUTS {
        bail!("Core input selector count is outside its bound")
    }

    let mut targets: Vec<Vec<String>> = Vec::with_capacity(inputs.len());
    let mut provenance = Vec::with_capacity(inputs.len());
    let mut copied_bytes = 0usize;
    let mut sources = BTreeSet::new();
    let mut source_bytes = 0usize;
    for input in inputs {
        let target = pointer(&input.target)?;
        if target.is_empty() {
            bail!("Core input target cannot replace the argument root")
        }
        if capability == Capability::SqlQuery && target.first().is_some_and(|key| key == "query") {
            bail!("Checked result data cannot be bound into a SQL query")
        }
        if capability == Capability::MathEvaluate
            && target.as_slice() == ["expression"]
            && !matches!(input.mode, Mode::NumericExpression)
        {
            bail!("Arithmetic expression bindings require numeric_expression mode")
        }
        if matches!(input.mode, Mode::NumericExpression)
            && (capability != Capability::MathEvaluate || target.as_slice() != ["expression"])
        {
            bail!("numeric_expression is only valid for arithmetic expressions")
        }
        validate_target(&resolved, &target)?;
        if targets.iter().any(|prior| overlaps(prior, &target)) {
            bail!("Core input targets cannot overlap")
        }
        targets.push(target.clone());

        if input.operation_id.len() > 64 || !valid_operation_id(&input.operation_id) {
            bail!("Core input references an invalid operation identity")
        }
        let raw_output = outputs
            .get(&input.operation_id)
            .ok_or_else(|| anyhow!("Core input references unavailable checked evidence"))?;
        if raw_output.len() > MAX_SOURCE_BYTES {
            bail!("Checked evidence exceeds the projection size limit")
        }
        if sources.insert(input.operation_id.clone()) {
            source_bytes = source_bytes.saturating_add(raw_output.len());
            if source_bytes > MAX_COPY_BYTES {
                bail!("Checked evidence sources exceed their aggregate size limit")
            }
        }
        let selected = crate::agent::typed_experts::select_raw_output(raw_output, &input.pointer)?;
        copied_bytes = copied_bytes.saturating_add(selected.len());
        if copied_bytes > MAX_COPY_BYTES {
            bail!("Core input projections exceed their aggregate size limit")
        }
        let replacement = match input.mode {
            Mode::Value => strict_json::value(selected.as_bytes(), MAX_SOURCE_BYTES)?,
            Mode::NumericExpression => {
                if !is_numeric_token(&selected) {
                    bail!("numeric_expression requires a plain JSON number")
                }
                Value::String(format!("({selected})"))
            }
        };
        let selected_sha256 = crate::store::sha256(selected.as_bytes());
        replace(&mut resolved, &target, replacement)?;
        provenance.push(serde_json::json!({
            "operation_id":input.operation_id,
            "source_pointer":input.pointer,
            "source_sha256":crate::store::sha256(raw_output.as_bytes()),
            "selected_sha256":selected_sha256,
            "target_pointer":input.target,
            "mode":match input.mode { Mode::Value => "value", Mode::NumericExpression => "numeric_expression" }
        }));
    }
    if serde_json::to_vec(&resolved)?.len() > MAX_ARGUMENTS {
        bail!("Resolved core arguments exceed their size limit")
    }
    let proof = Value::Array(provenance);
    if serde_json::to_vec(&proof)?.len() > 8_192 {
        bail!("Core input provenance exceeds its size limit")
    }
    Ok((resolved, proof))
}

fn pointer(value: &str) -> Result<Vec<String>> {
    if value.len() > MAX_POINTER_BYTES {
        bail!("Core JSON pointer exceeds its byte limit")
    }
    if value.is_empty() {
        return Ok(Vec::new());
    }
    if !value.starts_with('/') {
        bail!("Core pointer must be an RFC 6901 JSON pointer")
    }
    let mut output = Vec::new();
    for part in value[1..].split('/') {
        let mut decoded = String::with_capacity(part.len());
        let mut chars = part.chars();
        while let Some(ch) = chars.next() {
            if ch == '~' {
                match chars.next() {
                    Some('0') => decoded.push('~'),
                    Some('1') => decoded.push('/'),
                    _ => bail!("Core pointer contains invalid RFC 6901 escaping"),
                }
            } else {
                decoded.push(ch);
            }
        }
        output.push(decoded);
        if output.len() > MAX_POINTER_DEPTH {
            bail!("Core JSON pointer exceeds its nesting limit")
        }
    }
    // Keep one canonical implementation of the accepted pointer grammar.
    crate::agent::typed_experts::validate_source_pointer(value)?;
    Ok(output)
}

fn validate_target(root: &Value, path: &[String]) -> Result<()> {
    let mut current = root;
    for part in path {
        current = match current {
            Value::Object(object) => object
                .get(part)
                .ok_or_else(|| anyhow!("Core target must already exist in arguments"))?,
            Value::Array(array) => array
                .get(array_index(part)?)
                .ok_or_else(|| anyhow!("Core target array index is out of bounds"))?,
            _ => bail!("Core target traverses a scalar argument"),
        };
    }
    Ok(())
}

fn replace(root: &mut Value, path: &[String], replacement: Value) -> Result<()> {
    let (last, parents) = path
        .split_last()
        .ok_or_else(|| anyhow!("Core target cannot replace the argument root"))?;
    let mut current = root;
    for part in parents {
        current = match current {
            Value::Object(object) => object
                .get_mut(part)
                .ok_or_else(|| anyhow!("Core target disappeared during resolution"))?,
            Value::Array(array) => array
                .get_mut(array_index(part)?)
                .ok_or_else(|| anyhow!("Core target index is out of bounds"))?,
            _ => bail!("Core target traverses a scalar argument"),
        };
    }
    match current {
        Value::Object(object) => {
            *object
                .get_mut(last)
                .ok_or_else(|| anyhow!("Core target disappeared during resolution"))? = replacement;
        }
        Value::Array(array) => {
            *array
                .get_mut(array_index(last)?)
                .ok_or_else(|| anyhow!("Core target index is out of bounds"))? = replacement;
        }
        _ => bail!("Core target traverses a scalar argument"),
    }
    Ok(())
}

fn array_index(value: &str) -> Result<usize> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("Core array index must be a canonical nonnegative integer")
    }
    value
        .parse::<usize>()
        .map_err(|_| anyhow!("Core array index is out of range"))
}

fn overlaps(left: &[String], right: &[String]) -> bool {
    fn prefix(a: &[String], b: &[String]) -> bool {
        a.len() <= b.len() && a.iter().zip(b).all(|(x, y)| x == y)
    }
    prefix(left, right) || prefix(right, left)
}

fn valid_operation_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("core_") else {
        return false;
    };
    let Some((call, index)) = rest.split_once('_') else {
        return false;
    };
    call.parse::<usize>()
        .is_ok_and(|number| (1..=65_535).contains(&number) && number.to_string() == call)
        && index
            .parse::<usize>()
            .is_ok_and(|number| number < 4 && number.to_string() == index)
}

fn is_numeric_token(raw: &str) -> bool {
    let token = raw.trim();
    if token.is_empty() || token.bytes().any(|byte| matches!(byte, b'e' | b'E')) {
        return false;
    }
    token
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'.'))
        && token.bytes().any(|byte| byte.is_ascii_digit())
}
