//! Operator-reviewed measurements for one independently rejected solver retry.
//! Observed rates are not calibrated probabilities or provider attestation.
use super::{Decision, Descriptor, EscalationPolicy, Reason, Request};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_STUDY_BYTES: usize = 524_288;
pub const MAX_OBSERVATIONS: usize = 1024;

pub fn runtime_sources_fingerprint() -> &'static str {
    env!("COGFABRIC_RUNTIME_SOURCES_SHA256")
}

/// Operator-only inspection. This neither installs a policy nor creates a Store.
pub fn evaluate_files(
    study: &std::path::Path,
    thresholds: &std::path::Path,
) -> Result<serde_json::Value> {
    crate::security::authorize("human-cli", "evaluation.retry")?;
    let thresholds = crate::security::read_regular_bytes(thresholds, 4096)?;
    let thresholds: Thresholds =
        serde_json::from_value(crate::strict_json::value(&thresholds, 4096)?)?;
    let bytes = crate::security::read_regular_bytes(study, MAX_STUDY_BYTES)?;
    let policy = EmpiricalPolicy::from_json(&bytes, thresholds.clone())?;
    Ok(serde_json::json!({
        "evaluation":policy.summary(),"policy":policy.descriptor(),"thresholds":thresholds,
        "installed":false,"permissions_granted":false,"provider_health_proven":false
    }))
}

/// The complete effective configuration, excluding the nonserialized home path.
pub fn configuration_fingerprint(config: &crate::config::Config) -> Result<String> {
    config.validate()?;
    Ok(crate::store::sha256(&serde_json::to_vec(config)?))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetryStudy {
    pub format_version: u32,
    pub mode: String,
    pub criterion: String,
    pub cycle: u32,
    pub configuration_sha256: String,
    pub role_prompts_sha256: String,
    pub runtime_sources_sha256: String,
    pub dataset_sha256: String,
    pub source: String,
    pub license: String,
    pub reviewer: String,
    pub held_out_operator_attested: bool,
    pub independent_outcomes_operator_attested: bool,
    pub includes_failed_attempts_operator_attested: bool,
    pub observations: Vec<Observation>,
}

/// Every charged retry, including failed, timed-out and cancelled attempts.
/// `verified` means the independent checker accepted the revised answer.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub task_id: String,
    pub verified: bool,
    pub model_calls: u32,
    pub reserved_output_tokens: u32,
    pub tool_calls: u32,
    pub prompt_bytes: u32,
    pub elapsed_ms: u64,
}

/// Immutable operator choices. Limits are costs per independently verified gain,
/// counting the resources of unsuccessful attempts as well as successful ones.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    pub min_observations: u32,
    pub min_success_basis_points: u32,
    pub max_output_tokens_per_success: u64,
    pub max_prompt_bytes_per_success: u64,
    pub max_milliseconds_per_success: u64,
}
impl Thresholds {
    fn validate(&self) -> Result<()> {
        if !(1..=MAX_OBSERVATIONS as u32).contains(&self.min_observations)
            || !(1..=10_000).contains(&self.min_success_basis_points)
            || !(1..=16_384_000).contains(&self.max_output_tokens_per_success)
            || !(1..=1_000_000_000).contains(&self.max_prompt_bytes_per_success)
            || !(1..=3_600_000_000).contains(&self.max_milliseconds_per_success)
        {
            bail!("Invalid empirical retry thresholds")
        }
        Ok(())
    }
}

#[derive(Clone, Serialize)]
pub struct Summary {
    pub format_version: u32,
    /// Binds all original study bytes after canonical serialization and thresholds.
    pub evidence_sha256: String,
    pub dataset_sha256: String,
    pub mode: String,
    pub criterion: String,
    pub cycle: u32,
    pub configuration_sha256: String,
    pub role_prompts_sha256: String,
    pub runtime_sources_sha256: String,
    pub observations: u32,
    pub verified_retries: u32,
    pub observed_success_basis_points: u32,
    pub total_model_calls: u64,
    pub total_reserved_output_tokens: u64,
    pub total_tool_calls: u64,
    pub total_prompt_bytes: u64,
    pub total_elapsed_ms: u64,
    pub max_observed_elapsed_ms: u64,
    pub review_operator_attested: bool,
    pub calibrated_probability: bool,
    pub weights_attested: bool,
}

