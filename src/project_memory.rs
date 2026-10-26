//! Bounded, versioned project facts stored as typed records rather than transcripts.
//!
//! This module defines the wire/storage shape only. Records are untrusted context;
//! they never grant permissions, amend task budgets, or prove their own claims.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FORMAT_VERSION: u16 = 1;
pub const MAX_RECORD_BYTES: usize = 16_384;
pub const MAX_FACTS: usize = 48;
pub const MAX_RELATIONS: usize = 48;
pub const MAX_CONSTRAINTS: usize = 32;
pub const MAX_EVIDENCE: usize = 64;
pub const MAX_REVISIONS_PER_PROJECT: u32 = 32;
pub const MAX_TOTAL_RECORDS: u64 = 4_096;
pub const STORAGE_SKILL_ID: &str = "project.memory.v1";

/// Immutable evidence pointer. The referenced object stays in its original store.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub id: String,
    pub kind: EvidenceKind,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    ProjectProfile,
    Task,
    Document,
    Candidate,
    ExternalReference,
}

/// Atomic fact: a named subject has a typed attribute value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub value: Value,
    pub evidence: Vec<String>,
}

/// Directed relation between two named project entities.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Relation {
    pub id: String,
    pub source: String,
    pub predicate: String,
    pub target: String,
    pub evidence: Vec<String>,
}

/// Explicit constraint expressed as a typed operator and scalar value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub operator: ConstraintOperator,
    pub value: Value,
    pub evidence: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintOperator {
    Equals,
    NotEquals,
    AtLeast,
    AtMost,
    Contains,
    Excludes,
}

/// Operator input; identity and checksum are computed locally after validation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMemoryDraft {
    pub project_ref: String,
    pub revision: u32,
    pub facts: Vec<Fact>,
    pub relations: Vec<Relation>,
    pub constraints: Vec<Constraint>,
    pub evidence: Vec<EvidenceRef>,
}
impl ProjectMemoryDraft {
    pub fn into_record(self) -> Result<ProjectMemoryRecord> {
        ProjectMemoryRecord::new(
            self.project_ref,
            self.revision,
            self.facts,
            self.relations,
            self.constraints,
            self.evidence,
        )
    }
}

/// A complete immutable memory revision for exactly one operator-published project.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMemoryRecord {
    pub format_version: u16,
    pub id: String,
    pub project_ref: String,
    pub revision: u32,
    pub facts: Vec<Fact>,
    pub relations: Vec<Relation>,
    pub constraints: Vec<Constraint>,
    pub evidence: Vec<EvidenceRef>,
    pub checksum: String,
}

/// Storage envelope binds the record to its exact immutable operator profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMemoryProvenance {
    pub publisher: String,
    pub project_profile_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoredProjectMemory {
    pub format_version: u16,
    pub record: ProjectMemoryRecord,
    pub provenance: ProjectMemoryProvenance,
}

