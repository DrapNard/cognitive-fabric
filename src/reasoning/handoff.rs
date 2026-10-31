//! Versioned, task-bound Jev-R solver handoffs.
//!
//! Strategy advice is data, not evidence or authority. The schema preserves
//! exact model-facing advice while distinguishing structured deterministic
//! feedback from unverified text and binding the record to its objective.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

const FORMAT: &str = "cgf-jev-r-solver-handoff-v1";
const TRUST: &str = "unverified_advice_never_evidence_or_permissions";
const MAX_TEXT_BYTES: usize = 16_000;
const MAX_TEXT_CHARS: usize = 4_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SolverHandoff {
    format: String,
    version: u8,
    objective_sha256: String,
    trust: String,
    advice: Advice,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Advice {
    Empty,
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
}

/// Encode a new-format handoff without changing the text used by the solver.
pub(crate) fn encode(objective: &str, advice: &str) -> Result<serde_json::Value> {
    if objective.is_empty() || objective.len() > 12_000 || advice.len() > MAX_TEXT_BYTES {
        bail!("Jev-R solver handoff exceeds its byte bound")
    }
    let advice = if advice.is_empty() {
        Advice::Empty
    } else if let Some(message) =
        advice.strip_prefix("Independent checker rejected the candidate: ")
    {
        if message.trim().is_empty() || message.chars().count() > MAX_TEXT_CHARS {
            bail!("Jev-R checker feedback exceeds its schema bound")
        }
        Advice::CompiledCheckerFeedback {
            code: "compiled_checker_rejection".into(),
            message: message.to_owned(),
            message_sha256: crate::store::sha256(message.as_bytes()),
        }
    } else {
        if advice.chars().count() > MAX_TEXT_CHARS {
            bail!("Jev-R advisory text exceeds its schema bound")
        }
        Advice::UnverifiedText {
            source: "jev-r_strategy_projection".into(),
            content: advice.to_owned(),
            content_sha256: crate::store::sha256(advice.as_bytes()),
        }
    };
    let handoff = SolverHandoff {
        format: FORMAT.into(),
        version: 1,
        objective_sha256: crate::store::sha256(objective.as_bytes()),
        trust: TRUST.into(),
        advice,
    };
    let value = serde_json::to_value(handoff)?;
    if serde_json::to_vec(&value)?.len() > MAX_TEXT_BYTES + 1024 {
        bail!("Encoded Jev-R solver handoff exceeds its bound")
    }
    Ok(value)
}

/// Decode and validate a new-format handoff, preserving its exact prompt text.
pub(crate) fn decode(value: &serde_json::Value, objective: &str) -> Result<String> {
    let handoff: SolverHandoff = serde_json::from_value(value.clone())?;
    if objective.is_empty()
        || objective.len() > 12_000
        || handoff.format != FORMAT
        || handoff.version != 1
        || handoff.trust != TRUST
        || handoff.objective_sha256 != crate::store::sha256(objective.as_bytes())
    {
        bail!("Jev-R solver handoff identity or schema is invalid")
    }
    match handoff.advice {
        Advice::Empty => Ok(String::new()),
        Advice::CompiledCheckerFeedback {
            code,
            message,
            message_sha256,
        } => {
            if code != "compiled_checker_rejection"
                || message.trim().is_empty()
                || message.chars().count() > MAX_TEXT_CHARS
                || message_sha256 != crate::store::sha256(message.as_bytes())
            {
                bail!("Jev-R checker feedback is invalid")
            }
            Ok(format!(
                "Independent checker rejected the candidate: {message}"
            ))
        }
        Advice::UnverifiedText {
            source,
            content,
            content_sha256,
        } => {
            if source != "jev-r_strategy_projection"
                || content.is_empty()
                || content.len() > MAX_TEXT_BYTES
                || content.chars().count() > MAX_TEXT_CHARS
                || content_sha256 != crate::store::sha256(content.as_bytes())
            {
                bail!("Jev-R unverified strategy data is invalid")
            }
            Ok(content)
        }
    }
}

/// Read an immutable solver boundary. Historical v1 boundaries remain readable;
/// new v2 boundaries must contain only the typed handoff field.
pub(crate) fn from_boundary(boundary: &serde_json::Value, objective: &str) -> Result<String> {
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
            decode(&boundary["solver_handoff"], objective)
        }
        _ => bail!("Unsupported solver boundary format"),
    }
}
