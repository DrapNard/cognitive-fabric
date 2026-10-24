//! Validation of acquisition provenance embedded in a reviewed source manifest.
//!
//! The evidence records exact bounded JSON payloads and hashes. Validation is
//! offline: source URLs are checked syntactically, but never resolved or fetched.

use super::{parse_source_url, valid_sha256, MAX_DOCUMENT_BYTES, MAX_SOURCES, MAX_TOTAL_BYTES};
use crate::{security, store::sha256};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const MAX_PAYLOAD_BYTES: usize = 32_000;
const MAX_EVIDENCE_BYTES: usize = 65_536;

/// Hashes binding the draft source manifest to the exact acquisition payloads.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub plan_sha256: String,
    pub report_sha256: String,
}

/// Exact raw acquisition plan and report retained as prepared-candidate evidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub format_version: u32,
    pub plan_sha256: String,
    pub report_sha256: String,
    pub plan_json: String,
    pub report_json: String,
}

/// Acquisition metadata associated with one source in the approved plan.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Origin {
    pub id: String,
    pub path: String,
    pub original_sha256: String,
    pub source: String,
    pub license: String,
    pub bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    format_version: u32,
    reviewer: String,
    acquisition_approved: bool,
    sources: Vec<PlanSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanSource {
    id: String,
    url: String,
    sha256: String,
    source: String,
    license: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    format_version: u32,
    manifest_sha256: String,
    plan_sha256: String,
    reviewer: String,
    acquisition_approved: bool,
    source_count: usize,
    total_bytes: usize,
    untrusted_data: bool,
    candidate_created: bool,
    model_promoted: bool,
    sources: Vec<ReportSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportSource {
    id: String,
    sha256: String,
    url_sha256: String,
    source: String,
    license: String,
    bytes: usize,
}

impl Evidence {
    /// Revalidate the exact payloads and return their source metadata.
    pub(crate) fn validate(&self) -> Result<Vec<Origin>> {
        if self.format_version != 1
            || !valid_sha256(&self.plan_sha256)
            || !valid_sha256(&self.report_sha256)
        {
            bail!("Acquisition evidence version or digest is invalid")
        }
        let plan_bytes = self.plan_json.as_bytes();
        let report_bytes = self.report_json.as_bytes();
        validate_payload_sizes(plan_bytes, report_bytes)?;
        if sha256(plan_bytes) != self.plan_sha256 || sha256(report_bytes) != self.report_sha256 {
            bail!("Acquisition evidence payload hash mismatch")
        }
        let origins = validate_payloads(plan_bytes, report_bytes)?;
        if serde_json::to_vec(self)?.len() > MAX_EVIDENCE_BYTES {
            bail!("Acquisition evidence exceeds its serialized size limit")
        }
        Ok(origins)
    }
}

/// Validate the raw files against a draft's binding and retain their exact bytes.
pub(crate) fn validate_binding(
    binding: &Binding,
    plan_bytes: &[u8],
    report_bytes: &[u8],
) -> Result<Evidence> {
    validate_payload_sizes(plan_bytes, report_bytes)?;
    if !valid_sha256(&binding.plan_sha256)
        || !valid_sha256(&binding.report_sha256)
        || sha256(plan_bytes) != binding.plan_sha256
        || sha256(report_bytes) != binding.report_sha256
    {
        bail!("Acquisition draft binding does not match the exact payloads")
    }
    // Validate before preserving the payloads so callers can reject invalid
    // provenance at the same trust boundary where the binding is checked.
    validate_payloads(plan_bytes, report_bytes)?;
    let evidence = Evidence {
        format_version: 1,
        plan_sha256: binding.plan_sha256.clone(),
        report_sha256: binding.report_sha256.clone(),
        plan_json: std::str::from_utf8(plan_bytes)?.to_owned(),
        report_json: std::str::from_utf8(report_bytes)?.to_owned(),
    };
    if serde_json::to_vec(&evidence)?.len() > MAX_EVIDENCE_BYTES {
        bail!("Acquisition evidence exceeds its serialized size limit")
    }
    Ok(evidence)
}

fn validate_payload_sizes(plan: &[u8], report: &[u8]) -> Result<()> {
    if plan.is_empty()
        || report.is_empty()
        || plan.len() > MAX_PAYLOAD_BYTES
        || report.len() > MAX_PAYLOAD_BYTES
    {
        bail!("Acquisition plan or report exceeds its byte limits")
    }
    Ok(())
}

fn validate_payloads(plan_bytes: &[u8], report_bytes: &[u8]) -> Result<Vec<Origin>> {
    let plan_value = crate::strict_json::value(plan_bytes, MAX_PAYLOAD_BYTES)?;
    let report_value = crate::strict_json::value(report_bytes, MAX_PAYLOAD_BYTES)?;
    let plan: Plan = serde_json::from_value(plan_value)?;
    let report: Report = serde_json::from_value(report_value)?;
    let plan_hash = sha256(plan_bytes);

    if plan.format_version != 1
        || !plan.acquisition_approved
        || plan.reviewer.trim().is_empty()
        || plan.reviewer.len() > 100
        || plan.sources.is_empty()
        || plan.sources.len() > MAX_SOURCES
    {
        bail!("Acquisition plan lacks valid bounded operator approval")
    }
    if report.format_version != 1
        || report.manifest_sha256 != plan_hash
        || report.plan_sha256 != plan_hash
        || report.reviewer != plan.reviewer
        || !report.acquisition_approved
        || report.source_count != plan.sources.len()
        || report.source_count == 0
        || report.source_count > MAX_SOURCES
        || !report.untrusted_data
        || report.candidate_created
        || report.model_promoted
        || report.sources.len() != plan.sources.len()
    {
        bail!("Acquisition report does not match its approved plan")
    }

    let mut ids = BTreeSet::new();
    let mut total_bytes = 0usize;
    let mut origins = Vec::with_capacity(plan.sources.len());
    for (source, reported) in plan.sources.into_iter().zip(report.sources) {
        if source.id.len() > 64
            || !crate::registry::identifier(&source.id)
            || !ids.insert(source.id.clone())
            || !valid_sha256(&source.sha256)
        {
            bail!("Acquisition plan contains invalid or duplicate source metadata")
        }
        security::validate_provenance(&source.source, &source.license)?;
        if source.source.len() > 100 || source.license.trim().len() > 128 {
            bail!("Acquisition source provenance exceeds its byte limit")
        }
        let url = parse_source_url(&source.url)?;
        if reported.id != source.id
            || reported.sha256 != source.sha256
            || reported.url_sha256 != sha256(url.as_str().as_bytes())
            || reported.source != source.source
            || reported.license != source.license
            || reported.bytes == 0
            || reported.bytes > MAX_DOCUMENT_BYTES
        {
            bail!("Acquisition report source does not match its plan")
        }
        total_bytes = total_bytes
            .checked_add(reported.bytes)
            .ok_or_else(|| anyhow::anyhow!("Acquisition report byte count overflow"))?;
        if total_bytes > MAX_TOTAL_BYTES {
            bail!("Acquisition report exceeds its total byte limit")
        }
        origins.push(Origin {
            path: format!("{}.txt", source.id),
            id: source.id,
            original_sha256: source.sha256,
            source: source.source,
            license: source.license,
            bytes: reported.bytes,
        });
    }
    if report.total_bytes != total_bytes {
        bail!("Acquisition report total byte count is inconsistent")
    }
    Ok(origins)
}