impl StoredProjectMemory {
    pub(crate) fn new(record: ProjectMemoryRecord, profile_sha256: String) -> Result<Self> {
        let stored = Self {
            format_version: FORMAT_VERSION,
            record,
            provenance: ProjectMemoryProvenance {
                publisher: "human-cli".into(),
                project_profile_sha256: profile_sha256,
            },
        };
        stored.validate()?;
        Ok(stored)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.record.validate()?;
        validate_hash(&self.provenance.project_profile_sha256)?;
        if self.format_version != FORMAT_VERSION || self.provenance.publisher != "human-cli" {
            bail!("Invalid structured project memory provenance")
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct DigestFields<'a> {
    format_version: u16,
    id: &'a str,
    project_ref: &'a str,
    revision: u32,
    facts: &'a [Fact],
    relations: &'a [Relation],
    constraints: &'a [Constraint],
    evidence: &'a [EvidenceRef],
}

impl ProjectMemoryRecord {
    /// Construct a fresh immutable revision and checksum its complete typed body.
    pub fn new(
        project_ref: String,
        revision: u32,
        facts: Vec<Fact>,
        relations: Vec<Relation>,
        constraints: Vec<Constraint>,
        evidence: Vec<EvidenceRef>,
    ) -> Result<Self> {
        let mut record = Self {
            format_version: FORMAT_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            project_ref,
            revision,
            facts,
            relations,
            constraints,
            evidence,
            checksum: String::new(),
        };
        record.checksum = record.digest()?;
        record.validate()?;
        Ok(record)
    }

    fn digest(&self) -> Result<String> {
        Ok(crate::store::sha256(&serde_json::to_vec(&DigestFields {
            format_version: self.format_version,
            id: &self.id,
            project_ref: &self.project_ref,
            revision: self.revision,
            facts: &self.facts,
            relations: &self.relations,
            constraints: &self.constraints,
            evidence: &self.evidence,
        })?))
    }

    /// Validate strict schema, all claim-to-evidence links, and the full-record bound.
    pub fn validate(&self) -> Result<()> {
        if self.format_version != FORMAT_VERSION
            || uuid::Uuid::parse_str(&self.id).is_err()
            || self.revision == 0
            || self.project_ref.len() > 120
            || crate::projects::Selection::parse(&self.project_ref).is_err()
            || self.facts.len() > MAX_FACTS
            || self.relations.len() > MAX_RELATIONS
            || self.constraints.len() > MAX_CONSTRAINTS
            || self.evidence.is_empty()
            || self.evidence.len() > MAX_EVIDENCE
            || self.checksum != self.digest()?
        {
            bail!("Project memory identity, schema, quota, or checksum is invalid")
        }

        let mut evidence_ids = std::collections::BTreeSet::new();
        for evidence in &self.evidence {
            validate_identifier(&evidence.id, "evidence identity", 128)?;
            validate_hash(&evidence.sha256)?;
            if !evidence_ids.insert(evidence.id.as_str()) {
                bail!("Duplicate project memory evidence identity")
            }
        }

        let mut record_ids = std::collections::BTreeSet::new();
        for fact in &self.facts {
            validate_identifier(&fact.id, "fact identity", 64)?;
            validate_identifier(&fact.subject, "fact subject", 96)?;
            validate_identifier(&fact.predicate, "fact predicate", 64)?;
            validate_scalar(&fact.value)?;
            validate_links(&fact.evidence, &evidence_ids)?;
            if !record_ids.insert(fact.id.as_str()) {
                bail!("Duplicate project memory record identity")
            }
        }
        for relation in &self.relations {
            validate_identifier(&relation.id, "relation identity", 64)?;
            validate_identifier(&relation.source, "relation source", 96)?;
            validate_identifier(&relation.predicate, "relation predicate", 64)?;
            validate_identifier(&relation.target, "relation target", 96)?;
            validate_links(&relation.evidence, &evidence_ids)?;
            if !record_ids.insert(relation.id.as_str()) {
                bail!("Duplicate project memory record identity")
            }
        }
        for constraint in &self.constraints {
            validate_identifier(&constraint.id, "constraint identity", 64)?;
            validate_identifier(&constraint.subject, "constraint subject", 96)?;
            validate_identifier(&constraint.predicate, "constraint predicate", 64)?;
            validate_scalar(&constraint.value)?;
            validate_links(&constraint.evidence, &evidence_ids)?;
            if !record_ids.insert(constraint.id.as_str()) {
                bail!("Duplicate project memory record identity")
            }
        }

        if self.encode_unchecked()?.len() > MAX_RECORD_BYTES {
            bail!("Structured project memory exceeds its encoded byte limit")
        }
        Ok(())
    }

    /// Serialize validated records in a deterministic field order for immutable storage.
    pub fn encode(&self) -> Result<String> {
        self.validate()?;
        self.encode_unchecked()
    }

    fn encode_unchecked(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// Decode storage bytes and recheck schema, evidence links, bound, and checksum.
    pub fn decode(raw: &str) -> Result<Self> {
        if raw.len() > MAX_RECORD_BYTES {
            bail!("Structured project memory exceeds its encoded byte limit")
        }
        let record: Self = serde_json::from_str(raw)?;
        record.validate()?;
        if record.encode_unchecked()? != raw {
            bail!("Structured project memory is not in canonical encoding")
        }
        Ok(record)
    }
}

fn validate_identifier(value: &str, label: &str, maximum: usize) -> Result<()> {
    if value.is_empty()
        || value.len() > maximum
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/@-".contains(&b))
    {
        bail!("Invalid {label}")
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        bail!("Evidence digest must be lowercase SHA-256")
    }
    Ok(())
}

fn validate_scalar(value: &Value) -> Result<()> {
    match value {
        Value::String(text)
            if !text.trim().is_empty()
                && text.len() <= 256
                && !text.chars().any(char::is_control) =>
        {
            Ok(())
        }
        Value::Bool(_) | Value::Number(_) => Ok(()),
        _ => bail!("Project memory values must be bounded strings, numbers, or booleans"),
    }
}

fn validate_links(links: &[String], known: &std::collections::BTreeSet<&str>) -> Result<()> {
    if links.is_empty() || links.len() > 8 {
        bail!("Every project memory claim needs one to eight evidence links")
    }
    let mut unique = std::collections::BTreeSet::new();
    for link in links {
        validate_identifier(link, "evidence link", 128)?;
        if !known.contains(link.as_str()) || !unique.insert(link.as_str()) {
            bail!("Project memory claim has a missing or repeated evidence link")
        }
    }
    Ok(())
}
