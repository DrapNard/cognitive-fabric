//! Bounded, task-bound persistence for completed expert advisory summaries.
//!
//! This record stores advisory content only. Its trust label is descriptive and
//! never grants permissions or verifies the original goal's semantics.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

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
        validate_summary(&summary, 4)?;
        let record = Self {
            version: 4,
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
        if !matches!(self.version, 1..=4)
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
        validate_summary(&self.summary, self.version)?;
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
            "format": format!("cogfabric-expert-advice-v{}", self.version),
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

    /// Validate referenced core operation results against the task's durable
    /// core memory. A record without references does not need core memory.
    pub(super) fn validate_core_references(
        &self,
        memory: Option<&super::core_tools::Memory>,
    ) -> Result<()> {
        self.validate_checkpoint(&self.references_sha256)?;
        let references = core_references(&self.summary, self.version)?;
        if references.is_empty() {
            return Ok(());
        }
        let memory = memory.ok_or_else(|| {
            anyhow::anyhow!("Expert advice core references require task core memory")
        })?;
        memory.validate_digest(&self.root_sha256)?;
        for (operation_id, expected_digest) in references {
            if memory.result_sha256(&operation_id) != Some(expected_digest.as_str()) {
                bail!("Expert advice core result reference does not match task memory")
            }
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_summary(summary: &Value, version: u8) -> Result<()> {
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
            let mut core_ids = BTreeMap::new();
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
                validate_results(
                    expert.get("results").expect("required key checked"),
                    version,
                    &mut core_ids,
                )?;
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
            validate_results(
                object.get("results").expect("required key checked"),
                version,
                &mut BTreeMap::new(),
            )?;
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

fn validate_results(
    results: &Value,
    version: u8,
    core_ids: &mut BTreeMap<String, String>,
) -> Result<()> {
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
        if version >= 3 {
            if result.keys().any(|key| {
                !["advice", "partial", "trust", "core_operations"].contains(&key.as_str())
            }) || !["advice", "partial", "trust"]
                .iter()
                .all(|key| result.contains_key(*key))
            {
                bail!("Expert advice result contains missing or unknown fields")
            }
        } else {
            exact_keys(result, &["advice", "partial", "trust"])?;
        }
        if string_field(result, "trust")? != TRUST {
            bail!("Invalid bounded expert result")
        }
        let partial = result
            .get("partial")
            .and_then(Value::as_bool)
            .ok_or_else(|| anyhow::anyhow!("Expert partial marker must be boolean"))?;
        let advice = result
            .get("advice")
            .ok_or_else(|| anyhow::anyhow!("Expert advice result is missing"))?;
        match version {
            1 => {
                if advice
                    .as_str()
                    .is_none_or(|text| text.len() > MAX_SUMMARY_BYTES)
                {
                    bail!("Invalid legacy expert advice text")
                }
            }
            2..=4 => {
                if partial {
                    bail!("Structured expert memory cannot contain a partial result")
                }
                crate::agent::advisory_record::validate(advice, MAX_SUMMARY_BYTES)?;
                if version >= 3 {
                    if let Some(operations) = result.get("core_operations") {
                        let operations = operations.as_array().ok_or_else(|| {
                            anyhow::anyhow!("Core operation references must be an array")
                        })?;
                        if operations.is_empty() || operations.len() > 4 {
                            bail!("Core operation reference count is outside its bound")
                        }
                        let mut step_ids = BTreeSet::new();
                        for operation in operations {
                            let operation = operation.as_object().ok_or_else(|| {
                                anyhow::anyhow!("Core operation reference must be an object")
                            })?;
                            exact_keys(operation, &["operation_id", "result_sha256"])?;
                            let id = string_field(operation, "operation_id")?;
                            let digest = string_field(operation, "result_sha256")?;
                            let duplicate = if version == 3 {
                                core_ids.insert(id.to_owned(), digest.to_owned()).is_some()
                            } else {
                                let unique_in_step = step_ids.insert(id.to_owned());
                                match core_ids.get(id) {
                                    Some(previous_digest) if previous_digest != digest => true,
                                    Some(_) => !unique_in_step,
                                    None => {
                                        core_ids.insert(id.to_owned(), digest.to_owned());
                                        !unique_in_step
                                    }
                                }
                            };
                            if !is_core_operation_id(id) || !is_sha256(digest) || duplicate {
                                bail!("Invalid or duplicate core operation reference")
                            }
                        }
                    }
                }
            }
            _ => bail!("Unsupported expert memory schema"),
        }
    }
    Ok(())
}

fn is_core_operation_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("core_") else {
        return false;
    };
    let Some((call, index)) = rest.split_once('_') else {
        return false;
    };
    if call.is_empty()
        || index.is_empty()
        || !call.bytes().all(|byte| byte.is_ascii_digit())
        || !index.bytes().all(|byte| byte.is_ascii_digit())
        || (call.len() > 1 && call.starts_with('0'))
        || (index.len() > 1 && index.starts_with('0'))
    {
        return false;
    }
    matches!(call.parse::<u16>(), Ok(1..=u16::MAX)) && matches!(index.parse::<u8>(), Ok(0..=3))
}

fn core_references(summary: &Value, version: u8) -> Result<Vec<(String, String)>> {
    if version < 3 {
        return Ok(Vec::new());
    }
    let mut references = BTreeMap::new();
    let mut add_results = |results: &Value| -> Result<()> {
        let results = results
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Expert results must be an object"))?;
        for result in results.values() {
            let Some(operations) = result.get("core_operations") else {
                continue;
            };
            for operation in operations
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Core operation references must be an array"))?
            {
                let operation = operation
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("Core operation reference must be an object"))?;
                references.insert(
                    string_field(operation, "operation_id")?.to_owned(),
                    string_field(operation, "result_sha256")?.to_owned(),
                );
            }
        }
        Ok(())
    };
    let object = summary
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Expert advice summary must be an object"))?;
    if let Some(experts) = object.get("experts") {
        for expert in experts
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Expert summaries must be an array"))?
        {
            let expert = expert
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Expert summary entry must be an object"))?;
            add_results(
                expert
                    .get("results")
                    .ok_or_else(|| anyhow::anyhow!("Expert summary results are missing"))?,
            )?;
        }
    } else if let Some(results) = object.get("results") {
        add_results(results)?;
    }
    drop(add_results);
    Ok(references.into_iter().collect())
}

fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Expert advice field {key} must be a string"))
}
