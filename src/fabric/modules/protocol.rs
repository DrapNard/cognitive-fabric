//! Validated bounded FlatBuffers messages for process-backed typed modules.
use super::{
    Capability, DependencyNode, FiniteConstraint, FiniteCriterion, FiniteVariable, LinearTerm,
    Relation, Request, Response, SqlTable, SqlValue,
};
use anyhow::{bail, Result};

#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/general_module_generated.rs"));
}
use generated::fabric::general_module as fb;

pub(crate) const MAX_FRAME: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Request,
    Response,
}

#[derive(Clone, Debug)]
pub(crate) struct Envelope {
    pub kind: Kind,
    pub correlation: String,
    pub task: String,
    pub actor: String,
    pub module: String,
    pub version: String,
    pub capability: Capability,
    pub expires: u64,
    pub request: Option<Request>,
    pub response: Option<Response>,
    pub failed: bool,
    pub error: Option<String>,
}

pub(crate) fn encode(envelope: &Envelope) -> Result<Vec<u8>> {
    let mut native = fb::PacketT {
        version: 1,
        kind: match envelope.kind {
            Kind::Request => fb::MessageKind::Request,
            Kind::Response => fb::MessageKind::Response,
        },
        ..Default::default()
    };
    native.correlation_id.clone_from(&envelope.correlation);
    native.task_id.clone_from(&envelope.task);
    native.actor.clone_from(&envelope.actor);
    native.module_id.clone_from(&envelope.module);
    native.module_version.clone_from(&envelope.version);
    native.capability = capability_to_fb(envelope.capability);
    native.expires_at_ms = envelope.expires;
    native.failed = envelope.failed;
    native.error.clone_from(&envelope.error);
    native.request = envelope
        .request
        .as_ref()
        .map(request_to_fb)
        .transpose()?
        .unwrap_or_default();
    native.result = envelope
        .response
        .as_ref()
        .map(response_to_fb)
        .transpose()?
        .unwrap_or_default();
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let root = native.pack(&mut builder);
    fb::finish_packet_buffer(&mut builder, root);
    let bytes = builder.finished_data().to_vec();
    if bytes.len() > MAX_FRAME {
        bail!("General module FlatBuffers frame exceeds the byte limit")
    }
    // Verify our own encoder output and all shape invariants before transport.
    decode(&bytes)?;
    Ok(bytes)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Envelope> {
    if !(8..=MAX_FRAME).contains(&bytes.len()) || !fb::packet_buffer_has_identifier(bytes) {
        bail!("Invalid general module FlatBuffers frame")
    }
    let root = fb::root_as_packet(bytes)?;
    let packet = root.unpack();
    if packet.version != 1
        || uuid::Uuid::parse_str(&packet.task_id).is_err()
        || uuid::Uuid::parse_str(&packet.correlation_id).is_err()
        || packet.actor.len() > 32
        || packet.module_id.is_empty()
        || packet.module_id.len() > 64
        || packet.module_version.is_empty()
        || packet.module_version.len() > 64
        || packet.expires_at_ms == 0
    {
        bail!("General module envelope identity or version is invalid")
    }
    let capability = capability_from_fb(packet.capability)?;
    if packet
        .error
        .as_ref()
        .is_some_and(|error| error.is_empty() || error.len() > 512)
    {
        bail!("General module error exceeds bounds")
    }
    let kind = match packet.kind {
        fb::MessageKind::Request => Kind::Request,
        fb::MessageKind::Response => Kind::Response,
        _ => bail!("Unknown general module message kind"),
    };
    let request = match packet.request {
        fb::RequestValueT::NONE => None,
        value => Some(request_from_fb(value)?),
    };
    let response = match packet.result {
        fb::ResultValueT::NONE => None,
        value => Some(response_from_fb(value)?),
    };
    match kind {
        Kind::Request
            if request.is_none()
                || response.is_some()
                || packet.failed
                || packet.error.is_some() =>
        {
            bail!("Invalid general module request shape")
        }
        Kind::Response
            if request.is_some()
                || (packet.failed && (response.is_some() || packet.error.is_none()))
                || (!packet.failed && (response.is_none() || packet.error.is_some())) =>
        {
            bail!("Invalid general module response shape")
        }
        _ => {}
    }
    if let Some(request) = &request {
        if request.capability() != capability {
            bail!("General module payload does not match its declared capability")
        }
        request.validate_and_size()?;
    }
    Ok(Envelope {
        kind,
        correlation: packet.correlation_id,
        task: packet.task_id,
        actor: packet.actor,
        module: packet.module_id,
        version: packet.module_version,
        capability,
        expires: packet.expires_at_ms,
        request,
        response,
        failed: packet.failed,
        error: packet.error,
    })
}

fn capability_to_fb(value: Capability) -> fb::Capability {
    match value {
        Capability::MathEvaluate => fb::Capability::MathEvaluate,
        Capability::SqlQuery => fb::Capability::SqlQuery,
        Capability::SortingVerify => fb::Capability::SortingVerify,
        Capability::OrderingVerify => fb::Capability::OrderingVerify,
        Capability::PathVerify => fb::Capability::PathVerify,
        Capability::ConstraintsSolve => fb::Capability::ConstraintsSolve,
    }
}

fn capability_from_fb(value: fb::Capability) -> Result<Capability> {
    match value {
        fb::Capability::MathEvaluate => Ok(Capability::MathEvaluate),
        fb::Capability::SqlQuery => Ok(Capability::SqlQuery),
        fb::Capability::SortingVerify => Ok(Capability::SortingVerify),
        fb::Capability::OrderingVerify => Ok(Capability::OrderingVerify),
        fb::Capability::PathVerify => Ok(Capability::PathVerify),
        fb::Capability::ConstraintsSolve => Ok(Capability::ConstraintsSolve),
        _ => bail!("Unknown general module capability"),
    }
}

fn request_to_fb(value: &Request) -> Result<fb::RequestValueT> {
    use fb::{ConstraintT, ConstraintValueT as CV, RequestValueT as RV};
    Ok(match value {
        Request::Arithmetic { expression } => {
            RV::ArithmeticRequest(Box::new(fb::ArithmeticRequestT {
                expression: expression.clone(),
            }))
        }
        Request::Sql { query, tables } => RV::SqlRequest(Box::new(fb::SqlRequestT {
            query: query.clone(),
            tables: tables.iter().map(sql_table_to_fb).collect::<Result<_>>()?,
        })),
        Request::Sort {
            values,
            candidate,
            descending,
        } => RV::SortRequest(Box::new(fb::SortRequestT {
            values: values.clone(),
            candidate: candidate.clone(),
            descending: *descending,
        })),
        Request::DependencyOrder { nodes, candidate } => {
            RV::DependencyOrderRequest(Box::new(fb::DependencyOrderRequestT {
                nodes: nodes
                    .iter()
                    .map(|node| fb::DependencyNodeT {
                        id: node.id.clone(),
                        depends_on: node.depends_on.clone(),
                    })
                    .collect(),
                candidate: candidate.clone(),
            }))
        }
        Request::ShortestPath {
            nodes,
            edges,
            start,
            goal,
            candidate,
        } => RV::ShortestPathRequest(Box::new(fb::ShortestPathRequestT {
            nodes: nodes.clone(),
            edges: edges
                .iter()
                .map(|(first, second)| fb::StringPairT {
                    first: first.clone(),
                    second: second.clone(),
                })
                .collect(),
            start: start.clone(),
            goal: goal.clone(),
            candidate: candidate.clone(),
        })),
        Request::FiniteDomain { criterion } => {
            RV::FiniteDomainRequest(Box::new(fb::FiniteDomainRequestT {
                variables: criterion
                    .variables
                    .iter()
                    .map(|v| fb::FiniteVariableT {
                        id: v.id.clone(),
                        domain: v.domain.clone(),
                    })
                    .collect(),
                constraints: criterion
                    .constraints
                    .iter()
                    .map(|constraint| ConstraintT {
                        value: match constraint {
                            FiniteConstraint::Linear {
                                terms,
                                relation,
                                rhs,
                            } => CV::LinearConstraint(Box::new(fb::LinearConstraintT {
                                terms: terms
                                    .iter()
                                    .map(|t| fb::LinearTermT {
                                        variable: t.variable.clone(),
                                        coefficient: t.coefficient,
                                    })
                                    .collect(),
                                relation: match relation {
                                    Relation::Equal => fb::Relation::Equal,
                                    Relation::NotEqual => fb::Relation::NotEqual,
                                    Relation::LessOrEqual => fb::Relation::LessOrEqual,
                                    Relation::GreaterOrEqual => fb::Relation::GreaterOrEqual,
                                },
                                rhs: *rhs,
                            })),
                            FiniteConstraint::AllDifferent { variables } => {
                                CV::AllDifferentConstraint(Box::new(fb::AllDifferentConstraintT {
                                    variables: variables.clone(),
                                }))
                            }
                        },
                    })
                    .collect(),
            }))
        }
    })
}

fn request_from_fb(value: fb::RequestValueT) -> Result<Request> {
    use fb::{ConstraintValueT as CV, RequestValueT as RV};
    Ok(match value {
        RV::ArithmeticRequest(v) => Request::Arithmetic {
            expression: v.expression,
        },
        RV::SqlRequest(v) => Request::Sql {
            query: v.query,
            tables: v
                .tables
                .into_iter()
                .map(sql_table_from_fb)
                .collect::<Result<_>>()?,
        },
        RV::SortRequest(v) => Request::Sort {
            values: v.values,
            candidate: v.candidate,
            descending: v.descending,
        },
        RV::DependencyOrderRequest(v) => Request::DependencyOrder {
            nodes: v
                .nodes
                .into_iter()
                .map(|n| DependencyNode {
                    id: n.id,
                    depends_on: n.depends_on,
                })
                .collect(),
            candidate: v.candidate,
        },
        RV::ShortestPathRequest(v) => Request::ShortestPath {
            nodes: v.nodes,
            edges: v.edges.into_iter().map(|e| (e.first, e.second)).collect(),
            start: v.start,
            goal: v.goal,
            candidate: v.candidate,
        },
        RV::FiniteDomainRequest(v) => Request::FiniteDomain {
            criterion: FiniteCriterion {
                variables: v
                    .variables
                    .into_iter()
                    .map(|v| FiniteVariable {
                        id: v.id,
                        domain: v.domain,
                    })
                    .collect(),
                constraints: v
                    .constraints
                    .into_iter()
                    .map(|c| match c.value {
                        CV::LinearConstraint(v) => Ok(FiniteConstraint::Linear {
                            terms: v
                                .terms
                                .into_iter()
                                .map(|t| LinearTerm {
                                    variable: t.variable,
                                    coefficient: t.coefficient,
                                })
                                .collect(),
                            relation: match v.relation {
                                fb::Relation::Equal => Relation::Equal,
                                fb::Relation::NotEqual => Relation::NotEqual,
                                fb::Relation::LessOrEqual => Relation::LessOrEqual,
                                fb::Relation::GreaterOrEqual => Relation::GreaterOrEqual,
                                _ => anyhow::bail!("Unknown finite relation"),
                            },
                            rhs: v.rhs,
                        }),
                        CV::AllDifferentConstraint(v) => Ok(FiniteConstraint::AllDifferent {
                            variables: v.variables,
                        }),
                        _ => bail!("Unknown finite constraint variant"),
                    })
                    .collect::<Result<_>>()?,
            },
        },
        _ => bail!("Missing or unknown general module request payload"),
    })
}

fn sql_table_to_fb(table: &SqlTable) -> Result<fb::SqlTableT> {
    Ok(fb::SqlTableT {
        name: table.name.clone(),
        columns: table.columns.clone(),
        rows: table
            .rows
            .iter()
            .map(|row| -> Result<fb::SqlRowT> {
                Ok(fb::SqlRowT {
                    values: row
                        .iter()
                        .map(|value| {
                            Ok(fb::SqlValueT {
                                value: match value {
                                    SqlValue::Null => {
                                        fb::ScalarValueT::NullValue(Box::new(fb::NullValueT {}))
                                    }
                                    SqlValue::Boolean(value) => fb::ScalarValueT::BooleanValue(
                                        Box::new(fb::BooleanValueT { value: *value }),
                                    ),
                                    SqlValue::Integer(value) => fb::ScalarValueT::IntegerValue(
                                        Box::new(fb::IntegerValueT { value: *value }),
                                    ),
                                    SqlValue::Real(value) => {
                                        fb::ScalarValueT::RealValue(Box::new(fb::RealValueT {
                                            value: *value,
                                        }))
                                    }
                                    SqlValue::Text(value) => {
                                        fb::ScalarValueT::TextValue(Box::new(fb::TextValueT {
                                            value: value.clone(),
                                        }))
                                    }
                                },
                            })
                        })
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<_>>()?,
    })
}

fn sql_table_from_fb(table: fb::SqlTableT) -> Result<SqlTable> {
    use fb::ScalarValueT as SV;
    Ok(SqlTable {
        name: table.name,
        columns: table.columns,
        rows: table
            .rows
            .into_iter()
            .map(|row| {
                row.values
                    .into_iter()
                    .map(|value| match value.value {
                        SV::NullValue(_) => Ok(SqlValue::Null),
                        SV::BooleanValue(v) => Ok(SqlValue::Boolean(v.value)),
                        SV::IntegerValue(v) => Ok(SqlValue::Integer(v.value)),
                        SV::RealValue(v) if v.value.is_finite() => Ok(SqlValue::Real(v.value)),
                        SV::RealValue(_) => bail!("Nonfinite SQL real in module frame"),
                        SV::TextValue(v) => Ok(SqlValue::Text(v.value)),
                        _ => bail!("Missing or unknown SQL scalar variant"),
                    })
                    .collect::<Result<_>>()
            })
            .collect::<Result<_>>()?,
    })
}

fn response_to_fb(value: &Response) -> Result<fb::ResultValueT> {
    use fb::ResultValueT as RV;
    Ok(match value {
        Response::Arithmetic { value } => RV::ArithmeticResult(Box::new(fb::ArithmeticResultT {
            value: value.clone(),
        })),
        Response::Sql { columns, rows } => RV::SqlResult(Box::new(fb::SqlResultT {
            columns: columns.clone(),
            rows: rows
                .iter()
                .map(|row| -> Result<fb::SqlRowT> {
                    Ok(fb::SqlRowT {
                        values: row
                            .iter()
                            .map(|v| {
                                Ok(fb::SqlValueT {
                                    value: match v {
                                        SqlValue::Null => {
                                            fb::ScalarValueT::NullValue(Box::new(fb::NullValueT {}))
                                        }
                                        SqlValue::Boolean(value) => fb::ScalarValueT::BooleanValue(
                                            Box::new(fb::BooleanValueT { value: *value }),
                                        ),
                                        SqlValue::Integer(value) => fb::ScalarValueT::IntegerValue(
                                            Box::new(fb::IntegerValueT { value: *value }),
                                        ),
                                        SqlValue::Real(value) => {
                                            fb::ScalarValueT::RealValue(Box::new(fb::RealValueT {
                                                value: *value,
                                            }))
                                        }
                                        SqlValue::Text(value) => {
                                            fb::ScalarValueT::TextValue(Box::new(fb::TextValueT {
                                                value: value.clone(),
                                            }))
                                        }
                                    },
                                })
                            })
                            .collect::<Result<_>>()?,
                    })
                })
                .collect::<Result<_>>()?,
        })),
        Response::SortDiagnostic { claimed_valid } => {
            RV::SortDiagnostic(Box::new(fb::SortDiagnosticT {
                claimed_valid: *claimed_valid,
            }))
        }
        Response::OrderingDiagnostic { claimed_valid } => {
            RV::OrderingDiagnostic(Box::new(fb::OrderingDiagnosticT {
                claimed_valid: *claimed_valid,
            }))
        }
        Response::PathDiagnostic { claimed_valid } => {
            RV::PathDiagnostic(Box::new(fb::PathDiagnosticT {
                claimed_valid: *claimed_valid,
            }))
        }
        Response::FiniteAssignment { values } => {
            RV::FiniteAssignment(Box::new(fb::FiniteAssignmentT {
                values: values
                    .iter()
                    .map(|(variable, value)| fb::AssignmentValueT {
                        variable: variable.clone(),
                        value: *value,
                    })
                    .collect(),
            }))
        }
    })
}

fn response_from_fb(value: fb::ResultValueT) -> Result<Response> {
    use fb::ResultValueT as RV;
    Ok(match value {
        RV::ArithmeticResult(v) => Response::Arithmetic { value: v.value },
        RV::SqlResult(v) => Response::Sql {
            columns: v.columns,
            rows: v
                .rows
                .into_iter()
                .map(|row| {
                    row.values
                        .into_iter()
                        .map(|value| match value.value {
                            fb::ScalarValueT::NullValue(_) => Ok(SqlValue::Null),
                            fb::ScalarValueT::BooleanValue(v) => Ok(SqlValue::Boolean(v.value)),
                            fb::ScalarValueT::IntegerValue(v) => Ok(SqlValue::Integer(v.value)),
                            fb::ScalarValueT::RealValue(v) if v.value.is_finite() => {
                                Ok(SqlValue::Real(v.value))
                            }
                            fb::ScalarValueT::RealValue(_) => bail!("Nonfinite SQL result"),
                            fb::ScalarValueT::TextValue(v) => Ok(SqlValue::Text(v.value)),
                            _ => bail!("Invalid SQL result scalar"),
                        })
                        .collect::<Result<_>>()
                })
                .collect::<Result<_>>()?,
        },
        RV::SortDiagnostic(v) => Response::SortDiagnostic {
            claimed_valid: v.claimed_valid,
        },
        RV::OrderingDiagnostic(v) => Response::OrderingDiagnostic {
            claimed_valid: v.claimed_valid,
        },
        RV::PathDiagnostic(v) => Response::PathDiagnostic {
            claimed_valid: v.claimed_valid,
        },
        RV::FiniteAssignment(v) => Response::FiniteAssignment {
            values: v
                .values
                .into_iter()
                .map(|v| (v.variable, v.value))
                .collect(),
        },
        _ => bail!("Missing or unknown general module result payload"),
    })
}

pub(crate) fn response_envelope(
    mut request: Envelope,
    response: Result<Response>,
) -> Result<Envelope> {
    request.kind = Kind::Response;
    request.request = None;
    match response {
        Ok(response) => {
            request.response = Some(response);
            request.failed = false;
        }
        Err(error) => {
            request.response = None;
            request.failed = true;
            request.error = Some(error.to_string().chars().take(512).collect());
        }
    }
    Ok(request)
}

#[cfg(feature = "wasm-experts")]
pub(crate) fn encode_expert_request(request: &Request) -> Result<Vec<u8>> {
    request.validate_and_size()?;
    let native = fb::ExpertRequestPayloadT {
        version: 1,
        capability: capability_to_fb(request.capability()),
        request: request_to_fb(request)?,
    };
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let root = native.pack(&mut builder);
    builder.finish(root, Some("CGWI"));
    let bytes = builder.finished_data().to_vec();
    if bytes.len() > MAX_FRAME {
        bail!("Wasm expert request exceeds frame bound")
    }
    Ok(bytes)
}

#[cfg(feature = "wasm-experts")]
pub(crate) fn encode_expert_result(response: &Response) -> Result<Vec<u8>> {
    let capability = match response {
        Response::Arithmetic { .. } => Capability::MathEvaluate,
        Response::Sql { .. } => Capability::SqlQuery,
        Response::SortDiagnostic { .. } => Capability::SortingVerify,
        Response::OrderingDiagnostic { .. } => Capability::OrderingVerify,
        Response::PathDiagnostic { .. } => Capability::PathVerify,
        Response::FiniteAssignment { .. } => Capability::ConstraintsSolve,
    };
    let native = fb::ExpertResultPayloadT {
        version: 1,
        capability: capability_to_fb(capability),
        result: response_to_fb(response)?,
    };
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let root = native.pack(&mut builder);
    builder.finish(root, Some("CGWO"));
    let bytes = builder.finished_data().to_vec();
    decode_expert_result(&bytes, capability)?;
    Ok(bytes)
}

#[cfg(feature = "wasm-experts")]
pub(crate) fn decode_expert_result(bytes: &[u8], capability: Capability) -> Result<Response> {
    if !(8..=MAX_FRAME).contains(&bytes.len())
        || !flatbuffers::buffer_has_identifier(bytes, "CGWO", false)
    {
        bail!("Invalid Wasm expert result frame")
    }
    let options = flatbuffers::VerifierOptions {
        max_depth: 16,
        max_tables: 8192,
        max_apparent_size: 4 * 1024 * 1024,
        ..Default::default()
    };
    let root = flatbuffers::root_with_opts::<fb::ExpertResultPayload>(&options, bytes)?;
    if root.version() != 1 || capability_from_fb(root.capability())? != capability {
        bail!("Wasm expert result version or capability differs")
    }
    // Bound vectors and aggregate strings before generated object unpacking,
    // including FlatBuffers that alias the same nested objects repeatedly.
    if let Some(sql) = root.result_as_sql_result() {
        let columns = sql.columns();
        let rows = sql.rows();
        if columns.is_empty() || columns.len() > 16 || rows.len() > 64 {
            bail!("Wasm SQL result count exceeds bounds")
        }
        let mut size = 0usize;
        for name in columns {
            if name.is_empty() || name.len() > 128 {
                bail!("Invalid Wasm SQL column")
            };
            size += name.len();
        }
        for row in rows {
            let values = row.values();
            if values.len() > 16 {
                bail!("Wasm SQL row exceeds bounds")
            }
            for value in values {
                size += if let Some(text) = value.value_as_text_value() {
                    if text.value().len() > 2048 {
                        bail!("Wasm SQL text exceeds bounds")
                    };
                    text.value().len()
                } else {
                    8
                };
                if size > super::MAX_RESULT_BYTES {
                    bail!("Wasm SQL aggregate result exceeds bounds")
                }
            }
        }
    }
    if let Some(result) = root.result_as_arithmetic_result() {
        if result.value().len() > 256 {
            bail!("Wasm arithmetic result exceeds bounds")
        }
    }
    if let Some(result) = root.result_as_finite_assignment() {
        if result.values().len() > 64
            || result
                .values()
                .iter()
                .any(|value| value.variable().len() > 64)
        {
            bail!("Wasm assignment exceeds bounds")
        }
    }
    response_from_fb(root.unpack().result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "wasm-experts")]
    #[test]
    fn expert_decoder_bounds_aliased_sql_before_object_unpacking() {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let text = builder.create_string(&"x".repeat(2048));
        let text = fb::TextValue::create(&mut builder, &fb::TextValueArgs { value: Some(text) });
        let value = fb::SqlValue::create(
            &mut builder,
            &fb::SqlValueArgs {
                value_type: fb::ScalarValue::TextValue,
                value: Some(text.as_union_value()),
            },
        );
        let values = builder.create_vector(&[value; 16]);
        let row = fb::SqlRow::create(
            &mut builder,
            &fb::SqlRowArgs {
                values: Some(values),
            },
        );
        let rows = builder.create_vector(&[row; 64]);
        let name = builder.create_string("column");
        let columns = builder.create_vector(&[name; 16]);
        let result = fb::SqlResult::create(
            &mut builder,
            &fb::SqlResultArgs {
                columns: Some(columns),
                rows: Some(rows),
            },
        );
        let root = fb::ExpertResultPayload::create(
            &mut builder,
            &fb::ExpertResultPayloadArgs {
                version: 1,
                capability: fb::Capability::SqlQuery,
                result_type: fb::ResultValue::SqlResult,
                result: Some(result.as_union_value()),
            },
        );
        builder.finish(root, Some("CGWO"));
        assert!(builder.finished_data().len() < MAX_FRAME);
        let error =
            decode_expert_result(builder.finished_data(), Capability::SqlQuery).unwrap_err();
        assert!(error.to_string().contains("aggregate"), "{error}");
    }

    #[test]
    fn decoder_rejects_unknown_flatbuffer_capability_discriminant() {
        let packet = fb::PacketT {
            version: 1,
            kind: fb::MessageKind::Request,
            correlation_id: uuid::Uuid::new_v4().to_string(),
            task_id: uuid::Uuid::new_v4().to_string(),
            actor: "jev-plus".into(),
            module_id: "fixture.module".into(),
            module_version: "1.0.0".into(),
            capability: fb::Capability(255),
            expires_at_ms: 1,
            request: request_to_fb(&Request::Arithmetic {
                expression: "1+1".into(),
            })
            .unwrap(),
            ..Default::default()
        };
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let root = packet.pack(&mut builder);
        fb::finish_packet_buffer(&mut builder, root);
        assert!(decode(builder.finished_data()).is_err());
    }
}
