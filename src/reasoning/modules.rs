//! General module dispatch retains the original Session policy and tool ledger.
use super::*;
use crate::fabric::modules::{Broker, Grant, Pin, Request, Response, SqlTable, SqlValue};

impl Session {
    pub(crate) async fn dispatch_typed_module(
        &mut self,
        broker: &Broker,
        module: &Pin,
        request: Request,
    ) -> Result<Response> {
        let capability = request.capability();
        let actor = capability.actor();
        self.authorize_tool_as(actor, capability.as_str())?;
        self.dispatch_typed_module_pre_authorized(broker, module, request)
            .await
    }

    pub(crate) async fn dispatch_typed_module_pre_authorized(
        &mut self,
        broker: &Broker,
        module: &Pin,
        request: Request,
    ) -> Result<Response> {
        let capability = request.capability();
        let actor = capability.actor();
        let grant = Grant::narrow(&self.task, actor, [capability], 1, &self.policy)?;
        let deadline = self
            .deadline
            .min(Instant::now() + Duration::from_secs(self.cfg.timeout_seconds.min(300)));
        broker
            .dispatch(module, &grant, &self.policy, actor, request, deadline)
            .await
    }
}

/// Build the exact typed verifier request from the independently validated
/// structured goal and a bounded model candidate. The module's response stays
/// advisory; `Criterion::check` remains the acceptance authority.
pub(crate) fn candidate_verification_request(query: &str, answer: &str) -> Result<Option<Request>> {
    use serde::Deserialize;

    if answer.len() > 8192 {
        return Ok(None);
    }
    if let Some(raw) = query.strip_prefix("sort:") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SortGoal {
            values: Vec<i64>,
            #[serde(default)]
            descending: bool,
        }
        let goal: SortGoal =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 10_000)?)?;
        let Some(candidate) = parse_candidate::<Vec<i64>>(answer) else {
            return Ok(None);
        };
        return Ok(Some(Request::Sort {
            values: goal.values,
            candidate,
            descending: goal.descending,
        }));
    }
    if let Some(raw) = query.strip_prefix("order:") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct OrderGoal {
            nodes: Vec<String>,
            before: Vec<[String; 2]>,
        }
        let goal: OrderGoal =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 10_000)?)?;
        let Some(candidate) = parse_candidate::<Vec<String>>(answer) else {
            return Ok(None);
        };
        let nodes = goal
            .nodes
            .into_iter()
            .map(|id| crate::fabric::modules::DependencyNode {
                id: id.clone(),
                depends_on: goal
                    .before
                    .iter()
                    .filter(|edge| edge[1] == id)
                    .map(|edge| edge[0].clone())
                    .collect(),
            })
            .collect();
        return Ok(Some(Request::DependencyOrder { nodes, candidate }));
    }
    if let Some(raw) = query.strip_prefix("path:") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PathGoal {
            nodes: Vec<String>,
            edges: Vec<[String; 2]>,
            start: String,
            goal: String,
        }
        let goal: PathGoal =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 10_000)?)?;
        let Some(candidate) = parse_candidate::<Vec<String>>(answer) else {
            return Ok(None);
        };
        return Ok(Some(Request::ShortestPath {
            nodes: goal.nodes,
            edges: goal
                .edges
                .into_iter()
                .map(|edge| (edge[0].clone(), edge[1].clone()))
                .collect(),
            start: goal.start,
            goal: goal.goal,
            candidate,
        }));
    }
    Ok(None)
}

// Malformed model output is a rejected candidate, not a runtime/parser failure.
fn parse_candidate<T: serde::de::DeserializeOwned>(answer: &str) -> Option<T> {
    crate::strict_json::value(answer.as_bytes(), 8192)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
}

