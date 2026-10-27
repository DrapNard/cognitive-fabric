//! Bounded independent typed operations for one client request.
//!
//! A plan groups independent operations for scheduling. It does not express
//! dependencies between operations and never substitutes one operation's
//! output into another operation's input.

use super::{translation::Translation, Work};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashSet};

pub(in crate::agent) const BATCH_PREFIX: &str = "typed-batch:";

const MAX_PLAN_BYTES: usize = 32_000;
const MAX_OUTPUT_BYTES: usize = 16_384;
const MAX_ENTRIES: usize = 4;

/// One validated operation in a plan. Entries can only be built by Plan.
pub(in crate::agent) struct Entry {
    pub(in crate::agent) id: String,
    pub(in crate::agent) work: Work,
    pub(in crate::agent) expert: Option<super::providers::Selection>,
    pub(in crate::agent) capability_requirement: Option<String>,
    pub(in crate::agent) template: Option<super::graphs::Template>,
    pub(in crate::agent) provider_route: Option<super::providers::PreparedRoute>,
}

/// A validated collection of independent typed operations.
pub(in crate::agent) struct Plan {
    query_digest: String,
    entries: Vec<Entry>,
    batch: bool,
    translated: bool,
}

/// Provenance and output-shape data retained after dispatch entries are moved.
pub(in crate::agent) struct PlanIdentity {
    query_digest: String,
    batch: bool,
    translated: bool,
    entry_ids: Vec<String>,
    frozen_outputs: HashSet<String>,
    output_contract: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchDocument {
    operations: Vec<BatchOperation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchOperation {
    id: String,
    query: String,
    #[serde(default)]
    expert: Option<super::providers::Selection>,
    #[serde(default)]
    capability_requirement: Option<String>,
}

impl Plan {
    /// Parse either one legacy typed query or the strict independent-batch form.
    /// Unrecognized ordinary language remains available to the normal path.
    pub(in crate::agent) fn from_query(query: &str) -> Result<Option<Self>> {
        if let Some(entries) = super::graphs::parse(query)? {
            return Ok(Some(Self {
                query_digest: crate::store::sha256(query.as_bytes()),
                entries,
                batch: true,
                translated: false,
            }));
        }

        if let Some(encoded) = query.strip_prefix(BATCH_PREFIX) {
            let value = crate::strict_json::value(encoded.as_bytes(), MAX_PLAN_BYTES)?;
            let document: BatchDocument = serde_json::from_value(value)?;
            if document.operations.is_empty() || document.operations.len() > MAX_ENTRIES {
                bail!("Typed batch must contain between one and four operations")
            }

            let mut ids = HashSet::with_capacity(document.operations.len());
            let mut entries = Vec::with_capacity(document.operations.len());
            for operation in document.operations {
                if let Some(expert) = &operation.expert {
                    expert.validate()?;
                }
                if let Some(requirement) = &operation.capability_requirement {
                    validate_capability_requirement(requirement)?;
                }
                validate_id(&operation.id)?;
                if !ids.insert(operation.id.clone()) {
                    bail!("Typed batch operation IDs must be distinct")
                }
                if operation.query.starts_with(BATCH_PREFIX) {
                    bail!("Nested typed batches are not supported")
                }
                if Work::capability_hint(&operation.query).is_none() {
                    bail!("Every typed batch operation must use a supported typed query")
                }
                let work = Work::from_query(&operation.query)?
                    .ok_or_else(|| anyhow::anyhow!("Typed batch operation is unsupported"))?;
                entries.push(Entry {
                    id: operation.id,
                    work,
                    expert: operation.expert,
                    capability_requirement: operation.capability_requirement,
                    template: None,
                    provider_route: None,
                });
            }

            return Ok(Some(Self {
                query_digest: crate::store::sha256(query.as_bytes()),
                entries,
                batch: true,
                translated: false,
            }));
        }

        let Some(work) = Work::from_query(query)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            query_digest: crate::store::sha256(query.as_bytes()),
            entries: vec![Entry {
                id: "result".to_owned(),
                work,
                expert: None,
                capability_requirement: None,
                template: None,
                provider_route: None,
            }],
            batch: false,
            translated: false,
        }))
    }

    /// Wrap one trusted language translation while preserving the exact source
    /// query as the plan identity and rejecting typed requests that must stay exact.
    pub(in crate::agent) fn from_translation(
        original: &str,
        translation: &Translation,
    ) -> Result<Self> {
        reject_explicit_translation_source(original)?;
        let work = Work::from_translation(original, translation)?;
        Ok(Self {
            query_digest: crate::store::sha256(original.as_bytes()),
            entries: vec![Entry {
                id: "result".to_owned(),
                work,
                expert: None,
                capability_requirement: None,
                template: None,
                provider_route: None,
            }],
            batch: false,
            translated: true,
        })
    }

    /// Build a dependency graph from a validated client-side translation while
    /// keeping every resolved operation tied to the original natural-language
    /// request. Model-provided expert and capability selectors remain forbidden.
    pub(in crate::agent) fn from_translated_graph(
        original: &str,
        graph: &super::graph_translation::Translation,
    ) -> Result<Self> {
        reject_explicit_translation_source(original)?;
        let canonical_query = graph.canonical_query();
        let mut entries = super::graphs::parse(canonical_query)?
            .ok_or_else(|| anyhow::anyhow!("Translated typed graph has no supported graph"))?;

        let declared = graph.capabilities().collect::<HashSet<_>>();
        let actual = entries
            .iter()
            .map(|entry| entry.work.capability().as_str())
            .collect::<HashSet<_>>();
        if declared != actual {
            bail!("Translated graph changed its capability set")
        }
        for entry in &mut entries {
            if entry.expert.is_some() || entry.capability_requirement.is_some() {
                bail!("Translated typed graph cannot select experts or capability versions")
            }
            entry.work.mark_language_translation(original)?;
            entry
                .template
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Translated graph operation has no template"))?
                .mark_language_translation(original)?;
        }

        Ok(Self {
            query_digest: crate::store::sha256(original.as_bytes()),
            entries,
            batch: true,
            translated: true,
        })
    }

    /// Wrap independently translated entries. Every translation is checked
    /// against the same original request by Work's existing adapter.
    pub(in crate::agent) fn from_translated_entries(
        original: &str,
        entries: Vec<(String, Translation)>,
    ) -> Result<Self> {
        reject_explicit_translation_source(original)?;
        if entries.is_empty() || entries.len() > MAX_ENTRIES {
            bail!("Translated typed batch must contain between one and four operations")
        }

        let mut ids = HashSet::with_capacity(entries.len());
        let mut validated = Vec::with_capacity(entries.len());
        let mut input_bytes = 0usize;
        for (id, translation) in entries {
            validate_id(&id)?;
            if !ids.insert(id.clone()) {
                bail!("Translated typed batch operation IDs must be distinct")
            }
            input_bytes = input_bytes.saturating_add(translation.canonical_query()?.len());
            if input_bytes > MAX_PLAN_BYTES {
                bail!("Translated typed batch exceeds its aggregate input bound")
            }
            let work = Work::from_translation(original, &translation)?;
            validated.push(Entry {
                id,
                work,
                expert: None,
                capability_requirement: None,
                template: None,
                provider_route: None,
            });
        }

        Ok(Self {
            query_digest: crate::store::sha256(original.as_bytes()),
            entries: validated,
            batch: true,
            translated: true,
        })
    }

    pub(in crate::agent) fn query_digest(&self) -> &str {
        &self.query_digest
    }

    pub(in crate::agent) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(in crate::agent) fn works(&self) -> impl Iterator<Item = &Work> {
        self.entries.iter().map(|entry| &entry.work)
    }

    pub(in crate::agent) fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Bind the provider's prepared routing proofs to entries in plan order.
    /// Existing proofs are immutable and cannot be silently replaced.
    pub(in crate::agent) fn bind_routes(
        &mut self,
        routes: Vec<super::providers::PreparedRoute>,
    ) -> Result<()> {
        if routes.len() != self.entries.len() {
            bail!("Prepared route count does not match typed plan entries")
        }
        if self
            .entries
            .iter()
            .any(|entry| entry.provider_route.is_some())
        {
            bail!("Typed plan already has prepared provider routes")
        }
        for (entry, route) in self.entries.iter_mut().zip(routes) {
            entry.provider_route = Some(route);
        }
        Ok(())
    }

    /// Count entries with a provider routing proof, including partial bindings.
    pub(in crate::agent) fn routes_present(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.provider_route.is_some())
            .count()
    }

    pub(in crate::agent) fn translated(&self) -> bool {
        self.translated
    }

    pub(in crate::agent) fn output_contract(&self) -> &'static str {
        if self.entries.iter().any(|entry| {
            entry
                .template
                .as_ref()
                .is_some_and(|template| template.binding_sources().next().is_some())
        }) {
            return "Return exactly one JSON object with exactly every operation ID as a key. For each dependency source, copy the verified output value exactly as supplied, preserving its numeric text and JSON representation. Do not modify dependency values or add prose, claims, or code fences.";
        }
        if self.batch {
            "Return exactly one JSON object whose keys are every operation ID and whose values are that operation's verified numeric or JSON answer; include no extra keys, prose, code fences, or added claims."
        } else {
            self.entries[0].work.output_contract()
        }
    }

    pub(in crate::agent) fn into_parts(self) -> (PlanIdentity, Vec<Entry>) {
        let output_contract = self.output_contract();
        let frozen_outputs = self
            .entries
            .iter()
            .filter_map(|entry| entry.template.as_ref())
            .flat_map(|template| template.binding_sources().map(str::to_owned))
            .collect();
        let identity = PlanIdentity {
            query_digest: self.query_digest,
            batch: self.batch,
            translated: self.translated,
            entry_ids: self.entries.iter().map(|entry| entry.id.clone()).collect(),
            frozen_outputs,
            output_contract,
        };
        (identity, self.entries)
    }
}

