use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowCandidate {
    pub id: String,
    pub version: String,
    pub snapshot_sha256: String,
    pub required_calls: u32,
    pub required_output_tokens: u32,
}
impl WorkflowCandidate {
    pub fn from_pinned(
        manifest: &crate::registry::Manifest,
        model: Option<&crate::models::Registered>,
    ) -> Result<Self> {
        manifest.validate()?;
        if manifest.kind != "workflow" || manifest.model.is_some() != model.is_some() {
            bail!("Workflow ranking requires an available pinned workflow/model")
        }
        if let Some(model) = model {
            model.manifest.validate()?;
            let selection = manifest.model.as_ref().unwrap();
            if !model.enabled
                || selection.id != model.manifest.id
                || selection.version != model.manifest.version
                || model.checksum != store::sha256(&serde_json::to_vec(&model.manifest)?)
            {
                bail!("Workflow model fingerprint is unavailable or inconsistent")
            }
        }
        let calls = manifest.workflow_steps.as_ref().map_or(1, Vec::len) as u32;
        Ok(Self {
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            snapshot_sha256: store::sha256(&serde_json::to_vec(&(manifest, model))?),
            required_calls: calls,
            required_output_tokens: calls * 192,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRequest {
    pub mode: String,
    pub criterion: Option<String>,
    pub configuration_sha256: String,
    pub role_prompts_sha256: String,
    pub runtime_sources_sha256: String,
    pub evidence_sha256: String,
    pub remaining_calls: usize,
    pub remaining_output_tokens: usize,
    pub remaining_prompt_bytes: usize,
    pub remaining_tool_calls: u32,
    pub remaining_ms: u64,
    pub main_prompt_bytes: usize,
    pub candidates: Vec<WorkflowCandidate>,
}
impl WorkflowRequest {
    pub(super) fn validate(&self) -> Result<()> {
        if self.criterion.as_deref().is_some_and(|criterion| {
            !matches!(
                criterion,
                "integer_sort/v1" | "dependency_order/v1" | "shortest_path/v1" | "finite_domain/v1"
            )
        }) {
            bail!("Unsupported workflow verification criterion")
        }
        if !matches!(
            self.mode.as_str(),
            "fast" | "standard" | "deep" | "research"
        ) || self.candidates.len() > 4
            || [
                &self.configuration_sha256,
                &self.role_prompts_sha256,
                &self.runtime_sources_sha256,
                &self.evidence_sha256,
            ]
            .iter()
            .any(|s| !hash(s))
            || self.remaining_calls > 64
            || self.remaining_output_tokens > 16384
            || self.remaining_prompt_bytes > 1_000_000
            || self.remaining_tool_calls > 64
            || self.remaining_ms > 3_600_000
            || self.main_prompt_bytes > 1_000_000
        {
            bail!("Invalid bounded workflow routing context")
        }
        let mut ids = BTreeSet::new();
        for candidate in &self.candidates {
            if !crate::registry::identifier(&candidate.id)
                || candidate.id.len() > 101
                || candidate.version.len() > 64
                || semver::Version::parse(&candidate.version)?.to_string() != candidate.version
                || !hash(&candidate.snapshot_sha256)
                || !(1..=4).contains(&candidate.required_calls)
                || candidate.required_output_tokens != candidate.required_calls * 192
                || !ids.insert(&candidate.id)
            {
                bail!("Invalid pinned workflow routing candidate")
            }
        }
        Ok(())
    }
}
pub(super) fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Metadata/content identities of the pinned knowledge actually available to this
/// task. Query text, answers and private credential material are never included.
pub(crate) fn workflow_evidence_fingerprint(context: &store::TaskContext) -> Result<String> {
    let mut skills = context
        .skills
        .iter()
        .map(|skill| Ok((skill.id.clone(), store::sha256(&serde_json::to_vec(skill)?))))
        .collect::<Result<Vec<_>>>()?;
    let mut manifests = context
        .manifests
        .iter()
        .map(|manifest| {
            Ok((
                manifest.id.clone(),
                store::sha256(&serde_json::to_vec(manifest)?),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut documents = context
        .documents
        .iter()
        .map(|document| {
            (
                document.id.clone(),
                store::sha256(document.body.as_bytes()),
                document.title.clone(),
                document.source.clone(),
                document.license.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut memories = context
        .memories
        .iter()
        .map(|memory| (memory.id.clone(), memory.checksum.clone()))
        .collect::<Vec<_>>();
    skills.sort();
    manifests.sort();
    documents.sort();
    memories.sort();
    Ok(store::sha256(&serde_json::to_vec(&(
        skills, manifests, documents, memories,
    ))?))
}