pub(crate) fn finite_domain_request(query: &str) -> Result<Option<Request>> {
    use serde::Deserialize;
    let Some(raw) = query.strip_prefix("csp:") else {
        return Ok(None);
    };
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Goal {
        variables: Vec<Variable>,
        constraints: Vec<Constraint>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Variable {
        id: String,
        domain: Vec<i64>,
    }
    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum Constraint {
        Linear {
            terms: Vec<Term>,
            relation: Relation,
            rhs: i64,
        },
        AllDifferent {
            variables: Vec<String>,
        },
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Term {
        variable: String,
        coefficient: i64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Relation {
        Eq,
        Ne,
        Le,
        Ge,
    }
    let goal: Goal = serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 10_000)?)?;
    let variables = goal
        .variables
        .into_iter()
        .map(|variable| crate::fabric::modules::FiniteVariable {
            id: variable.id,
            domain: variable.domain,
        })
        .collect();
    let constraints = goal
        .constraints
        .into_iter()
        .map(|constraint| match constraint {
            Constraint::Linear {
                terms,
                relation,
                rhs,
            } => crate::fabric::modules::FiniteConstraint::Linear {
                terms: terms
                    .into_iter()
                    .map(|term| crate::fabric::modules::LinearTerm {
                        variable: term.variable,
                        coefficient: term.coefficient,
                    })
                    .collect(),
                relation: match relation {
                    Relation::Eq => crate::fabric::modules::Relation::Equal,
                    Relation::Ne => crate::fabric::modules::Relation::NotEqual,
                    Relation::Le => crate::fabric::modules::Relation::LessOrEqual,
                    Relation::Ge => crate::fabric::modules::Relation::GreaterOrEqual,
                },
                rhs,
            },
            Constraint::AllDifferent { variables } => {
                crate::fabric::modules::FiniteConstraint::AllDifferent { variables }
            }
        })
        .collect();
    Ok(Some(Request::FiniteDomain {
        criterion: crate::fabric::modules::FiniteCriterion {
            variables,
            constraints,
        },
    }))
}

pub(crate) fn sql_request(raw: &str) -> Result<Request> {
    crate::strict_json::value(raw.as_bytes(), 32000)?;
    let task = crate::sql::parse_task(raw)?;
    let tables = task
        .tables
        .into_iter()
        .map(|table| {
            let rows = table
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(sql_cell).collect::<Result<Vec<_>>>())
                .collect::<Result<Vec<_>>>()?;
            Ok(SqlTable {
                name: table.name,
                columns: table.columns,
                rows,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Request::Sql {
        query: task.query,
        tables,
    })
}
fn sql_cell(value: serde_json::Value) -> Result<SqlValue> {
    Ok(match value {
        serde_json::Value::Null => SqlValue::Null,
        serde_json::Value::Bool(v) => SqlValue::Boolean(v),
        serde_json::Value::Number(v) if v.is_i64() => SqlValue::Integer(v.as_i64().unwrap()),
        serde_json::Value::Number(v) if v.is_f64() => SqlValue::Real(
            v.as_f64()
                .ok_or_else(|| anyhow::anyhow!("Invalid SQL real"))?,
        ),
        serde_json::Value::String(v) => SqlValue::Text(v),
        _ => bail!("Unsupported inline SQL cell"),
    })
}
pub(crate) fn sql_response(response: Response) -> Result<String> {
    let Response::Sql { columns, rows } = response else {
        bail!("Typed SQL module returned another response kind")
    };
    let rows = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| match cell {
                    SqlValue::Null => serde_json::Value::Null,
                    SqlValue::Boolean(v) => json!(v),
                    SqlValue::Integer(v) => json!(v),
                    SqlValue::Real(v) => json!(v),
                    SqlValue::Text(v) => json!(v),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    Ok(json!({"columns":columns,"rows":rows}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_advisory_candidates_do_not_abort_compiled_verification() {
        for query in [
            r#"sort:{"values":[3,1,2]}"#,
            r#"order:{"nodes":["a","b"],"before":[["a","b"]]}"#,
            r#"path:{"nodes":["a","b"],"edges":[["a","b"]],"start":"a","goal":"b"}"#,
        ] {
            for candidate in ["not JSON", "[", r#"{"duplicate":1,"duplicate":2}"#, "null"] {
                assert!(candidate_verification_request(query, candidate)
                    .unwrap()
                    .is_none());
                assert!(crate::verification::Criterion::from_query(query)
                    .unwrap()
                    .unwrap()
                    .check(candidate)
                    .is_err());
            }
        }
    }
}
