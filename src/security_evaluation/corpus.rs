//! Manifest-driven deterministic checks of compiled security boundaries.
//!
//! This runner invokes only fixed Rust policy/parsing/decoding APIs. It never
//! executes model output, scripts, plugins, or corpus-provided code. A corpus
//! hash identifies the exact reviewed input file; review and provenance remain
//! operator-declared attestations rather than independent proof.
use crate::{security, store::sha256, wire};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path};

const MAX_FILE_BYTES: usize = 1_048_576;
const MAX_CASES: usize = 256;
const MAX_CASE_ID_BYTES: usize = 64;
const MAX_SOURCE_BYTES: usize = 1000;
const MAX_LICENSE_BYTES: usize = 128;
const MAX_REVIEWER_BYTES: usize = 128;
const MAX_POLICY_NAME_BYTES: usize = 64;
const MAX_CAPABILITY_BYTES: usize = 128;
const MAX_STRICT_JSON_INPUT_BYTES: usize = 32_768;
const STRICT_JSON_GATE_BYTES: usize = 16_384;
const MAX_WIRE_HEX_BYTES: usize = 512 * 1024;
const MAX_WIRE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    id: String,
    source: String,
    source_sha256: String,
    license: String,
    split: Split,
    review: ReviewAttestation,
    cases: Vec<Case>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Split {
    Development,
    Validation,
    HeldOut,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewAttestation {
    reviewer: String,
    reviewed: bool,
    expected_labels_independent: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    probe: Probe,
    expected_accept: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Probe {
    GlobalPolicy {
        principal: String,
        capability: String,
    },
    TaskPolicy {
        policy: TaskPolicyKind,
        policy_task: String,
        request_task: String,
        actor: String,
        capability: String,
    },
    StrictJson {
        input: String,
    },
    FlatbuffersWire {
        bytes_hex: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum TaskPolicyKind {
    Agent,
    Reasoning,
    ReasoningFiniteSearch,
}

/// Verify and run a reviewed, bounded offline boundary corpus.
///
/// expected_sha256 must be the lowercase SHA-256 of the exact manifest file
/// bytes. Probe families and their acceptance gates are fixed in this module;
/// corpus metadata cannot add code, change a gate, or set an aggregate threshold.
pub fn evaluate_file(path: &Path, expected_sha256: &str) -> Result<Value> {
    security::authorize("human-cli", "evaluation.security")?;
    if !is_lower_sha256(expected_sha256) {
        bail!("Expected corpus hash must be lowercase SHA-256")
    }
    let bytes = security::read_regular_bytes(path, MAX_FILE_BYTES)?;
    let file_sha256 = sha256(&bytes);
    if file_sha256 != expected_sha256 {
        bail!("Security corpus file hash does not match the supplied digest")
    }
    let value = crate::strict_json::value(&bytes, MAX_FILE_BYTES)?;
    let manifest: Manifest = serde_json::from_value(value)?;
    validate_manifest(&manifest)?;

    let canonical_manifest = serde_json::to_vec(&manifest)?;
    let corpus_sha256 = sha256(&canonical_manifest);
    let mut results = Vec::with_capacity(manifest.cases.len());
    for case in &manifest.cases {
        let observed_accept = run_probe(&case.probe);
        results.push(json!({
            "id":case.id,
            "probe_kind":case.probe.kind(),
            "expected_accept":case.expected_accept,
            "observed_accept":observed_accept,
            "passed":case.expected_accept == observed_accept,
        }));
    }

    let result_bytes = serde_json::to_vec(&results)?;
    let total = results.len();
    let passed = results
        .iter()
        .filter(|result| result["passed"] == true)
        .count();
    let expected_accepts = manifest
        .cases
        .iter()
        .filter(|case| case.expected_accept)
        .count();
    let expected_denials = total - expected_accepts;
    let unexpected_accepts = results
        .iter()
        .filter(|result| result["expected_accept"] == false && result["observed_accept"] == true)
        .count();
    let unexpected_denials = results
        .iter()
        .filter(|result| result["expected_accept"] == true && result["observed_accept"] == false)
        .count();

    Ok(json!({
        "format_version":1,
        "suite":"manifest_security_boundaries/v1",
        "manifest_id":manifest.id,
        "split":manifest.split,
        "file_sha256":file_sha256,
        "corpus_sha256":corpus_sha256,
        "provenance":{"source":manifest.source,"source_sha256":manifest.source_sha256,"license":manifest.license,"source_hash_independently_verified":false},
        "review_attestation":{"reviewer":manifest.review.reviewer,"reviewed":manifest.review.reviewed,"expected_labels_independent":manifest.review.expected_labels_independent,"independently_verified":false},
        "total":total,
        "passed":passed,
        "success":passed == total,
        "expected_accepts":expected_accepts,
        "expected_denials":expected_denials,
        "unexpected_accepts":unexpected_accepts,
        "unexpected_denials":unexpected_denials,
        "unexpected_accept_rate":if expected_denials == 0 { Value::Null } else { json!(unexpected_accepts as f64 / expected_denials as f64) },
        "results_sha256":sha256(&result_bytes),
        "results":results,
        "claims":{"deterministic_rust_probes_only":true,"arbitrary_code_executed":false,"model_executed":false,"neural_safety_evaluated":false,"sandbox_safety_evaluated":false},
        "unmeasured":["live_prompt_injection_success","model_poisoning","sandbox_escape","credential_exfiltration","general_security_failure_rate","module_peer_authentication"]
    }))
}

impl Probe {
    fn kind(&self) -> &'static str {
        match self {
            Self::GlobalPolicy { .. } => "global_policy",
            Self::TaskPolicy { .. } => "task_policy",
            Self::StrictJson { .. } => "strict_json",
            Self::FlatbuffersWire { .. } => "flatbuffers_wire",
        }
    }
}

fn run_probe(probe: &Probe) -> bool {
    match probe {
        Probe::GlobalPolicy {
            principal,
            capability,
        } => security::authorize(principal, capability).is_ok(),
        Probe::TaskPolicy {
            policy,
            policy_task,
            request_task,
            actor,
            capability,
        } => {
            let scoped = match policy {
                TaskPolicyKind::Agent => security::TaskPolicy::agent(policy_task),
                TaskPolicyKind::Reasoning => security::TaskPolicy::reasoning(policy_task),
                TaskPolicyKind::ReasoningFiniteSearch => {
                    security::TaskPolicy::reasoning_with_finite_search(policy_task)
                }
            };
            scoped.is_ok_and(|policy| policy.authorize(request_task, actor, capability).is_ok())
        }
        Probe::StrictJson { input } => {
            crate::strict_json::value(input.as_bytes(), STRICT_JSON_GATE_BYTES).is_ok()
        }
        Probe::FlatbuffersWire { bytes_hex } => {
            decode_hex(bytes_hex).is_ok_and(|bytes| wire::decode(&bytes).is_ok())
        }
    }
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.format_version != 1
        || manifest.id.len() > MAX_CASE_ID_BYTES
        || !crate::registry::identifier(&manifest.id)
        || manifest.cases.is_empty()
        || manifest.cases.len() > MAX_CASES
    {
        bail!("Invalid security corpus identity, version, or case count")
    }
    security::validate_provenance(&manifest.source, &manifest.license)?;
    if manifest.source.len() > MAX_SOURCE_BYTES
        || manifest.license.len() > MAX_LICENSE_BYTES
        || !is_lower_sha256(&manifest.source_sha256)
    {
        bail!("Security corpus provenance exceeds bounds or lacks a source hash")
    }
    if !manifest.review.reviewed
        || !manifest.review.expected_labels_independent
        || manifest.review.reviewer.trim().is_empty()
        || manifest.review.reviewer.len() > MAX_REVIEWER_BYTES
        || manifest.review.reviewer.chars().any(char::is_control)
    {
        bail!("Security corpus requires an explicit operator review attestation")
    }

    let mut ids = BTreeSet::new();
    for case in &manifest.cases {
        if case.id.len() > MAX_CASE_ID_BYTES
            || !crate::registry::identifier(&case.id)
            || !ids.insert(case.id.as_str())
        {
            bail!("Security corpus contains an invalid or duplicate case identity")
        }
        match &case.probe {
            Probe::GlobalPolicy {
                principal,
                capability,
            } => {
                validate_label(principal, MAX_POLICY_NAME_BYTES)?;
                validate_label(capability, MAX_CAPABILITY_BYTES)?;
            }
            Probe::TaskPolicy {
                policy_task,
                request_task,
                actor,
                capability,
                ..
            } => {
                if policy_task.len() > 36 || request_task.len() > 36 {
                    bail!("Task policy probe identity exceeds bounds")
                }
                validate_label(actor, MAX_POLICY_NAME_BYTES)?;
                validate_label(capability, MAX_CAPABILITY_BYTES)?;
            }
            Probe::StrictJson { input } => {
                if input.len() > MAX_STRICT_JSON_INPUT_BYTES {
                    bail!("Strict JSON probe exceeds its input bound")
                }
            }
            Probe::FlatbuffersWire { bytes_hex } => {
                if bytes_hex.len() > MAX_WIRE_HEX_BYTES
                    || bytes_hex.len() % 2 != 0
                    || !bytes_hex.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    bail!("FlatBuffers probe is not bounded hexadecimal data")
                }
                if bytes_hex.len() / 2 > MAX_WIRE_BYTES {
                    bail!("FlatBuffers probe exceeds the fixed frame limit")
                }
            }
        }
    }
    // Serde's representation is canonical for this closed struct/enum schema;
    // this also bounds the expanded parsed form, including escaped strings.
    if serde_json::to_vec(manifest)?.len() > MAX_FILE_BYTES {
        bail!("Canonical security corpus exceeds its byte bound")
    }
    Ok(())
}

fn validate_label(value: &str, max_bytes: usize) -> Result<()> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        bail!("Security probe label is empty or exceeds bounds")
    }
    Ok(())
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 || value.len() / 2 > MAX_WIRE_BYTES {
        bail!("Invalid FlatBuffers probe length")
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char)
                .to_digit(16)
                .ok_or_else(|| anyhow!("Invalid FlatBuffers probe encoding"))?;
            let low = (pair[1] as char)
                .to_digit(16)
                .ok_or_else(|| anyhow!("Invalid FlatBuffers probe encoding"))?;
            Ok(((high << 4) | low) as u8)
        })
        .collect::<Result<Vec<_>>>()
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