impl PlanIdentity {
    pub(in crate::agent) fn query_digest(&self) -> &str {
        &self.query_digest
    }

    pub(in crate::agent) fn entry_ids(&self) -> impl Iterator<Item = &str> {
        self.entry_ids.iter().map(String::as_str)
    }

    pub(in crate::agent) fn translated(&self) -> bool {
        self.translated
    }

    pub(in crate::agent) fn output_contract(&self) -> &'static str {
        self.output_contract
    }

    pub(in crate::agent) fn requires_exact_output(&self, id: &str) -> bool {
        self.frozen_outputs.contains(id)
    }

    /// Assemble verified per-operation answers into the public response shape.
    pub(in crate::agent) fn assemble(&self, results: &[(String, String)]) -> Result<String> {
        if results.len() != self.entry_ids.len() {
            bail!("Typed plan result count does not match its operations")
        }

        if !self.batch {
            let (id, result) = results
                .first()
                .ok_or_else(|| anyhow::anyhow!("Typed plan has no result"))?;
            if id != "result" || self.entry_ids.as_slice() != ["result"] {
                bail!("Single typed plan result has an unexpected operation ID")
            }
            if result.len() > MAX_OUTPUT_BYTES {
                bail!("Typed plan result exceeds its byte limit")
            }
            return Ok(result.clone());
        }

        let expected: HashSet<&str> = self.entry_ids.iter().map(String::as_str).collect();
        let mut output = BTreeMap::new();
        for (id, result) in results {
            if !expected.contains(id.as_str()) || output.contains_key(id) {
                bail!("Typed plan results contain an unknown or duplicate operation ID")
            }
            crate::strict_json::value(result.as_bytes(), MAX_OUTPUT_BYTES)?;
            // Preserve the verified numeric spelling instead of rounding or
            // rewriting it while wrapping an answer in the batch object.
            let value = RawValue::from_string(result.clone())?;
            output.insert(id.clone(), value);
        }
        if output.len() != expected.len() {
            bail!("Typed plan results are missing an operation")
        }
        let answer = serde_json::to_string(&output)?;
        if answer.len() > MAX_OUTPUT_BYTES {
            bail!("Assembled typed plan result exceeds its byte limit")
        }
        Ok(answer)
    }

    /// Split the final language presentation into one value per expected entry.
    /// Batch keys must match exactly. Raw value text remains intact so exact
    /// arithmetic checking cannot accept a different decimal rounded to one f64.
    pub(in crate::agent) fn presentation_parts(
        &self,
        expected_ids: &[&str],
        answer: &str,
    ) -> Result<Vec<String>> {
        if answer.len() > MAX_OUTPUT_BYTES {
            bail!("Typed plan presentation exceeds its byte limit")
        }
        if expected_ids.len() != self.entry_ids.len()
            || expected_ids
                .iter()
                .zip(&self.entry_ids)
                .any(|(expected, actual)| *expected != actual.as_str())
        {
            bail!("Typed plan presentation IDs do not match the validated plan")
        }

        if !self.batch {
            if expected_ids.len() != 1 || expected_ids[0] != "result" {
                bail!("Single typed plan presentation has an unexpected operation ID")
            }
            return Ok(vec![answer.to_owned()]);
        }

        crate::strict_json::value(answer.as_bytes(), MAX_OUTPUT_BYTES)?;
        let object: BTreeMap<String, Box<RawValue>> = serde_json::from_str(answer)?;
        if object.len() != expected_ids.len()
            || expected_ids.iter().any(|id| !object.contains_key(*id))
        {
            bail!("Typed batch presentation keys do not match its operation IDs")
        }
        expected_ids
            .iter()
            .map(|id| {
                object
                    .get(*id)
                    .map(|value| value.get().to_owned())
                    .ok_or_else(|| {
                        anyhow::anyhow!("Typed batch presentation is missing an operation")
                    })
            })
            .collect()
    }
}

fn reject_explicit_translation_source(original: &str) -> Result<()> {
    if original.starts_with(BATCH_PREFIX) || Work::has_explicit_prefix(original) {
        bail!("Language translation cannot replace an explicit typed goal")
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    let mut bytes = id.bytes();
    let valid_first = bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic());
    if id.is_empty()
        || id.len() > 32
        || !valid_first
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        bail!("Typed batch operation ID must be 1-32 ASCII letters, digits, or underscores and start with a letter")
    }
    Ok(())
}

fn validate_capability_requirement(requirement: &str) -> Result<()> {
    if requirement.is_empty() || requirement.len() > 64 {
        bail!("Typed batch capability requirement must contain 1-64 bytes")
    }
    semver::VersionReq::parse(requirement)?;
    Ok(())
}
