//! Bounded host-owned typed functions and task-scoped structured memory.
//!
//! Advertised schemas are descriptions for the model. They never authorize an
//! operation: callers must validate each proposal against the host work parser
//! and the independent policy kernel before execution.

use crate::{
    agent::types::{Function, Tool, ToolCall, Turn, MAX_CLIENT_TOOLS},
    fabric::modules::Capability,
    store::sha256,
    strict_json,
};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const MAX_ARGUMENT_BYTES: usize = 32_000;
const MAX_HOST_CALLS: usize = 4;
const MAX_MEMORY_RECORDS: usize = 16;
const MAX_MEMORY_BYTES: usize = 12_000;
const MEMORY_SCHEMA: u8 = 1;
const REASONING_ORIGIN: &str = "reasoning_model_arguments";

#[derive(Clone)]
pub(super) struct Catalog {
    tools: Vec<Tool>,
}

impl Catalog {
    /// Build host tool metadata only when the request permits automatic choice.
    /// The capability slice is supplied by trusted host code, never a model.
    pub(super) fn new(capabilities: &[Capability], request: &Turn) -> Result<Option<Self>> {
        if matches!(request.tool_choice.as_ref(), Some(choice)
            if choice == "none" || choice == "required" || choice.is_object())
        {
            return Ok(None);
        }

        let names = request
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut unique = BTreeSet::new();
        let mut tools = Vec::new();
        for capability in capabilities {
            let Some((name, description, parameters)) = definition(*capability) else {
                continue;
            };
            if !unique.insert(*capability) {
                continue;
            }
            if names.contains(name) {
                return Ok(None);
            }
            tools.push(Tool {
                kind: "function".to_owned(),
                function: Function {
                    name: name.to_owned(),
                    description: description.to_owned(),
                    parameters,
                    strict: Some(false),
                },
            });
        }
        if tools.is_empty() || request.tools.len().saturating_add(tools.len()) > MAX_CLIENT_TOOLS {
            return Ok(None);
        }
        Ok(Some(Self { tools }))
    }

    pub(super) fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// Return host proposals, `None` for an all-client response, and reject any
    /// response that mixes host-owned and client-owned function calls.
    pub(super) fn parse_calls(&self, calls: &[ToolCall]) -> Result<Option<Vec<Proposal>>> {
        let host_names = self
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<BTreeSet<_>>();
        let host_count = calls
            .iter()
            .filter(|call| host_names.contains(call.function.name.as_str()))
            .count();
        if host_count == 0 {
            return Ok(None);
        }
        if host_count != calls.len() {
            bail!("A completion cannot mix host-core and client tool calls")
        }
        if calls.len() > MAX_HOST_CALLS {
            bail!("Host-core call count exceeds the operation limit")
        }

        let mut ids = BTreeSet::new();
        let mut proposals = Vec::with_capacity(calls.len());
        for call in calls {
            if call.kind != "function"
                || !valid_identifier(&call.id, 64)
                || !ids.insert(call.id.as_str())
                || call.function.arguments.len() > MAX_ARGUMENT_BYTES
            {
                bail!("Invalid or duplicate host-core call identity")
            }
            let capability = capability_for_name(&call.function.name)
                .ok_or_else(|| anyhow::anyhow!("Unknown host-core function"))?;
            if !host_names.contains(call.function.name.as_str()) {
                bail!("Host-core function was not advertised")
            }
            let arguments =
                strict_json::value(call.function.arguments.as_bytes(), MAX_ARGUMENT_BYTES)?;
            if !arguments.is_object() {
                bail!("Host-core arguments must be a JSON object")
            }
            let arguments_json = serde_json::to_string(&arguments)?;
            let fingerprint =
                proposal_fingerprint(&call.function.name, capability.as_str(), &arguments_json);
            proposals.push(Proposal {
                call_id: call.id.clone(),
                name: call.function.name.clone(),
                capability,
                arguments,
                arguments_json,
                fingerprint,
            });
        }
        Ok(Some(proposals))
    }
}

#[derive(Clone, Debug)]
pub(super) struct Proposal {
    pub(super) call_id: String,
    pub(super) name: String,
    pub(super) capability: Capability,
    pub(super) arguments: Value,
    pub(super) arguments_json: String,
    pub(super) fingerprint: String,
}

