//! Client-proposed typed graph translation into the host's exact graph syntax.
//!
//! This module accepts only operations and dataflow bindings. Host parsing
//! validates the graph, while dispatch and permissions remain separate.

use crate::{language::Interpretation, registry::ProviderCapability};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

const MAX_TRANSLATION_BYTES: usize = 16_000;
const MAX_OPERATIONS: usize = super::MAX_TYPED_OPERATIONS;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GraphDocument {
    operations: Vec<Operation>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    id: String,
    capability: ProviderCapability,
    arguments: Value,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    bindings: Vec<Binding>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    target: String,
    source: String,
    #[serde(default)]
    pointer: String,
    #[serde(default)]
    mode: BindingMode,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum BindingMode {
    #[default]
    Value,
    NumericExpression,
}

/// A bounded, validated graph representation suitable for the exact host parser.
/// The capability set is retained for caller-side routing consistency checks.
pub(in crate::agent) struct Translation {
    canonical_graph: String,
    caps: BTreeSet<String>,
    operation_ids: BTreeSet<String>,
}

impl Translation {
    pub(in crate::agent) fn canonical_query(&self) -> &str {
        &self.canonical_graph
    }

    pub(in crate::agent) fn capabilities(&self) -> impl Iterator<Item = &str> {
        self.caps.iter().map(String::as_str)
    }

    pub(in crate::agent) fn operation_ids(&self) -> impl Iterator<Item = &str> {
        self.operation_ids.iter().map(String::as_str)
    }
}

pub(in crate::agent) fn parse(
    value: Value,
    interpretation: &Interpretation,
    evidence_scope: bool,
) -> Result<Translation> {
    if interpretation.clarification.is_some() {
        bail!("Typed graph is incompatible with a clarification")
    }

    if serde_json::to_vec(&value)?.len() > MAX_TRANSLATION_BYTES {
        bail!("Typed graph translation exceeds its byte limit")
    }

    let document: GraphDocument = serde_json::from_value(value)?;
    if document.operations.is_empty() || document.operations.len() > MAX_OPERATIONS {
        bail!("Typed graph must contain between one and eight operations")
    }

    let caps: BTreeSet<String> = document
        .operations
        .iter()
        .map(|operation| operation.capability.as_str().to_owned())
        .collect();
    let operation_ids: BTreeSet<String> = document
        .operations
        .iter()
        .map(|operation| operation.id.clone())
        .collect();
    let required: BTreeSet<String> = interpretation
        .required_capabilities
        .iter()
        .cloned()
        .collect();
    let capability_scope_matches = if evidence_scope {
        !caps.is_empty() && caps.is_subset(&required)
    } else {
        caps == required
    };
    if !capability_scope_matches {
        bail!("Typed graph capability scope is incompatible with required capabilities")
    }
    if caps
        .iter()
        .any(|capability| interpretation.capability_versions.contains_key(capability))
    {
        bail!("Typed graph does not support a requested capability version")
    }

    let encoded = serde_json::to_string(&document)?;
    if encoded.len() > MAX_TRANSLATION_BYTES {
        bail!("Canonical typed graph exceeds its byte limit")
    }
    let canonical_graph = format!("typed-graph:{encoded}");
    match super::graphs::parse(&canonical_graph)? {
        Some(_) => {}
        None => return Err(anyhow!("Canonical typed graph was not recognized")),
    }

    Ok(Translation {
        canonical_graph,
        caps,
        operation_ids,
    })
}
