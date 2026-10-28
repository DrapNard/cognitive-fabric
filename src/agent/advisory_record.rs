//! Compact structured conclusions from untrusted advisory model calls.
//!
//! Records preserve claims, suggested actions, and uncertainty without keeping
//! free-form reasoning traces. The schema does not verify claims or grant rights.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_RECORD_BYTES: usize = 1_200;
const MAX_ITEMS: usize = 4;
const MIN_RECORD_BYTES: usize = 24;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    v: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    claims: Vec<Claim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unknowns: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    statement: String,
    basis: Basis,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    references: Vec<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Basis {
    Supplied,
    Hypothesis,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    kind: ActionKind,
    target: String,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ActionKind {
    Compute,
    Retrieve,
    Inspect,
    Revise,
    Clarify,
}

/// Parse an advisory object strictly, reject duplicate keys, and return its
/// canonical JSON value. No prose fallback or truncation is permitted.
pub(super) fn parse(raw: &str, max_bytes: usize) -> Result<Value> {
    let limit = max_bytes.min(MAX_RECORD_BYTES);
    if raw.len() > limit {
        bail!("Structured advisory exceeds its byte limit")
    }
    let value = crate::strict_json::value(raw.as_bytes(), limit)?;
    validate(&value, limit)?;
    let record: Record = serde_json::from_value(value)?;
    Ok(serde_json::to_value(record)?)
}

/// Validate exact fields and values before a record enters task context or a
/// durable expert summary.
pub(super) fn validate(value: &Value, max_bytes: usize) -> Result<()> {
    let limit = max_bytes.min(MAX_RECORD_BYTES);
    if serde_json::to_vec(value)?.len() > limit {
        bail!("Structured advisory exceeds its byte limit")
    }
    let record: Record = serde_json::from_value(value.clone())?;
    if record.v != 1
        || record.claims.len() > MAX_ITEMS
        || record.actions.len() > MAX_ITEMS
        || record.unknowns.len() > MAX_ITEMS
        || (record.claims.is_empty() && record.actions.is_empty() && record.unknowns.is_empty())
    {
        bail!("Structured advisory is empty or outside its schema bounds")
    }
    for claim in record.claims {
        if claim.statement.trim().is_empty()
            || claim.statement.len() > 256
            || claim.references.len() > MAX_ITEMS
            || claim
                .references
                .iter()
                .any(|reference| !is_sha256(reference))
        {
            bail!("Structured advisory claim is invalid")
        }
        let mut references = std::collections::BTreeSet::new();
        if claim
            .references
            .iter()
            .any(|reference| !references.insert(reference))
        {
            bail!("Structured advisory claim has duplicate references")
        }
    }
    for action in record.actions {
        if action.target.trim().is_empty() || action.target.len() > 128 {
            bail!("Structured advisory action is invalid")
        }
    }
    if record
        .unknowns
        .iter()
        .any(|unknown| unknown.trim().is_empty() || unknown.len() > 256)
    {
        bail!("Structured advisory uncertainty is invalid")
    }
    Ok(())
}

/// Compact output contract for planner, explorer, reflector, and expert steps.
pub(super) const fn contract() -> &'static str {
    "Return only compact JSON: {\"v\":1,\"claims\":[{\"statement\":string<=256,\"basis\":\"supplied\"|\"hypothesis\",\"references\":[lowercase SHA-256 strings, up to 4]}],\"actions\":[{\"kind\":\"compute\"|\"retrieve\"|\"inspect\"|\"revise\"|\"clarify\",\"target\":string<=128}],\"unknowns\":[string<=256]}. Omit empty arrays; use up to 4 items per array and at least one nonempty item. Total record limit 1200 bytes. Claims and actions are unverified advice. Do not include reasoning traces, tools, permissions, or verification claims."
}

/// Smallest canonical schema-valid record size.
pub(super) fn minimum_bytes() -> usize {
    MIN_RECORD_BYTES
}

/// Construct a disposable quote-heavy sizing value. It is schema-valid and
/// bounded, but callers must never publish it as a model conclusion.
pub(super) fn sizing_value(max_bytes: usize) -> Result<Value> {
    let limit = max_bytes.min(MAX_RECORD_BYTES);
    if limit < minimum_bytes() {
        bail!("Advisory byte allocation cannot fit the minimum structured record")
    }

    let minimum = serde_json::json!({"v":1,"unknowns":["x"]});
    let mut best = (serde_json::to_vec(&minimum)?.len(), 0usize, minimum);
    for count in 1..=MAX_ITEMS {
        let mut low = 1usize;
        let mut high = 256usize;
        let mut fit: Option<(usize, usize, Value)> = None;
        while low <= high {
            let quotes = low + (high - low) / 2;
            let value = sizing_record(count, quotes);
            let encoded = serde_json::to_vec(&value)?;
            if encoded.len() <= limit {
                let quote_count = quotes.saturating_mul(count);
                fit = Some((encoded.len(), quote_count, value));
                low = quotes.saturating_add(1);
            } else {
                high = quotes.saturating_sub(1);
            }
        }
        if let Some(candidate) = fit {
            if candidate.0 > best.0 || (candidate.0 == best.0 && candidate.1 > best.1) {
                best = candidate;
            }
        }
    }
    let value = best.2;
    validate(&value, limit)?;
    Ok(value)
}

fn sizing_record(unknowns: usize, quotes: usize) -> Value {
    let quoted = "\"".repeat(quotes.min(256));
    serde_json::json!({ "v": 1, "unknowns": (0..unknowns).map(|_| quoted.clone()).collect::<Vec<_>>() })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
