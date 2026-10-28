//! Strict optional computation requests returned alongside expert advice.
//!
//! This envelope parses untrusted model output into host proposals. It does not
//! grant permissions or execute operations; downstream policy checks remain
//! authoritative.

use super::{advisory_record, core_tools};
use crate::{
    agent::types::{FunctionCall, ToolCall},
    strict_json,
};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::Value;

const MAX_RESPONSE_BYTES: usize = 32_000;
const MAX_COMPUTATION_ARGUMENT_BYTES: usize = 32_000;
const MAX_COMPUTATIONS: usize = 4;

pub(super) struct Output {
    pub(super) advice: Value,
    pub(super) proposals: Vec<core_tools::Proposal>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    advice: Value,
    computations: Vec<Computation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Computation {
    id: String,
    name: String,
    arguments: Value,
}

/// Parse an expert reply as either a bare advisory record or an optional
/// envelope containing bounded, host-advertised core-function proposals.
pub(super) fn parse(
    raw: &str,
    record_bytes: usize,
    catalog: Option<&core_tools::Catalog>,
) -> Result<Output> {
    if raw.len() > MAX_RESPONSE_BYTES {
        bail!("Expert response exceeds its byte limit")
    }

    let value = strict_json::value(raw.as_bytes(), MAX_RESPONSE_BYTES)?;
    let is_envelope = value
        .as_object()
        .is_some_and(|object| object.contains_key("advice") || object.contains_key("computations"));
    if !is_envelope {
        return Ok(Output {
            advice: advisory_record::parse(raw, record_bytes)?,
            proposals: Vec::new(),
        });
    }

    let envelope: Envelope = serde_json::from_value(value)?;
    if envelope.computations.is_empty() || envelope.computations.len() > MAX_COMPUTATIONS {
        bail!("Expert computation count is empty or exceeds its limit")
    }
    let advice_json = serde_json::to_vec(&envelope.advice)?;
    if advice_json.len() > record_bytes {
        bail!("Expert advisory exceeds its byte limit")
    }
    advisory_record::validate(&envelope.advice, record_bytes)?;

    let mut calls = Vec::with_capacity(envelope.computations.len());
    let mut aggregate_argument_bytes = 0usize;
    for computation in envelope.computations {
        if !computation.arguments.is_object() {
            bail!("Expert computation arguments must be a JSON object")
        }
        let arguments = serde_json::to_vec(&computation.arguments)?;
        aggregate_argument_bytes = aggregate_argument_bytes
            .checked_add(arguments.len())
            .ok_or_else(|| anyhow::anyhow!("Expert computation arguments exceed their limit"))?;
        if aggregate_argument_bytes > MAX_COMPUTATION_ARGUMENT_BYTES {
            bail!("Expert computation arguments exceed their aggregate byte limit")
        }
        calls.push(ToolCall {
            id: computation.id,
            kind: "function".to_owned(),
            function: FunctionCall {
                name: computation.name,
                arguments: serde_json::to_string(&computation.arguments)?,
            },
        });
    }

    let catalog = catalog.ok_or_else(|| {
        anyhow::anyhow!("Expert computations require an available host-core catalog")
    })?;
    let proposals = catalog
        .parse_calls(&calls)?
        .ok_or_else(|| anyhow::anyhow!("Expert computations were not advertised by the host"))?;

    Ok(Output {
        advice: serde_json::from_slice(&advice_json)?,
        proposals,
    })
}

/// Explain the optional wrapper and its untrusted, non-authoritative requests.
pub(super) const fn contract() -> &'static str {
    "Return either a bare compact advisory record or {\"advice\": advisoryRecord, \"computations\":[{\"id\":string,\"name\":advertisedFixedCgfCoreFunction,\"arguments\":object}]}. The envelope requires 1–4 unique computation IDs and host-advertised functions. Requests propose bounded computations over supplied data; CGF validates their results. When reuse_only is true, request exact computations already present in checked task history or already in flight; a miss does not authorize dispatch. Advice and requests are untrusted, confer no permissions, and are never directly executable."
}