/// Native host construction only; no model/CLI policy installation or promotion.
/// Each value is immutable. A changed study or threshold yields a new descriptor.
pub struct EmpiricalPolicy {
    descriptor: Descriptor,
    summary: Summary,
    thresholds: Thresholds,
}
impl EmpiricalPolicy {
    pub fn from_json(bytes: &[u8], thresholds: Thresholds) -> Result<Self> {
        thresholds.validate()?;
        let value = crate::strict_json::value(bytes, MAX_STUDY_BYTES)?;
        let study: RetryStudy = serde_json::from_value(value)?;
        if study.format_version != 1
            || !matches!(study.mode.as_str(), "deep" | "research")
            || !matches!(
                study.criterion.as_str(),
                "integer_sort/v1" | "dependency_order/v1" | "shortest_path/v1"
            )
            || !(1..=15).contains(&study.cycle)
            || !hash(&study.configuration_sha256)
            || !hash(&study.role_prompts_sha256)
            || !hash(&study.runtime_sources_sha256)
            || !hash(&study.dataset_sha256)
            || !crate::registry::identifier(&study.reviewer)
            || study.reviewer.len() > 64
            || !study.held_out_operator_attested
            || !study.independent_outcomes_operator_attested
            || !study.includes_failed_attempts_operator_attested
            || study.observations.is_empty()
            || study.observations.len() > MAX_OBSERVATIONS
        {
            bail!("Invalid or unreviewed empirical retry study")
        }
        crate::security::validate_provenance(&study.source, &study.license)?;
        let mut ids = BTreeSet::new();
        for row in &study.observations {
            if uuid::Uuid::parse_str(&row.task_id)?.to_string() != row.task_id
                || !ids.insert(&row.task_id)
                || row.model_calls != 1
                || row.reserved_output_tokens != 384
                || row.tool_calls > 1
                || (row.verified && row.tool_calls != 1)
                || !(1..=32_000).contains(&row.prompt_bytes)
                || row.elapsed_ms > 3_600_000
            {
                bail!("Invalid, duplicate or unsupported retry observation")
            }
        }
        let evidence_sha256 = crate::store::sha256(&serde_json::to_vec(&(&study, &thresholds))?);
        let observations = study.observations.len() as u32;
        let verified_retries = study.observations.iter().filter(|row| row.verified).count() as u32;
        let summary = Summary {
            format_version: 1,
            evidence_sha256: evidence_sha256.clone(),
            dataset_sha256: study.dataset_sha256,
            mode: study.mode,
            criterion: study.criterion,
            cycle: study.cycle,
            configuration_sha256: study.configuration_sha256,
            role_prompts_sha256: study.role_prompts_sha256,
            runtime_sources_sha256: study.runtime_sources_sha256,
            observations,
            verified_retries,
            observed_success_basis_points: verified_retries * 10_000 / observations,
            total_model_calls: study
                .observations
                .iter()
                .map(|r| u64::from(r.model_calls))
                .sum(),
            total_reserved_output_tokens: study
                .observations
                .iter()
                .map(|r| u64::from(r.reserved_output_tokens))
                .sum(),
            total_tool_calls: study
                .observations
                .iter()
                .map(|r| u64::from(r.tool_calls))
                .sum(),
            total_prompt_bytes: study
                .observations
                .iter()
                .map(|r| u64::from(r.prompt_bytes))
                .sum(),
            total_elapsed_ms: study.observations.iter().map(|r| r.elapsed_ms).sum(),
            max_observed_elapsed_ms: study
                .observations
                .iter()
                .map(|r| r.elapsed_ms)
                .max()
                .unwrap(),
            review_operator_attested: true,
            calibrated_probability: false,
            weights_attested: false,
        };
        // Preserve all 256 digest bits within the bounded logical descriptor.
        let descriptor = Descriptor {
            id: format!("jev-plus.empirical.{}", &evidence_sha256[..8]),
            version: format!("1.0.0+{}", &evidence_sha256[8..]),
        };
        descriptor.validate()?;
        Ok(Self {
            descriptor,
            summary,
            thresholds,
        })
    }

    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    fn reason(&self, request: &Request) -> &'static str {
        let s = &self.summary;
        let t = &self.thresholds;
        if !matches!(request.reason, Reason::IndependentRejection)
            || request.verification_criterion.is_none()
        {
            return "independent_rejection_required";
        }
        if request.mode != s.mode
            || request.verification_criterion.as_deref() != Some(s.criterion.as_str())
            || request.cycle != s.cycle
            || request.configuration_sha256 != s.configuration_sha256
            || request.role_prompts_sha256 != s.role_prompts_sha256
            || request.runtime_sources_sha256 != s.runtime_sources_sha256
        {
            return "study_scope_mismatch";
        }
        if request.required_calls != 1
            || request.required_output_tokens != 384
            || request.required_tool_calls != 1
            || request.required_prompt_bytes == 0
            || request.required_prompt_bytes > 32_000
            || request.remaining_calls < request.required_calls
            || request.remaining_output_tokens < request.required_output_tokens
            || request.remaining_prompt_bytes < request.required_prompt_bytes
            || request.remaining_tool_calls < request.required_tool_calls
        {
            return "unsupported_or_unaffordable_retry";
        }
        if s.observations < t.min_observations {
            return "insufficient_observations";
        }
        if s.verified_retries == 0
            || u64::from(s.verified_retries) * 10_000
                < u64::from(t.min_success_basis_points) * u64::from(s.observations)
        {
            return "observed_success_rate_too_low";
        }
        let successes = u64::from(s.verified_retries);
        if s.total_reserved_output_tokens > t.max_output_tokens_per_success * successes
            || s.total_prompt_bytes > t.max_prompt_bytes_per_success * successes
            || s.total_elapsed_ms > t.max_milliseconds_per_success * successes
        {
            return "observed_cost_per_success_too_high";
        }
        // Avoid applying measurements from short prompts to a more costly retry.
        // Failures count in the numerator; compare without rounding or floats.
        if request.required_prompt_bytes as u64 * u64::from(s.observations) > s.total_prompt_bytes {
            return "retry_prompt_exceeds_observed_mean";
        }
        if request.remaining_ms < s.max_observed_elapsed_ms.max(1) {
            return "observed_duration_exceeds_deadline";
        }
        "empirical_retry_admitted"
    }
}
impl EscalationPolicy for EmpiricalPolicy {
    fn descriptor(&self) -> Descriptor {
        self.descriptor.clone()
    }
    fn decide(&self, request: &Request) -> Result<Decision> {
        let reason = self.reason(request);
        Ok(Decision {
            allow: reason == "empirical_retry_admitted",
            reason_code: reason.into(),
        })
    }
}
fn hash(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests;
