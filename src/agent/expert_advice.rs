//! Bounded, task-bound persistence for completed expert advisory summaries.
//!
//! This record stores advisory content only. Its trust label is descriptive and
//! never grants permissions or verifies the original goal's semantics.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

const MAX_SUMMARY_BYTES: usize = 1_200;
const MAX_ENVELOPE_BYTES: usize = 2_048;
const TRUST: &str = "untrusted_model_advisory_never_permissions";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    version: u8,
    root_sha256: String,
    references_sha256: String,
    summary_sha256: String,
    trust: String,
    summary: Value,
}

impl Record {
    pub(super) fn new(raw_summary: &str, root_sha: &str, references_sha: &str) -> Result<Self> {
        if raw_summary.is_empty() || raw_summary.len() > MAX_SUMMARY_BYTES {
            bail!("Expert advice summary exceeds its byte bound")
        }
        let summary = crate::strict_json::value(raw_summary.as_bytes(), MAX_SUMMARY_BYTES)?;
        validate_summary(&summary)?;
        let record = Self {
            version: 1,
            root_sha256: root_sha.to_owned(),
            references_sha256: references_sha.to_owned(),
            summary_sha256: crate::store::sha256(summary.to_string().as_bytes()),
            trust: TRUST.to_owned(),
            summary,
        };
        record.validate(root_sha, references_sha)?;
        Ok(record)
    }

    pub(super) fn validate(&self, expected_root: &str, expected_refs: &str) -> Result<()> {
        self.validate_checkpoint(expected_refs)?;
        if self.root_sha256 != expected_root {
            bail!("Expert advice record belongs to a different root query")
        }
        Ok(())
    }

    /// Validate the durable record where the original request is no longer present.
    pub(super) fn validate_checkpoint(&self, expected_refs: &str) -> Result<()> {
        if self.version != 1
            || self.trust != TRUST
            || !is_sha256(&self.root_sha256)
            || !is_sha256(&self.references_sha256)
            || !is_sha256(&self.summary_sha256)
        {
            bail!("Invalid expert advice record metadata")
        }
        if self.references_sha256 != expected_refs {
            bail!("Expert advice record belongs to a different reference snapshot")
        }
        validate_summary(&self.summary)?;
        let canonical = self.summary.to_string();
        if canonical.len() > MAX_SUMMARY_BYTES
            || crate::store::sha256(canonical.as_bytes()) != self.summary_sha256
        {
            bail!("Expert advice summary digest or size is invalid")
        }
        Ok(())
    }

    /// Return a bounded JSON context envelope for downstream reasoning.
    pub(super) fn context(&self) -> Result<String> {
        self.validate_checkpoint(&self.references_sha256)?;
        let envelope = serde_json::json!({
            "format": "cogfabric-expert-advice-v1",
            "version": self.version,
            "root_sha256": self.root_sha256,
            "references_sha256": self.references_sha256,
            "summary_sha256": self.summary_sha256,
            "trust": self.trust,
            "original_goal_semantics_verified": false,
            "summary": self.summary,
        });
        let encoded = serde_json::to_string(&envelope)?;
        if encoded.len() > MAX_ENVELOPE_BYTES {
            bail!("Expert advice context envelope exceeds its byte bound")
        }
        Ok(encoded)
    }

    pub(super) fn digest(&self) -> &str {
        &self.summary_sha256
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_summary(summary: &Value) -> Result<()> {
    let object = summary
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Expert advice summary must be an object"))?;
    match object.get("experts") {
        Some(experts) => {
            exact_keys(object, &["experts", "trust"])?;
            require_trust(object)?;
            let experts = experts
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Expert summaries must be an array"))?;
            if experts.is_empty() || experts.len() > 3 {
                bail!("Expert summary count is outside its bound")
            }
            let mut ids = BTreeSet::new();
            for expert in experts {
                let expert = expert
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("Expert summary entry must be an object"))?;
                let allowed = [
                    "capability",
                    "expert_id",
                    "version",
                    "results",
                    "dependencies",
                ];
                if expert.keys().any(|key| !allowed.contains(&key.as_str())) {
                    bail!("Expert summary entry contains an unknown field")
                }
                if !["capability", "expert_id", "version", "results"]
                    .iter()
                    .all(|key| expert.contains_key(*key))
                {
                    bail!("Expert summary entry is incomplete")
                }
                let capability = string_field(expert, "capability")?;
                if !capability.is_empty() && !crate::registry::identifier(capability) {
                    bail!("Invalid expert capability identifier")
                }
                validate_identity(expert, &mut ids)?;
                validate_results(expert.get("results").expect("required key checked"))?;
                if let Some(dependencies) = expert.get("dependencies") {
                    let dependencies = dependencies
                        .as_array()
                        .ok_or_else(|| anyhow::anyhow!("Expert dependencies must be an array"))?;
                    if dependencies.len() > 16 {
                        bail!("Expert dependency list exceeds its bound")
                    }
                    let mut dependency_ids = BTreeSet::new();
                    for dependency in dependencies {
                        let id = dependency
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("Expert dependency must be an id"))?;
                        if !crate::registry::identifier(id) || !dependency_ids.insert(id) {
                            bail!("Invalid or duplicate expert dependency id")
                        }
                    }
                }
            }
        }
        None => {
            exact_keys(object, &["expert_id", "version", "results", "trust"])?;
            require_trust(object)?;
            let mut ids = BTreeSet::new();
            validate_identity(object, &mut ids)?;
            validate_results(object.get("results").expect("required key checked"))?;
        }
    }
    if summary.to_string().len() > MAX_SUMMARY_BYTES {
        bail!("Expert advice summary exceeds its canonical byte bound")
    }
    Ok(())
}

fn exact_keys(object: &Map<String, Value>, expected: &[&str]) -> Result<()> {
    if object.len() != expected.len() || object.keys().any(|key| !expected.contains(&key.as_str()))
    {
        bail!("Expert advice summary contains missing or unknown fields")
    }
    Ok(())
}

fn require_trust(object: &Map<String, Value>) -> Result<()> {
    if string_field(object, "trust")? != TRUST {
        bail!("Expert advice summary has an invalid trust label")
    }
    Ok(())
}

fn validate_identity(object: &Map<String, Value>, ids: &mut BTreeSet<String>) -> Result<()> {
    let id = string_field(object, "expert_id")?;
    if !crate::registry::identifier(id) || !ids.insert(id.to_owned()) {
        bail!("Invalid or duplicate expert id")
    }
    let version = string_field(object, "version")?;
    semver::Version::parse(version)?;
    Ok(())
}

fn validate_results(results: &Value) -> Result<()> {
    let results = results
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Expert results must be an object"))?;
    if results.is_empty() || results.len() > 4 {
        bail!("Expert result count is outside its bound")
    }
    for (step_id, result) in results {
        if !crate::registry::identifier(step_id) || step_id.len() > 32 {
            bail!("Invalid expert step identifier")
        }
        let result = result
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Expert result must be an object"))?;
        exact_keys(result, &["advice", "partial", "trust"])?;
        let advice = string_field(result, "advice")?;
        if advice.len() > MAX_SUMMARY_BYTES
            || result.get("partial").and_then(Value::as_bool).is_none()
            || string_field(result, "trust")? != TRUST
        {
            bail!("Invalid bounded expert result")
        }
    }
    Ok(())
}

fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Expert advice field {key} must be a string"))
}
