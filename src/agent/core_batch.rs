//! Bounded same-response dependency graphs for host core computations.
use super::{core_memory_bindings, core_tools::Proposal};
use crate::fabric::modules::Capability;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_OPS: usize = 4;
const MAX_INPUTS: usize = 8;
const MAX_DEPS: usize = 3;
const MAX_BYTES: usize = 32_000;

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    #[default]
    Value,
    NumericExpression,
}
impl Mode {
    fn text(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::NumericExpression => "numeric_expression",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    target: String,
    #[serde(default)]
    operation_id: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    pointer: String,
    #[serde(default)]
    mode: Mode,
}
pub(super) struct Batch {
    document: Value,
    normalized: Vec<Value>,
    capabilities: Vec<Capability>,
    sizes: Vec<Value>,
    admissions: Vec<Value>,
}

impl Batch {
    pub(super) fn prepare(
        proposals: &[Proposal],
        call: usize,
        prior: &BTreeMap<String, String>,
    ) -> Result<Self> {
        if proposals.is_empty() || proposals.len() > MAX_OPS || !(1..=65_535).contains(&call) {
            bail!("Core graph count or call index is outside its bound");
        }
        let mut call_map = BTreeMap::new();
        let mut ids = Vec::new();
        for (i, p) in proposals.iter().enumerate() {
            if prior.contains_key(&format!("core_{call}_{i}"))
                || !valid_id(&p.call_id)
                || call_map
                    .insert(p.call_id.as_str(), format!("core_{call}_{i}"))
                    .is_some()
            {
                bail!("Invalid or duplicate core proposal identity");
            }
            ids.push(format!("core_{call}_{i}"));
        }
        let mut operations = Vec::new();
        let mut normalized = Vec::new();
        let mut capabilities = Vec::new();
        let mut sizes = Vec::new();
        let mut admissions = Vec::new();
        for (i, p) in proposals.iter().enumerate() {
            let mut raw = p
                .arguments
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("Core arguments must be an object"))?;
            let had_inputs = raw.contains_key("_cgf_inputs");
            let after: Vec<String> = raw
                .remove("_cgf_after")
                .map(serde_json::from_value)
                .transpose()?
                .unwrap_or_default();
            let inputs: Vec<Input> = raw
                .remove("_cgf_inputs")
                .map(serde_json::from_value)
                .transpose()?
                .unwrap_or_default();
            if after.len() > MAX_DEPS
                || inputs.len() > MAX_INPUTS
                || (had_inputs && inputs.is_empty())
            {
                bail!("Core graph dependency bound exceeded");
            }
            for input in raw_inputs(&p.arguments)? {
                let object = input
                    .as_object()
                    .ok_or_else(|| anyhow!("Core input must be an object"))?;
                match (object.get("operation_id"), object.get("call_id")) {
                    (Some(Value::String(_)), None) | (None, Some(Value::String(_))) => {}
                    _ => bail!("Core input must contain exactly one non-null source ID"),
                }
            }
            let mut deps = BTreeSet::new();
            for cid in after {
                let id = call_map
                    .get(cid.as_str())
                    .ok_or_else(|| anyhow!("Unknown sibling ordering dependency"))?
                    .clone();
                if id == ids[i] || !deps.insert(id) {
                    bail!("Self or duplicate sibling ordering dependency");
                }
            }
            let mut mem_inputs = Vec::new();
            let mut graph_bindings = Vec::new();
            let mut normalized_inputs = Vec::new();
            let mut siblings = Vec::new();
            for input in inputs {
                let sibling = match (&input.operation_id, &input.call_id) {
                    (Some(_), Some(_)) | (None, None) => {
                        bail!("Core input must name exactly one source")
                    }
                    (Some(id), None) => {
                        if !valid_operation(id) || !prior.contains_key(id) {
                            bail!("Core input is not finalized task memory")
                        }
                        None
                    }
                    (None, Some(cid)) => {
                        let id = call_map
                            .get(cid.as_str())
                            .ok_or_else(|| anyhow!("Unknown sibling data dependency"))?
                            .clone();
                        if id == ids[i] {
                            bail!("Core operation cannot depend on itself")
                        }
                        deps.insert(id.clone());
                        Some(id)
                    }
                };
                let source = input
                    .operation_id
                    .clone()
                    .or_else(|| sibling.clone())
                    .ok_or_else(|| anyhow!("Core input source missing"))?;
                let value = json!({"target":input.target,"operation_id":source,"pointer":input.pointer,"mode":input.mode.text()});
                normalized_inputs.push(value.clone());
                if let Some(source) = sibling {
                    graph_bindings.push(json!({"target":input.target,"source":source,"pointer":input.pointer,"mode":input.mode.text()}));
                    siblings.push(json!({"target":input.target,"source_operation":source,"source_pointer":input.pointer,"mode":input.mode.text()}));
                } else {
                    mem_inputs.push(value);
                }
            }
            if deps.len() > MAX_DEPS {
                bail!("Combined core dependencies exceed their bound");
            }
            let mut args = Value::Object(raw);
            if !mem_inputs.is_empty() {
                args.as_object_mut()
                    .ok_or_else(|| anyhow!("Arguments must be an object"))?
                    .insert("_cgf_inputs".into(), Value::Array(mem_inputs));
            }
            let (resolved, prior_proof) =
                core_memory_bindings::resolve(p.capability, &args, prior)?;
            if !normalized_inputs.is_empty() {
                let all = Value::Array(normalized_inputs.clone());
                core_memory_bindings::validate_targets(p.capability, &args, &all)?;
            }
            let mut size = prior_proof.as_array().cloned().unwrap_or_default();
            for binding in &normalized_inputs {
                let operation_id = binding["operation_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Core input operation ID is missing"))?;
                if !prior.contains_key(operation_id) {
                    size.push(json!({
                        "operation_id":operation_id,
                        "source_pointer":binding["pointer"],
                        "source_sha256":"0".repeat(64),
                        "selected_sha256":"0".repeat(64),
                        "target_pointer":binding["target"],
                        "mode":binding["mode"]
                    }));
                }
            }
            let size = if size.is_empty() {
                Value::Null
            } else {
                Value::Array(size)
            };
            let admission = json!({"checked_memory":prior_proof,"same_response_bindings":siblings});
            if serde_json::to_vec(&size)?.len() > 8192
                || serde_json::to_vec(&admission)?.len() > 8192
            {
                bail!("Core graph provenance exceeds its bound");
            }
            let mut op = json!({"id":ids[i],"capability":p.capability.as_str(),"arguments":resolved,"depends_on":deps.into_iter().collect::<Vec<_>>()});
            if !graph_bindings.is_empty() {
                op["bindings"] = Value::Array(graph_bindings);
            }
            operations.push(op);
            let mut complete = resolved;
            if !normalized_inputs.is_empty() {
                complete
                    .as_object_mut()
                    .ok_or_else(|| anyhow!("Arguments must be an object"))?
                    .insert("_cgf_inputs".into(), Value::Array(normalized_inputs));
            }
            normalized.push(complete);
            capabilities.push(p.capability);
            sizes.push(size);
            admissions.push(admission);
        }
        let document = json!({"operations":operations});
        if serde_json::to_vec(&document)?.len() > MAX_BYTES {
            bail!("Core graph exceeds its encoded size bound");
        }
        acyclic(&operations)?;
        Ok(Self {
            document,
            normalized,
            capabilities,
            sizes,
            admissions,
        })
    }
    pub(super) fn document(&self) -> &Value {
        &self.document
    }
    pub(super) fn sizing_provenance(&self, i: usize) -> Result<&Value> {
        self.sizes
            .get(i)
            .ok_or_else(|| anyhow!("Core graph sizing index is out of range"))
    }
    pub(super) fn admission_provenance(&self, i: usize) -> Result<&Value> {
        self.admissions
            .get(i)
            .ok_or_else(|| anyhow!("Core graph admission index is out of range"))
    }
    pub(super) fn resolve_completed(
        &self,
        i: usize,
        prior: &BTreeMap<String, String>,
        completed: &BTreeMap<String, String>,
    ) -> Result<(Value, Value)> {
        let mut outputs = prior.clone();
        for (id, value) in completed {
            if outputs.insert(id.clone(), value.clone()).is_some() {
                bail!("Core graph output collides with prior memory");
            }
        }
        core_memory_bindings::resolve(
            *self
                .capabilities
                .get(i)
                .ok_or_else(|| anyhow!("Core graph index out of range"))?,
            self.normalized
                .get(i)
                .ok_or_else(|| anyhow!("Core graph arguments missing"))?,
            &outputs,
        )
    }
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn raw_inputs(arguments: &Value) -> Result<Vec<Value>> {
    match arguments.get("_cgf_inputs") {
        None => Ok(Vec::new()),
        Some(Value::Array(inputs)) => Ok(inputs.clone()),
        Some(_) => bail!("Core input references must be an array"),
    }
}

