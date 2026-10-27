//! Optional language-interface translation into the existing typed client route.
//!
//! This envelope is local to the client collective. It deliberately leaves the
//! public language interpretation and Engine response formats unchanged.

use crate::{fabric::modules::Capability, language};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::Value;

const MAX_TRANSLATION_BYTES: usize = 16_000;
const MAX_SQL_ARGUMENT_BYTES: usize = 32_000;
const MAX_STRUCTURED_ARGUMENT_BYTES: usize = 10_000;

/// Contract appended only when the host has an eligible typed route and budget.
/// The model may translate a clear request, but it cannot choose modules,
/// permissions, providers, or tools.
pub(in crate::agent) const INTERFACE_TRANSLATION_CONTRACT: &str = r#"
Optional typed_goal: use only for a single operation when the user's request unambiguously matches one
listed schema and all required data is present. Otherwise omit typed_goal or set
it to null, preserve the user's objective and constraints, and ask for missing
information through clarification. Never guess tables, rows, domains, variables,
edges, nodes, or other task data. This field is an advisory translation only; it
does not select tools, models, providers, modules, or permissions.

Optional typed_batch: {"operations":[{"id":"sum","capability":"math.evaluate","arguments":{"expression":"..."}}]}
Use it only for 1 to 4 independent operations, and only when every operation's
required data is present. Operations cannot depend on each other, refer to prior
outputs, or be partially selected. Never guess or omit an operation needed to
fulfill the request; omit typed_batch when the complete independent operation
set is ambiguous. The host advertises its operation budget and pinned
capabilities separately. This is an advisory translation only; it does not
select tools, models, providers, modules, or permissions.

Optional typed_graph: {"operations":[{"id":"first","capability":"sorting.verify","arguments":{"values":[3,1]}},{"id":"next","capability":"sorting.verify","arguments":{"values":[0],"descending":true},"depends_on":["first"],"bindings":[{"target":"/values","source":"first","pointer":""}]}]}
Use typed_graph only for a complete, unambiguous 1-to-4-operation DAG. Every binding
source must be in depends_on; targets replace existing argument values. Concrete
schema-valid placeholders are structural only and must be overwritten by their
bindings before use; never invent missing real input data. Result data cannot
replace SQL query text; math /expression bindings must use numeric_expression mode
with a whole numeric source token. Graphs cannot choose experts, modules, providers,
models, permissions, or capability versions. Omit typed_graph on ambiguity and
request clarification instead. typed_goal, typed_batch and typed_graph are mutually
exclusive; include exactly the graph's capability set in required_capabilities.

Optional typed_usage: "final" or "evidence" (default "final"). Use "evidence"
only when a complete typed computation is an intermediate input to the remaining
reasoning workflow, and include exactly one complete typed_goal, typed_batch or
typed_graph. Continue the original objective after that computation. Preserve all
source data and constraints; never invent arguments or treat a checked typed result
as proof of the user's broader meaning. Omit evidence mode when the typed result
alone fulfills the request. This field selects no model, provider, tool or permission.

math.evaluate: {"capability":"math.evaluate","arguments":{"expression":"..."}}
sql.query: {"capability":"sql.query","arguments":{"tables":[{"name":"...","columns":["..."],"rows":[[...]]}],"query":"SELECT ..."}}
sorting.verify: {"capability":"sorting.verify","arguments":{"values":[1,2],"descending":false}}
ordering.verify: {"capability":"ordering.verify","arguments":{"nodes":["a","b"],"before":[["a","b"]]}}
path.verify: {"capability":"path.verify","arguments":{"nodes":["a","b"],"edges":[["a","b"]],"start":"a","goal":"b"}}
constraints.solve: {"capability":"constraints.solve","arguments":{"variables":[{"id":"x","domain":[1,2]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1}]}}
"#;

