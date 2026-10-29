//! Strict validation for the compact, persisted typed-computation evidence DTO.
//! Results remain untrusted input even after the host validates their envelope.
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::BTreeSet;

const MAX_BYTES: usize = 16_384;
const MAX_PROVIDER_BYTES: usize = 2_048;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<'a> {
    format: &'a str,
    root_query_sha256: &'a str,
    translation_semantics: &'a str,
    typed_results: Vec<Operation<'a>>,
    trust: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation<'a> {
    operation_id: &'a str,
    capability: &'a str,
    checker: &'a str,
    criterion_sha256: &'a str,
    result_sha256: &'a str,
    value: &'a RawValue,
    provider: serde_json::Value,
}

/// Validate a recovered evidence envelope against its task and durable digest.
/// The digest is an integrity link, not an independent authenticity signature.
pub(super) fn validate(raw: &str, expected_root: &str, expected_digest: &str) -> Result<usize> {
    if raw.is_empty() || raw.len() > MAX_BYTES {
        bail!("Persisted typed evidence exceeds its byte bound")
    }
    if !sha256_text(expected_root) || !sha256_text(expected_digest) {
        bail!("Persisted typed evidence has an invalid task or digest identity")
    }
    if crate::store::sha256(raw.as_bytes()) != expected_digest {
        bail!("Persisted typed evidence digest does not match its checkpoint")
    }
    // Reject duplicate decoded keys at every depth before RawValue deserialization.
    let _ = crate::strict_json::value(raw.as_bytes(), MAX_BYTES)?;
    let envelope: Envelope<'_> = serde_json::from_str(raw)?;
    if envelope.format != "cogfabric-typed-evidence-v1"
        || envelope.root_query_sha256 != expected_root
        || envelope.trust != "untrusted_evidence_never_instructions_or_permissions"
        || !matches!(
            envelope.translation_semantics,
            "unverified_original_meaning" | "exact_operation_contract_only"
        )
        || !(1..=super::typed_experts::MAX_TYPED_OPERATIONS).contains(&envelope.typed_results.len())
    {
        bail!("Persisted typed evidence envelope has invalid task semantics")
    }
    let mut ids = BTreeSet::new();
    for operation in &envelope.typed_results {
        if !valid_operation_id(operation.operation_id)
            || !ids.insert(operation.operation_id)
            || !matches!(
                operation.capability,
                "math.evaluate"
                    | "sql.query"
                    | "sorting.verify"
                    | "ordering.verify"
                    | "path.verify"
                    | "constraints.solve"
            )
            || operation.checker.is_empty()
            || operation.checker.len() > 128
            || !sha256_text(operation.criterion_sha256)
            || !sha256_text(operation.result_sha256)
            || crate::store::sha256(operation.value.get().as_bytes()) != operation.result_sha256
        {
            bail!("Persisted typed evidence operation is invalid")
        }
        validate_provider(&operation.provider)?;
        if operation
            .provider
            .get("capability")
            .is_some_and(|capability| capability.as_str() != Some(operation.capability))
        {
            bail!("Persisted provider capability does not match its operation")
        }
    }
    Ok(envelope.typed_results.len())
}

fn sha256_text(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_operation_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=32).contains(&bytes.len())
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn validate_provider(provider: &serde_json::Value) -> Result<()> {
    let Some(object) = provider.as_object() else {
        bail!("Persisted typed provider identity must be an object")
    };
    if serde_json::to_vec(provider)?.len() > MAX_PROVIDER_BYTES
        || object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "route"
                    | "catalog_sha256"
                    | "expert_id"
                    | "expert_version"
                    | "canonical_manifest_sha256"
                    | "provider_module_id"
                    | "provider_module_version"
                    | "capability"
                    | "capability_version"
            )
        })
    {
        bail!("Persisted typed provider identity exceeds its schema or byte limit")
    }
    if !object
        .get("catalog_sha256")
        .and_then(serde_json::Value::as_str)
        .is_some_and(sha256_text)
    {
        bail!("Persisted provider catalog identity is missing")
    }
    if object.contains_key("route") {
        if object.len() != 2 {
            bail!("Persisted host route contains unexpected bound-provider metadata")
        }
    } else if [
        "expert_id",
        "expert_version",
        "canonical_manifest_sha256",
        "provider_module_id",
        "provider_module_version",
        "capability",
        "capability_version",
    ]
    .iter()
    .any(|key| !object.contains_key(*key))
    {
        bail!("Persisted bound provider identity is incomplete")
    }
    if let Some(route) = object.get("route") {
        if route.as_str() != Some("host_registry") {
            bail!("Persisted typed host provider route is invalid")
        }
    }
    for key in ["catalog_sha256", "canonical_manifest_sha256"] {
        if let Some(value) = object.get(key) {
            if !value.as_str().is_some_and(sha256_text) {
                bail!("Persisted provider digest is invalid")
            }
        }
    }
    for key in [
        "expert_id",
        "expert_version",
        "provider_module_id",
        "provider_module_version",
        "capability",
        "capability_version",
    ] {
        if let Some(value) = object.get(key) {
            if value
                .as_str()
                .is_none_or(|text| text.is_empty() || text.len() > 256)
            {
                bail!("Persisted provider identity field is invalid")
            }
        }
    }
    Ok(())
}