/// A bounded audit record. JSON results remain strings so their numeric lexemes
/// survive persistence and can be independently hashed and revalidated.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub(super) operation_id: String,
    pub(super) model_call_index: usize,
    pub(super) proposal_id: String,
    pub(super) name: String,
    pub(super) capability: String,
    pub(super) fingerprint: String,
    pub(super) arguments_json: String,
    pub(super) result_json: String,
    pub(super) result_sha256: String,
    pub(super) criterion_sha256: String,
    pub(super) provider_sha256: String,
    pub(super) provider_identity: String,
    pub(super) origin: String,
    pub(super) original_semantics_verified: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Memory {
    schema_version: u8,
    root_sha256: String,
    records: Vec<Record>,
}

impl Memory {
    pub(super) fn new(root: &str) -> Result<Self> {
        if root.is_empty() {
            bail!("Task root is empty")
        }
        let memory = Self {
            schema_version: MEMORY_SCHEMA,
            root_sha256: sha256(root.as_bytes()),
            records: Vec::new(),
        };
        memory.validate_checkpoint()?;
        Ok(memory)
    }

    pub(super) fn validate(&self, root: &str) -> Result<()> {
        self.validate_checkpoint()?;
        if root.is_empty() || self.root_sha256 != sha256(root.as_bytes()) {
            bail!("Task memory belongs to a different root")
        }
        Ok(())
    }

    pub(super) fn validate_checkpoint(&self) -> Result<()> {
        if self.schema_version != MEMORY_SCHEMA
            || !valid_sha256(&self.root_sha256)
            || self.records.len() > MAX_MEMORY_RECORDS
        {
            bail!("Invalid task memory envelope")
        }
        let mut fingerprints = BTreeSet::new();
        let mut operation_ids = BTreeSet::new();
        for record in &self.records {
            validate_record(record)?;
            let fingerprint =
                proposal_fingerprint(&record.name, &record.capability, &record.arguments_json);
            if !fingerprints.insert(fingerprint) || !operation_ids.insert(&record.operation_id) {
                bail!("Duplicate operation in task memory")
            }
        }
        if self.encoded_len()? > MAX_MEMORY_BYTES {
            bail!("Task memory exceeds its byte limit")
        }
        Ok(())
    }

    pub(super) fn contains_fingerprint(&self, fingerprint: &str) -> bool {
        self.records.iter().any(|record| {
            proposal_fingerprint(&record.name, &record.capability, &record.arguments_json)
                == fingerprint
        })
    }

    pub(super) fn len(&self) -> usize {
        self.records.len()
    }

    pub(super) fn contains_call(&self, model_call_index: usize) -> bool {
        self.records
            .iter()
            .any(|record| record.model_call_index == model_call_index)
    }

    pub(super) fn validate_digest(&self, expected_root: &str) -> Result<()> {
        self.validate_checkpoint()?;
        if self.root_sha256 != expected_root {
            bail!("Task memory belongs to a different root")
        }
        Ok(())
    }

    pub(super) fn result_sha256(&self, operation_id: &str) -> Option<&str> {
        self.records
            .iter()
            .find(|record| record.operation_id == operation_id)
            .map(|record| record.result_sha256.as_str())
    }

    /// Exact finalized operation outputs available to checked-data references.
    /// The checkpoint is revalidated before exposing any result to the resolver.
    pub(super) fn outputs(&self) -> Result<std::collections::BTreeMap<String, String>> {
        self.validate_checkpoint()?;
        Ok(self
            .records
            .iter()
            .map(|record| (record.operation_id.clone(), record.result_json.clone()))
            .collect())
    }

    pub(super) fn append(&mut self, record: Record) -> Result<()> {
        validate_record(&record)?;
        let fingerprint =
            proposal_fingerprint(&record.name, &record.capability, &record.arguments_json);
        if self.contains_fingerprint(&fingerprint) {
            bail!("Operation already exists in task memory")
        }
        if self.records.len() >= MAX_MEMORY_RECORDS {
            bail!("Task memory record limit exceeded")
        }
        let mut candidate = self.clone();
        candidate.records.push(record);
        candidate.validate_checkpoint()?;
        *self = candidate;
        Ok(())
    }