/// A strict envelope around the stable public language interpretation.
pub(in crate::agent) struct Parsed {
    pub(in crate::agent) interpretation: language::Interpretation,
    pub(in crate::agent) typed_goal: Option<Translation>,
    pub(in crate::agent) typed_batch: Option<Vec<(String, Translation)>>,
    pub(in crate::agent) typed_graph: Option<super::graph_translation::Translation>,
    pub(in crate::agent) typed_evidence: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TypedGoal {
    capability: TypedCapability,
    arguments: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TypedBatch {
    operations: Vec<TypedOperation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TypedOperation {
    id: String,
    capability: TypedCapability,
    arguments: Value,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
enum TypedCapability {
    #[serde(rename = "math.evaluate")]
    MathEvaluate,
    #[serde(rename = "sql.query")]
    SqlQuery,
    #[serde(rename = "sorting.verify")]
    SortingVerify,
    #[serde(rename = "ordering.verify")]
    OrderingVerify,
    #[serde(rename = "path.verify")]
    PathVerify,
    #[serde(rename = "constraints.solve")]
    ConstraintsSolve,
}

impl TypedCapability {
    fn as_str(self) -> &'static str {
        match self {
            Self::MathEvaluate => "math.evaluate",
            Self::SqlQuery => "sql.query",
            Self::SortingVerify => "sorting.verify",
            Self::OrderingVerify => "ordering.verify",
            Self::PathVerify => "path.verify",
            Self::ConstraintsSolve => "constraints.solve",
        }
    }

    fn capability(self) -> Capability {
        match self {
            Self::MathEvaluate => Capability::MathEvaluate,
            Self::SqlQuery => Capability::SqlQuery,
            Self::SortingVerify => Capability::SortingVerify,
            Self::OrderingVerify => Capability::OrderingVerify,
            Self::PathVerify => Capability::PathVerify,
            Self::ConstraintsSolve => Capability::ConstraintsSolve,
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::MathEvaluate => "calc:",
            Self::SqlQuery => "sql:",
            Self::SortingVerify => "sort:",
            Self::OrderingVerify => "order:",
            Self::PathVerify => "path:",
            Self::ConstraintsSolve => "csp:",
        }
    }
}

/// Validated model translation that can only express one existing typed route.
/// Fields stay private so callers cannot forge a capability/argument pairing.
pub(in crate::agent) struct Translation {
    capability: TypedCapability,
    arguments: Value,
}

impl Parsed {
    /// Parse the existing interpretation plus the optional client-only extension.
    /// The shared strict parser rejects duplicate keys and surrounding prose;
    /// the stable public Interpretation parser remains authoritative for its
    /// original fields and bounds.
    pub(in crate::agent) fn parse(text: &str) -> Result<Self> {
        let mut envelope: Value = crate::structured::parse(text, MAX_TRANSLATION_BYTES)?;
        let object = envelope
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Language response must be a JSON object"))?;
        let typed_value = object.remove("typed_goal");
        let batch_value = object.remove("typed_batch");
        let graph_value = object
            .remove("typed_graph")
            .filter(|value| !value.is_null());
        let typed_usage = match object.remove("typed_usage") {
            None => "final",
            Some(Value::String(value)) if value == "final" || value == "evidence" => {
                if value == "evidence" {
                    "evidence"
                } else {
                    "final"
                }
            }
            Some(_) => bail!("Typed usage must be either final or evidence"),
        };
        let typed_evidence = typed_usage == "evidence";
        let typed_goal = match typed_value {
            None | Some(Value::Null) => None,
            Some(value) => {
                let goal: TypedGoal = serde_json::from_value(value)?;
                if !goal.arguments.is_object() {
                    bail!("Typed goal arguments must be a JSON object")
                }
                Some(goal)
            }
        };
        let typed_batch = match batch_value {
            None | Some(Value::Null) => None,
            Some(value) => {
                let batch: TypedBatch = serde_json::from_value(value)?;
                if !(1..=4).contains(&batch.operations.len()) {
                    bail!("Typed batch must contain between one and four operations")
                }
                let mut ids = std::collections::HashSet::with_capacity(batch.operations.len());
                let mut translated = Vec::with_capacity(batch.operations.len());
                for operation in batch.operations {
                    if !valid_operation_id(&operation.id) || !ids.insert(operation.id.clone()) {
                        bail!("Typed batch operation IDs must be unique valid ASCII identifiers")
                    }
                    if !operation.arguments.is_object() {
                        bail!("Typed batch operation arguments must be a JSON object")
                    }
                    translated.push((
                        operation.id,
                        Translation {
                            capability: operation.capability,
                            arguments: operation.arguments,
                        },
                    ));
                }
                Some(translated)
            }
        };
        if usize::from(typed_goal.is_some())
            + usize::from(typed_batch.is_some())
            + usize::from(graph_value.is_some())
            > 1
        {
            bail!("Typed goal, typed batch and typed graph are mutually exclusive")
        }
        if typed_evidence && typed_goal.is_none() && typed_batch.is_none() && graph_value.is_none()
        {
            bail!("Evidence usage requires a complete typed operation")
        }

        let interpretation_json = serde_json::to_string(&envelope)?;
        let interpretation = language::Interpretation::parse(&interpretation_json)?;

        let typed_goal = typed_goal
            .map(|goal| {
                let capability = goal.capability;
                if interpretation.clarification.is_some()
                    || interpretation.required_capabilities.len() != 1
                    || !interpretation
                        .required_capabilities
                        .iter()
                        .any(|required| required == capability.as_str())
                {
                    bail!("Typed goal is incompatible with the language interpretation")
                }
                if interpretation
                    .capability_versions
                    .contains_key(capability.as_str())
                {
                    bail!("Typed route does not support a requested capability version")
                }
                Ok(Translation {
                    capability,
                    arguments: goal.arguments,
                })
            })
            .transpose()?;

        let typed_batch = typed_batch
            .map(|entries| {
                if interpretation.clarification.is_some() {
                    bail!("Typed batch is incompatible with a clarification")
                }
                let batch_capabilities: std::collections::HashSet<_> = entries
                    .iter()
                    .map(|(_, translation)| translation.capability.as_str())
                    .collect();
                let required_capabilities: std::collections::HashSet<_> = interpretation
                    .required_capabilities
                    .iter()
                    .map(String::as_str)
                    .collect();
                if batch_capabilities != required_capabilities {
                    bail!("Typed batch capabilities do not match required capabilities")
                }
                if entries.iter().any(|(_, translation)| {
                    interpretation
                        .capability_versions
                        .contains_key(translation.capability.as_str())
                }) {
                    bail!("Typed batch does not support a requested capability version")
                }
                Ok(entries)
            })
            .transpose()?;

        let typed_graph = graph_value
            .map(|value| super::graph_translation::parse(value, &interpretation))
            .transpose()?;
        Ok(Self {
            interpretation,
            typed_goal,
            typed_batch,
            typed_graph,
            typed_evidence,
        })
    }
}

fn valid_operation_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=32).contains(&bytes.len())
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

impl Translation {
    pub(in crate::agent) fn capability(&self) -> Capability {
        self.capability.capability()
    }

    /// Convert the bounded structured arguments to the existing exact-prefix
    /// host parser input. Domain validation remains in `Work::from_query`.
    pub(in crate::agent) fn canonical_query(&self) -> Result<String> {
        let object = self
            .arguments
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Typed goal arguments must be a JSON object"))?;

        if self.capability == TypedCapability::MathEvaluate {
            if object.len() != 1 {
                bail!("Arithmetic arguments must contain only expression")
            }
            let expression = object["expression"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Arithmetic expression must be a string"))?;
            if expression.trim().is_empty() || expression.len() > 120 {
                bail!("Arithmetic expression is empty or exceeds its byte limit")
            }
            return Ok(format!("calc:{expression}"));
        }

        let encoded = serde_json::to_string(&self.arguments)?;
        let max_bytes = if self.capability == TypedCapability::SqlQuery {
            MAX_SQL_ARGUMENT_BYTES
        } else {
            MAX_STRUCTURED_ARGUMENT_BYTES
        };
        if encoded.len() > max_bytes {
            bail!("Typed structured arguments exceed their byte limit")
        }
        Ok(format!("{}{encoded}", self.capability.prefix()))
    }
}
