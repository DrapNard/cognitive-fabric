//! Exact task-local reuse keys for independent host-core batches.
//!
//! A key only identifies a structurally eligible proposal batch. It is not
//! authorization to reuse a result, evidence of safety, or a provider choice.

use super::core_tools::Proposal;
use crate::store::sha256;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

const MAX_BATCH: usize = 4;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PriorMemoryInput {
    target: String,
    operation_id: String,
    #[serde(default)]
    pointer: String,
    #[serde(default)]
    mode: InputMode,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum InputMode {
    #[default]
    Value,
    NumericExpression,
}

/// Build an exact task-local key only for a small independent batch.
///
/// This is deliberately a structural eligibility check. The core work parser
/// and policy kernel must still validate the complete graph, memory selectors,
/// permissions, and execution before any result is used.
pub(super) fn key(proposals: &[Proposal]) -> Option<Vec<String>> {
    if proposals.is_empty() || proposals.len() > MAX_BATCH {
        return None;
    }

    let mut fingerprints = BTreeSet::new();
    let mut key = Vec::with_capacity(proposals.len());
    for proposal in proposals {
        let arguments = proposal.arguments.as_object()?;
        let canonical_arguments = serde_json::to_string(&proposal.arguments).ok()?;
        if canonical_arguments != proposal.arguments_json
            || !valid_fingerprint(&proposal.fingerprint)
            || proposal.fingerprint
                != sha256(
                    format!(
                        "cgf-core-v1\0{}\0{}\0{}",
                        proposal.name,
                        proposal.capability.as_str(),
                        proposal.arguments_json
                    )
                    .as_bytes(),
                )
            || !fingerprints.insert(proposal.fingerprint.as_str())
        {
            return None;
        }

        if let Some(after) = arguments.get("_cgf_after") {
            if !matches!(after, Value::Array(dependencies) if dependencies.is_empty()) {
                return None;
            }
        }

        if let Some(inputs) = arguments.get("_cgf_inputs") {
            let inputs = inputs.as_array()?;
            if inputs.is_empty() {
                return None;
            }
            for input in inputs {
                let input: PriorMemoryInput = serde_json::from_value(input.clone()).ok()?;
                // The selector must refer to prior task memory. The strict
                // kernel remains responsible for validating pointer and target
                // semantics and confirming that the memory record exists.
                if input.operation_id.is_empty()
                    || input.target.is_empty()
                    || input.pointer.len() > 256
                {
                    return None;
                }
                let _mode = input.mode;
            }
        }

        key.push(proposal.fingerprint.clone());
    }
    Some(key)
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