    /// Serialize only the bounded, schema-checked task records for model context.
    /// Callers must still label this content as untrusted evidence.
    pub(super) fn context(&self) -> Result<String> {
        self.validate_checkpoint()?;
        let context = serde_json::to_string(self)?;
        if context.len() > MAX_MEMORY_BYTES {
            bail!("Task memory context exceeds its byte limit")
        }
        Ok(context)
    }

    pub(super) fn digest(&self) -> Result<String> {
        self.validate_checkpoint()?;
        Ok(sha256(serde_json::to_vec(self)?.as_slice()))
    }

    fn encoded_len(&self) -> Result<usize> {
        Ok(serde_json::to_vec(self)?.len())
    }
}

fn validate_record(record: &Record) -> Result<()> {
    let Some(capability) = capability_for_name(&record.name) else {
        bail!("Unknown host-core function in task memory")
    };
    if capability.as_str() != record.capability
        || !valid_identifier(&record.operation_id, 64)
        || !valid_identifier(&record.proposal_id, 64)
        || record.model_call_index == 0
        || record.model_call_index > 65_535
        || record.origin != REASONING_ORIGIN
        || record.original_semantics_verified
        || record.provider_identity.is_empty()
        || record.provider_identity.len() > 2_048
        || !valid_sha256(&record.result_sha256)
        || !valid_sha256(&record.criterion_sha256)
        || !valid_sha256(&record.provider_sha256)
        || record.provider_sha256 != sha256(record.provider_identity.as_bytes())
        || record.arguments_json.len() > MAX_ARGUMENT_BYTES
        || record.result_json.len() > 16_384
    {
        bail!("Invalid host-core memory record")
    }
    let prefix = format!("core_{}_", record.model_call_index);
    let suffix = record
        .operation_id
        .strip_prefix(&prefix)
        .ok_or_else(|| anyhow::anyhow!("Core record does not match its model-call identity"))?;
    let index = suffix.parse::<usize>()?;
    if index >= MAX_HOST_CALLS || index.to_string() != suffix {
        bail!("Core record operation index is invalid")
    }
    let arguments = strict_json::value(record.arguments_json.as_bytes(), MAX_ARGUMENT_BYTES)?;
    if !arguments.is_object() || serde_json::to_string(&arguments)? != record.arguments_json {
        bail!("Memory arguments must use canonical JSON")
    }
    let provider_identity = strict_json::value(record.provider_identity.as_bytes(), 2_048)?;
    if !provider_identity.is_object()
        || serde_json::to_string(&provider_identity)? != record.provider_identity
    {
        bail!("Provider identity must use canonical JSON")
    }
    if !valid_sha256(&record.fingerprint)
        || record.fingerprint
            != proposal_fingerprint(&record.name, &record.capability, &record.arguments_json)
    {
        bail!("Memory proposal fingerprint does not match its canonical arguments")
    }
    strict_json::value(record.result_json.as_bytes(), 16_384)?;
    if sha256(record.result_json.as_bytes()) != record.result_sha256 {
        bail!("Memory result digest does not match its exact JSON lexemes")
    }
    Ok(())
}

fn valid_sha256(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
}

fn proposal_fingerprint(name: &str, capability: &str, arguments_json: &str) -> String {
    sha256(format!("cgf-core-v1\0{name}\0{capability}\0{arguments_json}").as_bytes())
}

fn capability_for_name(name: &str) -> Option<Capability> {
    Some(match name {
        "cgf_core_math" => Capability::MathEvaluate,
        "cgf_core_sql" => Capability::SqlQuery,
        "cgf_core_sort" => Capability::SortingVerify,
        "cgf_core_order" => Capability::OrderingVerify,
        "cgf_core_path" => Capability::PathVerify,
        "cgf_core_constraints" => Capability::ConstraintsSolve,
        _ => return None,
    })
}

