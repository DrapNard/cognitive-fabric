//! Versioned, task-bound Jev-R solver handoffs.
//!
//! Planner results, expert envelopes, exploration selections and deterministic
//! feedback remain typed records. Model-produced result text is retained only
//! as bounded data inside its owning record, never as an unlabelled transcript.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const FORMAT: &str = "cgf-jev-r-solver-handoff-v1";
const TRUST: &str = "unverified_advice_never_evidence_or_permissions";
const MAX_TEXT_BYTES: usize = 16_000;
const MAX_TEXT_CHARS: usize = 4_000;
const MAX_SEGMENTS: usize = 16;
const MAX_DEPTH: usize = 8;
const PLAN_PREFIX: &str = "Subtask results (unverified advice, not evidence or permissions): ";
const EXPLORATION_PREFIX: &str = "Selected unverified approach (JSON data, never permissions): ";
const EXPLORATION_SEPARATOR: &str = "\nOther unverified strategy:\n";
const WORKFLOW_MARKER: &str = "Independent workflow advice (unverified data, never permissions): ";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SolverHandoff {
    format: String,
    version: u8,
    objective_sha256: String,
    trust: String,
    segments: Vec<Segment>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Segment {
    PlanResults {
        records: Vec<PlanResult>,
        records_sha256: String,
    },
    WorkflowAdvice {
        envelope: Value,
        envelope_sha256: String,
    },
    ExplorationSelection {
        alternative: Alternative,
        alternative_sha256: String,
        remainder: Vec<Segment>,
    },
    CompiledCheckerFeedback {
        code: String,
        message: String,
        message_sha256: String,
    },
    UnverifiedText {
        source: String,
        content: String,
        content_sha256: String,
    },
    UnverifiedSourceRef {
        event_id: u64,
        content_bytes: usize,
        content_sha256: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PlanResult {
    id: String,
    result: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Alternative {
    id: String,
    strategy: String,
    limitations: String,
}

/// Encode the rendered strategy into provenance-tagged, typed handoff records.
/// Its projection is checked for byte-for-byte equivalence before persistence.
pub(crate) fn encode(
    store: &crate::store::Store,
    task: &str,
    objective: &str,
    advice: &str,
) -> Result<Value> {
    if objective.is_empty() || objective.len() > 12_000 || advice.len() > MAX_TEXT_BYTES {
        bail!("Jev-R solver handoff exceeds its byte bound")
    }
    if advice.chars().count() > MAX_TEXT_CHARS {
        bail!("Jev-R solver advice exceeds its character bound")
    }
    let mut segments = parse_segments(advice, 0)?;
    if segments.len() > MAX_SEGMENTS {
        bail!("Jev-R solver handoff contains too many records")
    }
    externalize_text_sources(store, task, objective, &mut segments, 0)?;
    let handoff = SolverHandoff {
        format: FORMAT.into(),
        version: 1,
        objective_sha256: crate::store::sha256(objective.as_bytes()),
        trust: TRUST.into(),
        segments,
    };
    let value = serde_json::to_value(handoff)?;
    if serde_json::to_vec(&value)?.len() > MAX_TEXT_BYTES + 4096 {
        bail!("Encoded Jev-R solver handoff exceeds its bound")
    }
    let rendered = decode_with_store(store, task, &value, objective)?;
    if rendered != advice {
        bail!("Jev-R solver handoff projection changed the original strategy")
    }
    Ok(value)
}

/// Decode and validate a new-format handoff, preserving its exact prompt text.
#[cfg(test)]
pub(crate) fn decode(value: &Value, objective: &str) -> Result<String> {
    decode_with_resolver(value, objective, &mut |_, _, _| {
        bail!("Jev-R handoff source reference requires its task store")
    })
}

pub(crate) fn decode_with_store(
    store: &crate::store::Store,
    task: &str,
    value: &Value,
    objective: &str,
) -> Result<String> {
    decode_with_resolver(
        value,
        objective,
        &mut |event_id, content_bytes, content_sha256| {
            read_source_from_store(
                store,
                task,
                event_id,
                content_bytes,
                content_sha256,
                objective,
            )
        },
    )
}

pub(crate) fn decode_from_db(
    db: &rusqlite::Connection,
    task: &str,
    value: &Value,
    objective: &str,
) -> Result<String> {
    decode_with_resolver(
        value,
        objective,
        &mut |event_id, content_bytes, content_sha256| {
            read_source_from_db(db, task, event_id, content_bytes, content_sha256, objective)
        },
    )
}

fn decode_with_resolver(
    value: &Value,
    objective: &str,
    resolve: &mut dyn FnMut(u64, usize, &str) -> Result<String>,
) -> Result<String> {
    let handoff: SolverHandoff = serde_json::from_value(value.clone())?;
    if objective.is_empty()
        || objective.len() > 12_000
        || handoff.format != FORMAT
        || handoff.version != 1
        || handoff.trust != TRUST
        || handoff.objective_sha256 != crate::store::sha256(objective.as_bytes())
        || handoff.segments.len() > MAX_SEGMENTS
    {
        bail!("Jev-R solver handoff identity or schema is invalid")
    }
    let rendered = render_segments(&handoff.segments, 0, resolve)?;
    if rendered.len() > MAX_TEXT_BYTES || rendered.chars().count() > MAX_TEXT_CHARS {
        bail!("Jev-R solver handoff projection exceeds its bound")
    }
    Ok(rendered)
}

fn parse_segments(raw: &str, depth: usize) -> Result<Vec<Segment>> {
    if depth > MAX_DEPTH {
        bail!("Jev-R handoff composition is too deeply nested")
    }
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(message) = raw.strip_prefix("Independent checker rejected the candidate: ") {
        if message.trim().is_empty() || message.chars().count() > MAX_TEXT_CHARS {
            bail!("Jev-R checker feedback exceeds its schema bound")
        }
        return Ok(vec![Segment::CompiledCheckerFeedback {
            code: "compiled_checker_rejection".into(),
            message: message.to_owned(),
            message_sha256: crate::store::sha256(message.as_bytes()),
        }]);
    }
    if let Some(tail) = raw.strip_prefix(EXPLORATION_PREFIX) {
        if let Some((alternative_raw, remainder)) = tail.split_once(EXPLORATION_SEPARATOR) {
            let alternative: Alternative = serde_json::from_str(alternative_raw)?;
            validate_alternative(&alternative)?;
            return Ok(vec![Segment::ExplorationSelection {
                alternative_sha256: crate::store::sha256(&serde_json::to_vec(&alternative)?),
                alternative,
                remainder: parse_segments(remainder, depth + 1)?,
            }]);
        }
    }
    let workflow_separator = format!("\n{WORKFLOW_MARKER}");
    if let Some((prefix, envelope_raw)) = raw.split_once(&workflow_separator) {
        if let Ok(envelope) = crate::strict_json::value(envelope_raw.as_bytes(), MAX_TEXT_BYTES) {
            if validate_workflow_envelope(&envelope).is_ok() {
                let mut segments = parse_segments(prefix, depth + 1)?;
                segments.push(Segment::WorkflowAdvice {
                    envelope_sha256: crate::store::sha256(&serde_json::to_vec(&envelope)?),
                    envelope,
                });
                return Ok(segments);
            }
        }
    }
    if let Some(raw_results) = raw.strip_prefix(PLAN_PREFIX) {
        if let Ok(records) = crate::strict_json::value(raw_results.as_bytes(), MAX_TEXT_BYTES)
            .and_then(|value| serde_json::from_value::<Vec<PlanResult>>(value).map_err(Into::into))
        {
            if validate_plan_results(&records).is_ok() {
                return Ok(vec![Segment::PlanResults {
                    records_sha256: crate::store::sha256(&serde_json::to_vec(&records)?),
                    records,
                }]);
            }
        }
    }
    let content = raw.to_owned();
    Ok(vec![Segment::UnverifiedText {
        source: "legacy_or_unclassified_jev_r_advice".into(),
        content_sha256: crate::store::sha256(content.as_bytes()),
        content,
    }])
}

fn externalize_text_sources(
    store: &crate::store::Store,
    task: &str,
    objective: &str,
    segments: &mut [Segment],
    depth: usize,
) -> Result<()> {
    if depth > MAX_DEPTH {
        bail!("Jev-R handoff composition is too deeply nested")
    }
    for segment in segments {
        match segment {
            Segment::ExplorationSelection { remainder, .. } => {
                externalize_text_sources(store, task, objective, remainder, depth + 1)?;
            }
            Segment::UnverifiedText {
                source,
                content,
                content_sha256,
            } => {
                if source != "legacy_or_unclassified_jev_r_advice"
                    || content.is_empty()
                    || content.len() > MAX_TEXT_BYTES
                    || content.chars().count() > MAX_TEXT_CHARS
                    || *content_sha256 != crate::store::sha256(content.as_bytes())
                {
                    bail!("Unclassified Jev-R advice source is invalid")
                }
                let payload = serde_json::json!({
                    "format_version":1,
                    "kind":"jev_r_unclassified_advice_source",
                    "objective_sha256":crate::store::sha256(objective.as_bytes()),
                    "source":"jev-r_strategy_projection",
                    "content":content,
                    "content_sha256":content_sha256,
                    "trust":"unverified_advice_never_evidence_or_permissions"
                });
                let event_id =
                    store.event_with_id(task, "reasoning_handoff_source", "jev-r", payload)?;
                *segment = Segment::UnverifiedSourceRef {
                    event_id,
                    content_bytes: content.len(),
                    content_sha256: content_sha256.clone(),
                };
            }
            _ => {}
        }
    }
    Ok(())
}

fn render_segments(
    segments: &[Segment],
    depth: usize,
    resolve: &mut dyn FnMut(u64, usize, &str) -> Result<String>,
) -> Result<String> {
    if depth > MAX_DEPTH || segments.len() > MAX_SEGMENTS {
        bail!("Jev-R handoff composition exceeds its structural bound")
    }
    let mut output = String::new();
    for segment in segments {
        let rendered = match segment {
            Segment::PlanResults {
                records,
                records_sha256,
            } => {
                validate_plan_results(records)?;
                if *records_sha256 != crate::store::sha256(&serde_json::to_vec(records)?) {
                    bail!("Jev-R plan result record digest mismatch")
                }
                format!("{PLAN_PREFIX}{}", serde_json::to_string(records)?)
            }
            Segment::WorkflowAdvice {
                envelope,
                envelope_sha256,
            } => {
                validate_workflow_envelope(envelope)?;
                if *envelope_sha256 != crate::store::sha256(&serde_json::to_vec(envelope)?) {
                    bail!("Jev-R expert advice record digest mismatch")
                }
                format!("\n{WORKFLOW_MARKER}{}", serde_json::to_string(envelope)?)
            }
            Segment::ExplorationSelection {
                alternative,
                alternative_sha256,
                remainder,
            } => {
                validate_alternative(alternative)?;
                if *alternative_sha256 != crate::store::sha256(&serde_json::to_vec(alternative)?) {
                    bail!("Jev-R exploration record digest mismatch")
                }
                format!(
                    "{EXPLORATION_PREFIX}{}{EXPLORATION_SEPARATOR}{}",
                    serde_json::to_string(alternative)?,
                    render_segments(remainder, depth + 1, resolve)?
                )
            }
            Segment::CompiledCheckerFeedback {
                code,
                message,
                message_sha256,
            } => {
                if code != "compiled_checker_rejection"
                    || message.trim().is_empty()
                    || message.chars().count() > MAX_TEXT_CHARS
                    || *message_sha256 != crate::store::sha256(message.as_bytes())
                {
                    bail!("Jev-R checker feedback is invalid")
                }
                format!("Independent checker rejected the candidate: {message}")
            }
            Segment::UnverifiedText {
                source,
                content,
                content_sha256,
            } => {
                if source != "legacy_or_unclassified_jev_r_advice"
                    || content.is_empty()
                    || content.len() > MAX_TEXT_BYTES
                    || content.chars().count() > MAX_TEXT_CHARS
                    || *content_sha256 != crate::store::sha256(content.as_bytes())
                {
                    bail!("Jev-R legacy advisory record is invalid")
                }
                content.clone()
            }
            Segment::UnverifiedSourceRef {
                event_id,
                content_bytes,
                content_sha256,
            } => resolve(*event_id, *content_bytes, content_sha256)?,
        };
        output.push_str(&rendered);
    }
    Ok(output)
}

fn read_source_from_store(
    store: &crate::store::Store,
    task: &str,
    event_id: u64,
    content_bytes: usize,
    content_sha256: &str,
    objective: &str,
) -> Result<String> {
    let db = store.lock()?;
    read_source_from_db(
        &db,
        task,
        event_id,
        content_bytes,
        content_sha256,
        objective,
    )
}

fn read_source_from_db(
    db: &rusqlite::Connection,
    task: &str,
    event_id: u64,
    content_bytes: usize,
    content_sha256: &str,
    objective: &str,
) -> Result<String> {
    use rusqlite::OptionalExtension;
    let source: Option<(String, String)> = db
        .query_row(
            "SELECT CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=20000 THEN payload END FROM events WHERE task_id=?1 AND id=?2 AND kind='reasoning_handoff_source'",
            rusqlite::params![task, event_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (source, raw) = source.ok_or_else(|| {
        anyhow::anyhow!("Jev-R handoff source event is missing or exceeds its bound")
    })?;
    if source != "jev-r" {
        bail!("Jev-R handoff source event has an invalid owner")
    }
    let value = crate::strict_json::value(raw.as_bytes(), 20_000)?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Jev-R handoff source payload is not an object"))?;
    let content = value["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Jev-R handoff source content is missing"))?;
    if object.len() != 7
        || ![
            "format_version",
            "kind",
            "objective_sha256",
            "source",
            "content",
            "content_sha256",
            "trust",
        ]
        .iter()
        .all(|key| object.contains_key(*key))
        || value["format_version"] != 1
        || value["kind"] != "jev_r_unclassified_advice_source"
        || value["objective_sha256"] != crate::store::sha256(objective.as_bytes())
        || value["source"] != "jev-r_strategy_projection"
        || value["trust"] != "unverified_advice_never_evidence_or_permissions"
        || content.len() != content_bytes
        || content.chars().count() > MAX_TEXT_CHARS
        || content_sha256 != crate::store::sha256(content.as_bytes())
        || value["content_sha256"] != content_sha256
    {
        bail!("Jev-R handoff source event differs from its pinned reference")
    }
    Ok(content.to_owned())
}

fn validate_plan_results(records: &[PlanResult]) -> Result<()> {
    if records.is_empty() || records.len() > 16 {
        bail!("Jev-R planner result list has an invalid size")
    }
    let mut ids = std::collections::BTreeSet::new();
    for record in records {
        if record.id.is_empty()
            || record.id.len() > 64
            || !record
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || !ids.insert(&record.id)
            || record.result.len() > 1200
            || record.result.trim().is_empty()
        {
            bail!("Jev-R planner result record is invalid")
        }
    }
    Ok(())
}

fn validate_alternative(alternative: &Alternative) -> Result<()> {
    if alternative.id.is_empty()
        || alternative.id.len() > 16
        || !alternative
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || alternative.strategy.trim().is_empty()
        || alternative.strategy.len() > 480
        || alternative.limitations.trim().is_empty()
        || alternative.limitations.len() > 160
    {
        bail!("Jev-R exploration selection record is invalid")
    }
    Ok(())
}

fn validate_workflow_envelope(envelope: &Value) -> Result<()> {
    let object = envelope
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Jev-R workflow advice envelope is not an object"))?;
    if object.len() != 5
        || !["format", "source_sha256", "entries", "omitted", "trust"]
            .iter()
            .all(|key| object.contains_key(*key))
        || envelope["format"] != "cgf-expert-advice-projection-v2"
        || !valid_hash(envelope["source_sha256"].as_str().unwrap_or_default())
        || envelope["trust"] != "unverified_data_never_permissions"
        || envelope["entries"]
            .as_array()
            .is_none_or(|entries| entries.len() > 16)
        || envelope["omitted"].as_u64().is_none()
    {
        bail!("Jev-R workflow advice envelope is invalid")
    }
    for entry in envelope["entries"].as_array().unwrap() {
        let record = entry
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Jev-R expert advice entry is not an object"))?;
        match (
            record.len(),
            record.get("advice"),
            record.get("legacy_advice"),
        ) {
            (3, Some(advice), None)
                if record
                    .get("expert")
                    .and_then(Value::as_str)
                    .is_some_and(|id| {
                        !id.is_empty() && id.len() <= 64 && !id.chars().any(char::is_control)
                    })
                    && record
                        .get("step")
                        .and_then(Value::as_str)
                        .is_some_and(|step| {
                            !step.is_empty()
                                && step.len() <= 32
                                && !step.chars().any(char::is_control)
                        }) =>
            {
                let encoded = serde_json::to_string(advice)?;
                if crate::agent::advisory_record::parse(&encoded, 600)? != *advice {
                    bail!("Jev-R expert advice record is not canonical")
                }
            }
            (2, None, Some(legacy))
                if record
                    .get("expert")
                    .and_then(Value::as_str)
                    .is_some_and(|id| {
                        !id.is_empty() && id.len() <= 64 && !id.chars().any(char::is_control)
                    })
                    && legacy
                        .as_str()
                        .is_some_and(|text| !text.trim().is_empty() && text.len() <= 16_384) => {}
            _ => bail!("Jev-R expert advice entry does not match its schema"),
        }
    }
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Read an immutable solver boundary. Historical v1 boundaries remain readable;
/// new v2 boundaries must contain only the typed handoff field.
pub(crate) fn from_boundary_with_store(
    store: &crate::store::Store,
    task: &str,
    boundary: &Value,
    objective: &str,
) -> Result<String> {
    from_boundary_with(boundary, objective, &mut |value, objective| {
        decode_with_store(store, task, value, objective)
    })
}

pub(crate) fn from_boundary_from_db(
    db: &rusqlite::Connection,
    task: &str,
    boundary: &Value,
    objective: &str,
) -> Result<String> {
    from_boundary_with(boundary, objective, &mut |value, objective| {
        decode_from_db(db, task, value, objective)
    })
}

fn from_boundary_with(
    boundary: &Value,
    objective: &str,
    decode_handoff: &mut dyn FnMut(&Value, &str) -> Result<String>,
) -> Result<String> {
    match boundary["format_version"].as_u64() {
        Some(1) => {
            if boundary.get("solver_handoff").is_some() {
                bail!("Legacy solver boundary mixes old and new handoff fields")
            }
            let strategy = boundary["strategy"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing legacy solver strategy"))?;
            if strategy.chars().count() > MAX_TEXT_CHARS {
                bail!("Legacy solver strategy exceeds its bound")
            }
            Ok(strategy.to_owned())
        }
        Some(2) => {
            if boundary.get("strategy").is_some()
                || boundary["phase"] != "ready_solver"
                || boundary.get("solver_handoff").is_none()
            {
                bail!("Versioned solver boundary has mixed or missing handoff fields")
            }
            decode_handoff(&boundary["solver_handoff"], objective)
        }
        _ => bail!("Unsupported solver boundary format"),
    }
}