fn valid_operation(s: &str) -> bool {
    s.strip_prefix("core_").is_some_and(|s| {
        let Some((a, b)) = s.split_once('_') else {
            return false;
        };
        a.parse::<usize>()
            .is_ok_and(|n| (1..=65535).contains(&n) && n.to_string() == a)
            && b.parse::<usize>()
                .is_ok_and(|n| n < 4 && n.to_string() == b)
    })
}
fn acyclic(ops: &[Value]) -> Result<()> {
    let ids: BTreeMap<&str, usize> = ops
        .iter()
        .enumerate()
        .filter_map(|(i, op)| Some((op["id"].as_str()?, i)))
        .collect();
    fn visit(i: usize, ops: &[Value], ids: &BTreeMap<&str, usize>, state: &mut [u8]) -> Result<()> {
        match state[i] {
            1 => bail!("Core graph contains a dependency cycle"),
            2 => return Ok(()),
            _ => (),
        }
        state[i] = 1;
        if let Some(ds) = ops[i]["depends_on"].as_array() {
            for d in ds {
                let id = d.as_str().ok_or_else(|| anyhow!("Invalid dependency"))?;
                visit(
                    *ids.get(id).ok_or_else(|| anyhow!("Missing dependency"))?,
                    ops,
                    ids,
                    state,
                )?;
            }
        }
        state[i] = 2;
        Ok(())
    }
    let mut state = vec![0; ops.len()];
    for i in 0..ops.len() {
        visit(i, ops, &ids, &mut state)?;
    }
    Ok(())
}