fn definition(capability: Capability) -> Option<(&'static str, &'static str, Value)> {
    let (name, description, mut parameters) = match capability {
        Capability::MathEvaluate => (
            "cgf_core_math",
            "Evaluate a bounded arithmetic expression with the host math module.",
            json!({"type":"object","properties":{"expression":{"type":"string","minLength":1,"maxLength":120}},"required":["expression"],"additionalProperties":false}),
        ),
        Capability::SqlQuery => (
            "cgf_core_sql",
            "Run a bounded read-only query over the explicitly supplied in-memory tables.",
            json!({"type":"object","properties":{"tables":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"columns":{"type":"array","items":{"type":"string"}},"rows":{"type":"array","items":{"type":"array"}}},"required":["name","columns","rows"],"additionalProperties":false}},"query":{"type":"string"}},"required":["tables","query"],"additionalProperties":false}),
        ),
        Capability::SortingVerify => (
            "cgf_core_sort",
            "Verify or compute a bounded ordering of the supplied values.",
            json!({"type":"object","properties":{"values":{"type":"array","minItems":1,"maxItems":256,"items":{"type":"integer"}},"descending":{"type":"boolean"}},"required":["values"],"additionalProperties":false}),
        ),
        Capability::OrderingVerify => (
            "cgf_core_order",
            "Solve a bounded dependency-ordering problem from supplied nodes and edges.",
            json!({"type":"object","properties":{"nodes":{"type":"array","items":{"type":"string"}},"before":{"type":"array","items":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":2}}},"required":["nodes","before"],"additionalProperties":false}),
        ),
        Capability::PathVerify => (
            "cgf_core_path",
            "Find a bounded path between the supplied endpoints in the supplied graph.",
            json!({"type":"object","properties":{"nodes":{"type":"array","items":{"type":"string"}},"edges":{"type":"array","items":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":2}},"start":{"type":"string"},"goal":{"type":"string"}},"required":["nodes","edges","start","goal"],"additionalProperties":false}),
        ),
        Capability::ConstraintsSolve => (
            "cgf_core_constraints",
            "Solve a bounded finite-domain constraint problem from supplied variables and constraints.",
            json!({"type":"object","properties":{"variables":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":32},"domain":{"type":"array","minItems":1,"maxItems":64,"uniqueItems":true,"items":{"type":"integer"}}},"required":["id","domain"],"additionalProperties":false}},"constraints":{"type":"array","minItems":1,"maxItems":128,"items":{"oneOf":[{"type":"object","properties":{"kind":{"const":"linear"},"terms":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"object","properties":{"variable":{"type":"string"},"coefficient":{"type":"integer"}},"required":["variable","coefficient"],"additionalProperties":false}},"relation":{"enum":["eq","ne","le","ge"]},"rhs":{"type":"integer"}},"required":["kind","terms","relation","rhs"],"additionalProperties":false},{"type":"object","properties":{"kind":{"const":"all_different"},"variables":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["kind","variables"],"additionalProperties":false}]}}},"required":["variables","constraints"],"additionalProperties":false}),
        ),
    };
    parameters["properties"]["_cgf_inputs"] = json!({
        "type":"array",
        "minItems":1,
        "maxItems":8,
        "description":"Optional checked-result data bindings. Each source selects exactly one earlier finalized core_* memory operation ID or a sibling function call ID in this same response. Same-batch dependencies wait for checked results; future and unknown sources fail.",
        "items":{
            "type":"object",
            "properties":{
                "target":{"type":"string","minLength":1,"maxLength":256,"description":"RFC 6901 pointer to an existing non-root field in this function's arguments."},
                "operation_id":{"type":"string","pattern":"^core_[1-9][0-9]*_[0-3]$"},
                "call_id":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9_-]{1,64}$","description":"Sibling function call ID in the same model response; its operation must finish with checked evidence first."},
                "pointer":{"type":"string","maxLength":256,"description":"RFC 6901 pointer into the finalized result; empty string selects the whole result."},
                "mode":{"type":"string","enum":["value","numeric_expression"],"default":"value"}
            },
            "required":["target"],
            "oneOf":[
                {"required":["operation_id"],"not":{"required":["call_id"]}},
                {"required":["call_id"],"not":{"required":["operation_id"]}}
            ],
            "additionalProperties":false
        }
    });
    parameters["properties"]["_cgf_after"] = json!({
        "type":"array",
        "maxItems":3,
        "uniqueItems":true,
        "description":"Optional ordering-only dependencies on sibling function call IDs in this response. CGF waits for each dependency to produce checked evidence before running this operation.",
        "items":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9_-]{1,64}$"}
    });
    Some((name, description, parameters))
}
