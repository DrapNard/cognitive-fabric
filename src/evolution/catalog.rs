//! Operator-reviewed, hash-pinned catalog discovery for recorded capability gaps.
//!
//! This reads a local catalog only. It does not resolve URLs, fetch documents,
//! create candidates, grant permissions, evaluate skills, or promote artifacts.

use crate::{
    evolution::{acquisition, preparation},
    gaps::{self, Preparation},
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};
use url::Url;

const MAX_CATALOG_BYTES: usize = 65_536;
const MAX_CATALOG_SOURCES: usize = 128;
const MAX_SELECTED_SOURCES: usize = 32;
const DISCOVERY_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    format_version: u32,
    reviewer: String,
    catalog_reviewed: bool,
    sources: Vec<CatalogSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogSource {
    id: String,
    url: String,
    sha256: String,
    source: String,
    license: String,
    capabilities: Vec<String>,
    priority: u32,
}

struct SelectedSource {
    id: String,
    url: String,
    sha256: String,
    source: String,
    license: String,
    priority: u32,
}

/// A bounded acquisition-plan draft derived from a reviewed local catalog.
///
/// The store borrow lets `write` recheck that the original gap remains current
/// and unavailable immediately before the private bundle is published.
pub struct Draft<'a> {
    store: &'a Store,
    task: String,
    capability: String,
    gap: Preparation,
    started: Instant,
    manifest: Value,
    manifest_bytes: Vec<u8>,
    report: Value,
}

/// Discover exact-capability entries from an operator-reviewed local catalog.
///
/// Catalog URLs and hashes are metadata only. This function performs no DNS or
/// network access. The returned acquisition plan is deliberately unapproved.
pub fn discover<'a>(
    store: &'a Store,
    task: &str,
    capability: &str,
    catalog: &Path,
    expected_sha256: &str,
    max_sources: usize,
) -> Result<Draft<'a>> {
    let started = Instant::now();
    security::authorize("human-cli", "candidate.prepare")?;
    if !acquisition::valid_sha256(expected_sha256) {
        bail!("Catalog hash must be lowercase SHA-256")
    }
    if !(1..=MAX_SELECTED_SOURCES).contains(&max_sources) {
        bail!("Maximum selected source count must be between 1 and 32")
    }
    if !crate::registry::identifier(capability) {
        bail!("Invalid capability identity")
    }

    let gap = gaps::prepare_before(store, task, capability, started + DISCOVERY_DEADLINE)?;
    if gap.available_now {
        bail!("Capability is already available; reuse it before source discovery")
    }
    let raw = security::read_regular_bytes(catalog, MAX_CATALOG_BYTES)?;
    if expired(started) || sha256(&raw) != expected_sha256 {
        bail!("Catalog hash mismatch or discovery deadline exhausted")
    }
    let catalog_value = crate::strict_json::value(&raw, MAX_CATALOG_BYTES)?;
    let parsed: Catalog = serde_json::from_value(catalog_value)
        .context("Catalog must use the strict versioned source schema")?;
    validate_catalog_header(&parsed)?;

    // Validate every entry before selecting any match. This prevents malformed
    // or duplicate records from being hidden by the requested capability filter.
    let mut ids = BTreeSet::new();
    let mut urls = BTreeSet::new();
    let mut validated = Vec::with_capacity(parsed.sources.len());
    for source in parsed.sources {
        if expired(started) {
            bail!("Catalog discovery deadline exhausted")
        }
        let url = validate_catalog_entry(&source, &mut ids, &mut urls)?;
        validated.push((source, url));
    }
    if expired(started) {
        bail!("Catalog discovery deadline exhausted")
    }

    let mut matches = validated
        .into_iter()
        .filter_map(|(source, url)| {
            source
                .capabilities
                .iter()
                .any(|entry| entry == capability)
                .then_some(SelectedSource {
                    id: source.id,
                    url: url.to_string(),
                    sha256: source.sha256,
                    source: source.source,
                    license: source.license,
                    priority: source.priority,
                })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.id.cmp(&right.id))
    });
    let matched_count = matches.len();
    if matches.is_empty() {
        bail!("Catalog has no exact match for the recorded capability gap")
    }

    let mut content_hashes = BTreeSet::new();
    let mut selected = Vec::new();
    let mut duplicate_content_count = 0usize;
    for source in matches {
        if !content_hashes.insert(source.sha256.clone()) {
            duplicate_content_count += 1;
            continue;
        }
        if selected.len() < max_sources {
            selected.push(source);
        }
    }
    if selected.is_empty() {
        bail!("Catalog has no unique source content for the requested capability")
    }
    let selected_ids = selected
        .iter()
        .map(|source| source.id.clone())
        .collect::<Vec<_>>();
    let sources = selected
        .iter()
        .map(|source| {
            json!({
                "id": source.id,
                "url": source.url,
                "sha256": source.sha256,
                "source": source.source,
                "license": source.license
            })
        })
        .collect::<Vec<_>>();
    let manifest = json!({
        "format_version": 1,
        "reviewer": "",
        "acquisition_approved": false,
        "sources": sources
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    if manifest_bytes.len() > 32_000 {
        bail!("Generated acquisition plan exceeds its 32 KB limit")
    }
    let report = json!({
        "format_version": 1,
        "catalog_sha256": expected_sha256,
        "catalog_reviewer": parsed.reviewer,
        "catalog_reviewed": true,
        "task_id": task,
        "gap_event_id": gap.gap_event_id,
        "gap_checksum": gap.gap_checksum,
        "capability": capability,
        "available_now": false,
        "catalog_source_count": ids.len(),
        "matched_source_count": matched_count,
        "selected_source_count": selected_ids.len(),
        "duplicate_content_count": duplicate_content_count,
        "selected_ids": selected_ids,
        "manifest_sha256": sha256(&manifest_bytes),
        "untrusted_data": true,
        "network_used": false,
        "permissions_granted": false,
        "candidate_created": false,
        "evaluation_authority": false,
        "model_promoted": false
    });
    if serde_json::to_vec_pretty(&report)?.len() > 65_536 {
        bail!("Catalog discovery report exceeds its 64 KB limit")
    }
    if expired(started) {
        bail!("Catalog discovery deadline exhausted")
    }

    Ok(Draft {
        store,
        task: task.to_owned(),
        capability: capability.to_owned(),
        gap,
        started,
        manifest,
        manifest_bytes,
        report,
    })
}

impl Draft<'_> {
    /// Publish a private, unapproved acquisition plan and its discovery record.
    pub fn write(&self, output: &Path) -> Result<Value> {
        if expired(self.started) {
            bail!("Catalog discovery deadline exhausted")
        }
        let current_gap = gaps::prepare_before(
            self.store,
            &self.task,
            &self.capability,
            self.started + DISCOVERY_DEADLINE,
        )?;
        if current_gap.gap_event_id != self.gap.gap_event_id
            || current_gap.gap_checksum != self.gap.gap_checksum
            || current_gap.available_now
        {
            bail!("Recorded capability gap changed or became available")
        }
        if expired(self.started) {
            bail!("Catalog discovery deadline exhausted")
        }
        let manifest_bytes = serde_json::to_vec_pretty(&self.manifest)?;
        if manifest_bytes != self.manifest_bytes
            || self.report["manifest_sha256"].as_str() != Some(sha256(&manifest_bytes).as_str())
            || manifest_bytes.len() > 32_000
        {
            bail!("Generated acquisition plan changed or exceeds its bound")
        }
        let report_bytes = serde_json::to_vec_pretty(&self.report)?;
        if report_bytes.len() > 65_536 || expired(self.started) {
            bail!("Catalog discovery report exceeds its bound or deadline")
        }
        preparation::write_bundle(
            output,
            vec![
                ("acquisition-plan.json", manifest_bytes),
                ("discovery.json", report_bytes),
            ],
        )?;
        Ok(self.report.clone())
    }
}

