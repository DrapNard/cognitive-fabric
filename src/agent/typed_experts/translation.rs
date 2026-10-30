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
Use it only for 1 to 8 independent operations, and only when every operation's
required data is present. Operations cannot depend on each other, refer to prior
outputs, or be partially selected. Never guess or omit an operation needed to
fulfill the chosen computation; omit typed_batch when its complete independent
operation set is ambiguous. In final usage it must fulfill the full request; in
evidence usage it must fulfill the selected intermediate computation. The host advertises its operation budget and pinned
capabilities separately. This is an advisory translation only; it does not
select tools, models, providers, modules, or permissions.

Optional typed_graph: {"operations":[{"id":"first","capability":"sorting.verify","arguments":{"values":[3,1]}},{"id":"next","capability":"sorting.verify","arguments":{"values":[0],"descending":true},"depends_on":["first"],"bindings":[{"target":"/values","source":"first","pointer":""}]}]}
Use typed_graph only for a complete, unambiguous 1-to-8-operation DAG; each operation
may have at most seven direct dependencies. Every binding
source must be in depends_on; targets replace existing argument values. Concrete
schema-valid placeholders are structural only and must be overwritten by their
bindings before use; never invent missing real input data. Result data cannot
replace SQL query text; math /expression bindings must use numeric_expression mode
with a whole numeric source token. Graphs cannot choose experts, modules, providers,
models, permissions, or capability versions. Omit typed_graph on ambiguity and
request clarification instead. typed_goal, typed_batch and typed_graph are mutually
exclusive. Final usage must cover all required capabilities; evidence usage may
cover a complete non-empty subset, preserving other requirements for reasoning.

Optional typed_usage: "final" or "evidence" (default "final"). Use "evidence"
only when a complete typed computation is an intermediate input to the remaining
reasoning workflow, and include exactly one complete typed_goal, typed_batch or
typed_graph for a non-empty subset of required_capabilities. Preserve the full
required_capabilities list and all capability_versions for the remaining model
reasoning workflow. Every typed capability must appear in required_capabilities,
and typed capabilities cannot carry a version request. Continue the original
objective after that computation. Preserve source data and constraints; never
invent arguments or treat a checked typed result as proof of broader meaning.
Optional reasoning_capabilities is permitted only with typed_usage="evidence".
It controls Jev+ expert workflow matching only; it never selects a model, grants
permissions or changes the primary Jev-R solver's full objective. Every listed
capability must be in required_capabilities. Together with the typed computation,
the list must cover every requirement. By default it is the remaining capabilities
after typed work; explicitly repeat a typed capability only when additional domain
reasoning under that same capability is still needed. Continue the primary Jev-R
workflow with the full original goal and checked typed evidence. Omit evidence mode
when the typed result alone fulfills the request.

Optional expert_dependency: "typed_results" (default) or "independent" is allowed
only in evidence usage. It applies only to the optional Jev+ model-expert advisory
workflow. "independent" declares those expert inputs do not need typed results,
so that advisory work may overlap typed computation; the final Jev-R solver always
waits for both. This declaration is unverified translation guidance. It selects no
model, provider, permission or tool.

