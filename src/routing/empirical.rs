//! Reviewed paired outcomes for optional workflow ranking, never permissions.
use super::workflow::hash;
use super::{Descriptor, ExpertRouter, Request, TagRouter, WorkflowRequest};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_BYTES: usize = 524_288;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub task_id: String,
    pub verified: bool,
    pub model_calls: u32,
    pub reserved_output_tokens: u32,
    pub prompt_bytes: u32,
    pub tool_calls: u32,
    pub elapsed_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub case_sha256: String,
    pub baseline: Measurement,
    pub expert: Measurement,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub workflow_id: String,
    pub workflow_version: String,
    pub workflow_sha256: String,
    pub pairs: Vec<Pair>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub format_version: u32,
    pub mode: String,
    pub criterion: String,
    pub configuration_sha256: String,
    pub role_prompts_sha256: String,
    pub runtime_sources_sha256: String,
    pub pinned_evidence_sha256: String,
    pub dataset_sha256: String,
    pub source: String,
    pub license: String,
    pub reviewer: String,
    pub held_out_operator_attested: bool,
    pub independent_outcomes_operator_attested: bool,
    pub includes_failed_attempts_operator_attested: bool,
    pub studies: Vec<Study>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    pub format_version: u32,
    pub min_pairs: u32,
    pub min_net_successes: u32,
    pub min_net_gain_per_mille: u32,
    pub max_extra_output_tokens_per_net_success: u64,
    pub main_solver_reserve_ms: u64,
}
impl Thresholds {
    fn validate(&self) -> Result<()> {
        if self.format_version != 1
            || !(1..=1024).contains(&self.min_pairs)
            || !(1..=1024).contains(&self.min_net_successes)
            || !(1..=1000).contains(&self.min_net_gain_per_mille)
            || self.max_extra_output_tokens_per_net_success > 16_777_216
            || !(1..=300_000).contains(&self.main_solver_reserve_ms)
        {
            bail!("Invalid workflow routing thresholds")
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub workflow_id: String,
    pub workflow_version: String,
    pub workflow_sha256: String,
    pub pairs: u32,
    pub wins: u32,
    pub regressions: u32,
    pub extra_output_tokens: u64,
    pub max_extra_calls: u32,
    pub max_extra_output_tokens: u32,
    pub max_extra_prompt_bytes: u32,
    pub max_extra_ms: u64,
}
impl Summary {
    fn net(&self) -> u32 {
        self.wins.saturating_sub(self.regressions)
    }
}
pub struct EvidenceRouter {
    descriptor: Descriptor,
    evidence: Evidence,
    thresholds: Thresholds,
    summaries: Vec<Summary>,
}
impl EvidenceRouter {
    pub fn from_json(bytes: &[u8], thresholds: Thresholds) -> Result<Self> {
        thresholds.validate()?;
        let mut evidence: Evidence =
            serde_json::from_value(crate::strict_json::value(bytes, MAX_BYTES)?)?;
        if evidence.format_version != 1
            || !matches!(evidence.mode.as_str(), "standard" | "deep" | "research")
            || !matches!(
                evidence.criterion.as_str(),
                "integer_sort/v1" | "dependency_order/v1" | "shortest_path/v1" | "finite_domain/v1"
            )
            || [
                &evidence.configuration_sha256,
                &evidence.role_prompts_sha256,
                &evidence.runtime_sources_sha256,
                &evidence.pinned_evidence_sha256,
                &evidence.dataset_sha256,
            ]
            .iter()
            .any(|v| !hash(v))
            || evidence.reviewer.len() > 64
            || !crate::registry::identifier(&evidence.reviewer)
            || !evidence.held_out_operator_attested
            || !evidence.independent_outcomes_operator_attested
            || !evidence.includes_failed_attempts_operator_attested
            || evidence.studies.is_empty()
            || evidence.studies.len() > 32
        {
            bail!("Invalid or unreviewed workflow routing evidence")
        }
        crate::security::validate_provenance(&evidence.source, &evidence.license)?;
        let mut workflows = BTreeSet::new();
        let mut task_ids = BTreeSet::new();
        let mut rows = 0;
        for study in &mut evidence.studies {
            if study.workflow_id.len() > 101
                || !crate::registry::identifier(&study.workflow_id)
                || study.workflow_version.len() > 64
                || semver::Version::parse(&study.workflow_version)?.to_string()
                    != study.workflow_version
                || !hash(&study.workflow_sha256)
                || !workflows.insert((study.workflow_id.clone(), study.workflow_version.clone()))
                || study.pairs.is_empty()
            {
                bail!("Invalid or duplicate workflow study")
            }
            let mut cases = BTreeSet::new();
            for pair in &study.pairs {
                rows += 1;
                if rows > 1024 || !hash(&pair.case_sha256) || !cases.insert(&pair.case_sha256) {
                    bail!("Duplicate or excessive paired workflow observations")
                }
                for sample in [&pair.baseline, &pair.expert] {
                    if uuid::Uuid::parse_str(&sample.task_id)?.to_string() != sample.task_id
                        || !task_ids.insert(sample.task_id.clone())
                        || sample.model_calls > 64
                        || sample.reserved_output_tokens > 16384
                        || sample.prompt_bytes > 1_000_000
                        || sample.tool_calls > 64
                        || sample.elapsed_ms > 3_600_000
                        || (sample.verified && sample.model_calls == 0)
                    {
                        bail!("Invalid or reused paired task measurement")
                    }
                }
            }
            study
                .pairs
                .sort_by(|a, b| a.case_sha256.cmp(&b.case_sha256));
        }
        evidence.studies.sort_by(|a, b| {
            (&a.workflow_id, &a.workflow_version).cmp(&(&b.workflow_id, &b.workflow_version))
        });
        let digest = crate::store::sha256(&serde_json::to_vec(&(&evidence, &thresholds))?);
        let descriptor = Descriptor {
            id: format!("jev-plus.workflow.{}", &digest[..8]),
            version: format!("1.0.0+{}", &digest[8..]),
        };
        descriptor.validate()?;
        let summaries = evidence
            .studies
            .iter()
            .map(|study| {
                let mut summary = Summary {
                    workflow_id: study.workflow_id.clone(),
                    workflow_version: study.workflow_version.clone(),
                    workflow_sha256: study.workflow_sha256.clone(),
                    pairs: study.pairs.len() as u32,
                    wins: 0,
                    regressions: 0,
                    extra_output_tokens: 0,
                    max_extra_calls: 0,
                    max_extra_output_tokens: 0,
                    max_extra_prompt_bytes: 0,
                    max_extra_ms: 0,
                };
                for pair in &study.pairs {
                    summary.wins += u32::from(!pair.baseline.verified && pair.expert.verified);
                    summary.regressions +=
                        u32::from(pair.baseline.verified && !pair.expert.verified);
                    let extra = pair
                        .expert
                        .reserved_output_tokens
                        .saturating_sub(pair.baseline.reserved_output_tokens);
                    summary.extra_output_tokens += u64::from(extra);
                    summary.max_extra_calls = summary.max_extra_calls.max(
                        pair.expert
                            .model_calls
                            .saturating_sub(pair.baseline.model_calls),
                    );
                    summary.max_extra_output_tokens = summary.max_extra_output_tokens.max(extra);
                    summary.max_extra_prompt_bytes = summary.max_extra_prompt_bytes.max(
                        pair.expert
                            .prompt_bytes
                            .saturating_sub(pair.baseline.prompt_bytes),
                    );
                    summary.max_extra_ms = summary.max_extra_ms.max(
                        pair.expert
                            .elapsed_ms
                            .saturating_sub(pair.baseline.elapsed_ms),
                    );
                }
                summary
            })
            .collect();
        Ok(Self {
            descriptor,
            evidence,
            thresholds,
            summaries,
        })
    }
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({"policy":self.descriptor,"observed_workflows":self.summaries,"thresholds":self.thresholds,"outcomes_independently_proven":false,"probabilities_calibrated":false,"permissions_granted":false})
    }
}
impl ExpertRouter for EvidenceRouter {
    fn descriptor(&self) -> Descriptor {
        self.descriptor.clone()
    }
    fn select(&self, request: Request<'_>) -> Result<Vec<String>> {
        TagRouter.select(request)
    }
    fn rank_workflows(&self, request: &WorkflowRequest) -> Result<Vec<String>> {
        request.validate()?;
        if request.mode != self.evidence.mode
            || request.criterion.as_deref() != Some(&self.evidence.criterion)
            || request.configuration_sha256 != self.evidence.configuration_sha256
            || request.role_prompts_sha256 != self.evidence.role_prompts_sha256
            || request.runtime_sources_sha256 != self.evidence.runtime_sources_sha256
            || request.evidence_sha256 != self.evidence.pinned_evidence_sha256
            || request.remaining_tool_calls == 0
        {
            return Ok(vec![]);
        }
        let mut ranked = Vec::new();
        for candidate in &request.candidates {
            let Some(summary) = self.summaries.iter().find(|s| {
                s.workflow_id == candidate.id
                    && s.workflow_version == candidate.version
                    && s.workflow_sha256 == candidate.snapshot_sha256
            }) else {
                continue;
            };
            let net = summary.net();
            let cost = summary
                .extra_output_tokens
                .max(u64::from(summary.pairs) * u64::from(candidate.required_output_tokens));
            let calls = summary.max_extra_calls.max(candidate.required_calls) as usize;
            let outputs = summary
                .max_extra_output_tokens
                .max(candidate.required_output_tokens) as usize;
            if summary.pairs < self.thresholds.min_pairs
                || net < self.thresholds.min_net_successes
                || u128::from(net) * 1000
                    < u128::from(self.thresholds.min_net_gain_per_mille) * u128::from(summary.pairs)
                || u128::from(cost)
                    > u128::from(self.thresholds.max_extra_output_tokens_per_net_success)
                        * u128::from(net)
                || request.remaining_calls < calls + 1
                || request.remaining_output_tokens < outputs + 384
                || request.remaining_prompt_bytes
                    < (summary.max_extra_prompt_bytes as usize)
                        .saturating_add(request.main_prompt_bytes)
                || request.remaining_ms
                    < summary
                        .max_extra_ms
                        .saturating_add(self.thresholds.main_solver_reserve_ms)
            {
                continue;
            }
            ranked.push((summary, cost));
        }
        ranked.sort_by(|(a, ac), (b, bc)| {
            (u128::from(b.net()) * u128::from(*ac))
                .cmp(&(u128::from(a.net()) * u128::from(*bc)))
                .then_with(|| a.workflow_id.cmp(&b.workflow_id))
        });
        Ok(ranked
            .into_iter()
            .map(|(s, _)| s.workflow_id.clone())
            .collect())
    }
}
pub fn load_files(study: &std::path::Path, thresholds: &std::path::Path) -> Result<EvidenceRouter> {
    crate::security::authorize("human-cli", "evaluation.routing")?;
    let bytes = crate::security::read_regular_bytes(thresholds, 4096)?;
    let thresholds = serde_json::from_value(crate::strict_json::value(&bytes, 4096)?)?;
    EvidenceRouter::from_json(
        &crate::security::read_regular_bytes(study, MAX_BYTES)?,
        thresholds,
    )
}
pub fn evaluate_files(
    study: &std::path::Path,
    thresholds: &std::path::Path,
) -> Result<serde_json::Value> {
    let policy = load_files(study, thresholds)?;
    Ok(
        serde_json::json!({"evaluation":policy.summary(),"installed":false,"provider_health_proven":false,"permissions_granted":false}),
    )
}

#[cfg(test)]
mod tests;