fn expired(started: Instant) -> bool {
    started.elapsed() > DISCOVERY_DEADLINE
}

fn validate_catalog_header(catalog: &Catalog) -> Result<()> {
    if catalog.format_version != 1
        || !catalog.catalog_reviewed
        || catalog.reviewer.trim().is_empty()
        || catalog.reviewer.len() > 100
        || catalog.sources.is_empty()
        || catalog.sources.len() > MAX_CATALOG_SOURCES
    {
        bail!("Catalog lacks bounded operator review or source entries")
    }
    Ok(())
}

fn validate_catalog_entry(
    source: &CatalogSource,
    ids: &mut BTreeSet<String>,
    urls: &mut BTreeSet<String>,
) -> Result<Url> {
    if source.id.len() > 64
        || !crate::registry::identifier(&source.id)
        || !ids.insert(source.id.clone())
    {
        bail!("Invalid or duplicate catalog source identity")
    }
    if !acquisition::valid_sha256(&source.sha256) {
        bail!("Catalog source hash must be lowercase SHA-256")
    }
    if source.source.trim().is_empty()
        || source.source.len() > 100
        || source.license.trim().is_empty()
        || source.license.trim().len() > 128
    {
        bail!("Catalog provenance or license label exceeds its bounds")
    }
    security::validate_provenance(&source.source, &source.license)?;
    let url = acquisition::parse_source_url(&source.url)?;
    if !urls.insert(url.as_str().to_owned()) {
        bail!("Duplicate catalog source URL")
    }
    if source.priority > 1000 || source.capabilities.is_empty() || source.capabilities.len() > 16 {
        bail!("Catalog priority or capability count is outside its bounds")
    }
    let mut capabilities = BTreeSet::new();
    for entry in &source.capabilities {
        if !crate::registry::identifier(entry) || !capabilities.insert(entry) {
            bail!("Invalid or duplicate catalog capability")
        }
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_schema_rejects_duplicate_capabilities_and_unreviewed_catalogs() {
        let duplicate = json!({
            "format_version":1,"reviewer":"operator","catalog_reviewed":true,
            "sources":[{"id":"docs.runtime","url":"https://docs.example.org/a.txt",
                "sha256":"a".repeat(64),"source":"upstream docs","license":"CC0",
                "capabilities":["rust.s1","rust.s1"],"priority":10}]
        });
        let parsed: Catalog = serde_json::from_value(duplicate).unwrap();
        assert!(validate_catalog_header(&parsed).is_ok());
        assert!(validate_catalog_entry(
            &parsed.sources[0],
            &mut BTreeSet::new(),
            &mut BTreeSet::new()
        )
        .is_err());
        let unreviewed = json!({
            "format_version":1,"reviewer":"operator","catalog_reviewed":false,
            "sources":[{"id":"docs.runtime","url":"https://docs.example.org/a.txt",
                "sha256":"a".repeat(64),"source":"upstream docs","license":"CC0",
                "capabilities":["rust.s1"],"priority":10}]
        });
        let parsed: Catalog = serde_json::from_value(unreviewed).unwrap();
        assert!(validate_catalog_header(&parsed).is_err());
    }
}