Optional expert_typed_inputs: {"math.evaluate":[{"operation_id":"sum","pointer":"/0/total"}],"sorting.verify":[]}
is allowed only with typed_usage="evidence", a typed_graph, and the default
expert_dependency="typed_results". Keys must be selected reasoning_capabilities;
each value contains up to eight selectors. A legacy string names a whole operation
result; an object selects an exact JSON pointer from that operation's checked
result. Operation IDs must occur in typed_graph. Exact duplicate
(operation_id,pointer) pairs are forbidden. Listed selections gate that capability's
optional Jev+ expert advice until their typed results exist. An empty list means
independent expert advice for that capability; an omitted capability defaults to
depending on all typed inputs. This is unverified translation guidance and source
provenance only; it does not verify semantic relevance, grant authority, or change
permissions. The primary Jev-R/final solver always waits for every typed result and
reasons over the full original interpretation and version constraints.


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
    pub(in crate::agent) client_tool_arguments: Option<Value>,
    pub(in crate::agent) client_tools_independent: bool,
    pub(in crate::agent) typed_goal: Option<Translation>,
    pub(in crate::agent) typed_batch: Option<Vec<(String, Translation)>>,
    pub(in crate::agent) typed_graph: Option<super::graph_translation::Translation>,
    pub(in crate::agent) typed_evidence: bool,
    pub(in crate::agent) reasoning_capabilities: Option<Vec<String>>,
    pub(in crate::agent) independent_experts: bool,
    pub(in crate::agent) expert_typed_inputs:
        Option<std::collections::BTreeMap<String, Vec<super::super::typed_input::Input>>>,
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
        let client_tool_arguments = match object.remove("client_tool_arguments") {
            None | Some(Value::Null) => None,
            Some(value) if value.is_object() => Some(value),
            Some(_) => bail!("Client tool arguments must be an object"),
        };
        let client_tools_independent = match object.remove("client_tools_independent") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => false,
            Some(Value::Bool(true)) => true,
            Some(_) => bail!("Client tool independence must be a boolean"),
        };
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
        let independent_experts = match object.remove("expert_dependency") {
            None => false,
            Some(Value::String(value)) if typed_evidence && value == "typed_results" => false,
            Some(Value::String(value)) if typed_evidence && value == "independent" => true,
            Some(_) => bail!(
                "Expert dependency must be typed_results or independent within evidence usage"
            ),
        };
        let expert_typed_inputs_value = object
            .remove("expert_typed_inputs")
            .filter(|value| !value.is_null());
        if expert_typed_inputs_value.is_some() && independent_experts {
            bail!("Expert typed inputs cannot override independent expert dependency")
        }
        if expert_typed_inputs_value.is_some() && !typed_evidence {
            bail!("Expert typed inputs are allowed only for evidence usage")
        }
        let expert_typed_inputs = expert_typed_inputs_value
            .map(
                serde_json::from_value::<
                    std::collections::BTreeMap<String, Vec<super::super::typed_input::Input>>,
                >,
            )
            .transpose()?;
        if let Some(inputs) = &expert_typed_inputs {
            if inputs.is_empty() || inputs.len() > 16 {
                bail!("Expert typed inputs must contain between one and sixteen capabilities")
            }
            for selectors in inputs.values() {
                if selectors.len() > 8 {
                    bail!("Each expert typed input list must contain up to eight selectors")
                }
                let mut unique = std::collections::HashSet::new();
                for selector in selectors {
                    selector.validate()?;
                    if !unique.insert((selector.operation_id(), selector.pointer())) {
                        bail!("Expert typed input selectors must be unique")
                    }
                }
            }
        }
        let reasoning_capabilities = match object.remove("reasoning_capabilities") {
            None | Some(Value::Null) => None,
            Some(value) if typed_evidence => Some(serde_json::from_value::<Vec<String>>(value)?),
            Some(_) => bail!("Reasoning capability scope is allowed only for evidence usage"),
        };
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
                if !(1..=super::MAX_TYPED_OPERATIONS).contains(&batch.operations.len()) {
                    bail!("Typed batch must contain between one and eight operations")
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
                    || (!typed_evidence && interpretation.required_capabilities.len() != 1)
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
                let capability_scope_matches = if typed_evidence {
                    !batch_capabilities.is_empty()
                        && batch_capabilities.is_subset(&required_capabilities)
                } else {
                    batch_capabilities == required_capabilities
                };
                if !capability_scope_matches {
                    bail!("Typed batch capability scope is incompatible with required capabilities")
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
            .map(|value| super::graph_translation::parse(value, &interpretation, typed_evidence))
            .transpose()?;
        if client_tool_arguments.is_some()
            && (interpretation.clarification.is_some()
                || typed_goal.is_some()
                || typed_batch.is_some()
                || typed_graph.is_some()
                || typed_evidence)
        {
            bail!("Client tool arguments cannot be combined with typed work or clarification")
        }
        if expert_typed_inputs.is_some() && typed_graph.is_none() {
            bail!("Expert typed inputs require a typed graph")
        }
        let typed_capabilities = typed_goal
            .iter()
            .map(|translation| translation.capability.as_str().to_owned())
            .chain(
                typed_batch
                    .iter()
                    .flatten()
                    .map(|(_, translation)| translation.capability.as_str().to_owned()),
            )
            .chain(
                typed_graph
                    .iter()
                    .flat_map(|graph| graph.capabilities().map(str::to_owned)),
            )
            .collect::<std::collections::BTreeSet<_>>();
        let reasoning_capabilities = if typed_evidence {
            let all = interpretation
                .required_capabilities
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            let reasoning = reasoning_capabilities
                .unwrap_or_else(|| all.difference(&typed_capabilities).cloned().collect());
            let reasoning_set = reasoning
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            if reasoning.len() > 16
                || reasoning_set.len() != reasoning.len()
                || !reasoning_set.is_subset(&all)
                || !typed_capabilities.union(&reasoning_set).eq(all.iter())
            {
                bail!("Typed and reasoning capability scopes must uniquely cover the full request")
            }
            Some(reasoning)
        } else {
            None
        };
        if let Some(inputs) = &expert_typed_inputs {
            let reasoning = reasoning_capabilities
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Expert typed inputs require evidence scope"))?
                .iter()
                .map(String::as_str)
                .collect::<std::collections::HashSet<_>>();
            if inputs
                .keys()
                .any(|capability| !reasoning.contains(capability.as_str()))
            {
                bail!("Expert typed input keys must be selected reasoning capabilities")
            }
            let operation_ids = typed_graph
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Expert typed inputs require a typed graph"))?
                .operation_ids()
                .collect::<std::collections::HashSet<_>>();
            if inputs
                .values()
                .flatten()
                .any(|selector| !operation_ids.contains(selector.operation_id()))
            {
                bail!("Expert typed inputs reference an unknown typed graph operation")
            }
        }
        Ok(Self {
            interpretation,
            client_tool_arguments,
            client_tools_independent,
            typed_goal,
            typed_batch,
            typed_graph,
            typed_evidence,
            reasoning_capabilities,
            independent_experts,
            expert_typed_inputs,
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
