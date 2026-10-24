//! Hash-pinned prepared RAG imports with durable non-evaluation provenance.
use super::preparation;
use crate::{
    config::Config,
    security,
    store::{sha256, Candidate, Store},
};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CStr,
    fs::{self, File},
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::OpenOptionsExt,
    },
    path::{Component, Path},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Split {
    Reference,
    Validation,
    Test,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    path: String,
    source: String,
    license: String,
    split: Split,
    original_sha256: String,
    normalized_sha256: String,
    normalized_bytes: usize,
    duplicate_of: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    format_version: u32,
    manifest_sha256: String,
    reviewer: String,
    sources_reviewed_by_operator: bool,
    independent_review_proven: bool,
    untrusted_data: bool,
    permissions_granted: bool,
    candidate_created: bool,
    model_promoted: bool,
    artifact_sha256: String,
    artifact_bytes: usize,
    held_out_documents: usize,
    held_out_sha256: String,
    input_bytes: usize,
    sources: Vec<Record>,
    cleaning: String,
    deduplication: String,
    split_overlap_check: String,
    #[serde(default)]
    acquisition: Option<super::acquisition::lineage::Evidence>,
}
fn digest(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read_file(directory: &File, name: &CStr, maximum: usize) -> Result<Vec<u8>> {
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    if !file.metadata()?.is_file() || file.metadata()?.len() > maximum as u64 {
        bail!("Prepared bundle entry is not a bounded regular file")
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > maximum {
        bail!("Prepared bundle entry is empty or oversized")
    }
    Ok(bytes)
}
fn report(raw: &[u8]) -> Result<Report> {
    let value = crate::strict_json::value(raw, 65_536)?;
    let has_acquisition = value.get("acquisition").is_some();
    let report: Report = serde_json::from_value(value)?;
    if !matches!(report.format_version, 2 | 3)
        || (report.format_version == 2 && has_acquisition)
        || (report.format_version == 3 && report.acquisition.is_none())
        || !digest(&report.manifest_sha256)
        || report.reviewer.trim().is_empty()
        || report.reviewer.len() > 100
        || !report.sources_reviewed_by_operator
        || report.independent_review_proven
        || !report.untrusted_data
        || report.permissions_granted
        || report.candidate_created
        || report.model_promoted
        || !digest(&report.artifact_sha256)
        || !digest(&report.held_out_sha256)
        || report.sources.len() < 3
        || report.sources.len() > 32
        || report.artifact_bytes == 0
        || report.artifact_bytes > 256_000
        || report.input_bytes == 0
        || report.input_bytes > 512_000
        || report.held_out_documents < 2
        || report.held_out_documents > 32
        || report.cleaning != "utf8-newlines-trailing-whitespace/v1"
        || report.deduplication != "normalized-document-sha256/v1"
        || report.split_overlap_check
            != "exact-normalized-documents-and-paragraphs-at-least-32-bytes/v1"
    {
        bail!("Prepared source report version, identity, bounds or non-authority flags are invalid")
    }
    if let Some(evidence) = &report.acquisition {
        let origins = evidence
            .validate()?
            .into_iter()
            .map(|origin| (origin.id.clone(), origin))
            .collect::<BTreeMap<_, _>>();
        let mut ids = BTreeSet::new();
        let mut original_bytes = 0usize;
        for source in &report.sources {
            let origin = origins
                .get(&source.id)
                .ok_or_else(|| anyhow::anyhow!("Prepared source has no acquired origin"))?;
            if !ids.insert(&source.id)
                || source.path != origin.path
                || source.original_sha256 != origin.original_sha256
                || source.source != origin.source
                || source.license != origin.license
            {
                bail!("Prepared source acquisition lineage differs")
            }
            original_bytes = original_bytes
                .checked_add(origin.bytes)
                .ok_or_else(|| anyhow::anyhow!("Acquisition lineage byte count overflow"))?;
        }
        if original_bytes != report.input_bytes {
            bail!("Prepared source input bytes differ from acquisition lineage")
        }
    }
    Ok(report)
}

/// Recheck metadata at the transactional storage boundary, using the same
/// version/lineage rules as preparation import and durable inspection.
pub(crate) fn validate_report_snapshot(raw: &[u8], checksum: &str) -> Result<()> {
    if report(raw)?.artifact_sha256 != checksum {
        bail!("Prepared lineage cannot change artifact identity")
    }
    Ok(())
}

/// Import a prepared artifact snapshot; original document metadata remains
/// operator-asserted. Source reports never count as passing evaluations.
pub fn create(
    cfg: &Config,
    store: &Store,
    skill: &str,
    bundle: &Path,
    expected_report_sha256: &str,
    license: &str,
) -> Result<Candidate> {
    security::authorize("human-cli", "candidate.create")?;
    let started = Instant::now();
    let directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(bundle)?;
    let raw = read_file(&directory, c"report.json", 65_536)?;
    if !digest(expected_report_sha256) || sha256(&raw) != expected_report_sha256 {
        bail!("Prepared report hash mismatch")
    }
    let report = report(&raw)?;
    let artifact = read_file(&directory, c"artifact.md", 256_000)?;
    let held = read_file(&directory, c"held-out.json", 1_100_000)?;
    if artifact.len() != report.artifact_bytes
        || sha256(&artifact) != report.artifact_sha256
        || sha256(&held) != report.held_out_sha256
    {
        bail!("Prepared artifact or held-out snapshot hash changed")
    }
    let text = std::str::from_utf8(&artifact)?;
    let mut held: BTreeMap<String, String> =
        serde_json::from_value(crate::strict_json::value(&held, 1_100_000)?)?;
    if held.len() != report.held_out_documents {
        bail!("Prepared held-out coverage changed")
    }
    let mut ids = BTreeSet::new();
    let mut splits = BTreeSet::new();
    let mut bodies = BTreeMap::<String, (Split, String, String)>::new();
    let mut hashes = BTreeMap::<String, (String, Split)>::new();
    let mut paragraphs = BTreeMap::new();
    let mut cursor = 0usize;
    let mut normalized_total = 0usize;
    for source in &report.sources {
        if started.elapsed() > Duration::from_secs(5) {
            bail!("Prepared import deadline exhausted")
        }
        security::validate_provenance(&source.source, &source.license)?;
        let path = Path::new(&source.path);
        if source.id.len() > 64
            || !crate::registry::identifier(&source.id)
            || !ids.insert(source.id.clone())
            || source.path.len() > 240
            || path.components().count() > 8
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("md" | "txt")
            )
            || !digest(&source.original_sha256)
            || !digest(&source.normalized_sha256)
            || source.normalized_bytes == 0
            || source.normalized_bytes > 64_000
        {
            bail!("Invalid prepared source metadata")
        }
        normalized_total += source.normalized_bytes;
        if normalized_total > report.input_bytes {
            bail!("Prepared source byte lineage exceeds original input")
        }
        splits.insert(match source.split {
            Split::Reference => 0,
            Split::Validation => 1,
            Split::Test => 2,
        });
        let body = if let Some(previous) = &source.duplicate_of {
            let (split, hash, body) = bodies
                .get(previous)
                .ok_or_else(|| anyhow::anyhow!("Prepared duplicate has no earlier source"))?;
            if split != &source.split
                || hash != &source.normalized_sha256
                || body.len() != source.normalized_bytes
                || hashes.get(hash).map(|(id, _)| id) != Some(previous)
            {
                bail!("Prepared duplicate lineage differs")
            }
            body.clone()
        } else {
            if hashes.contains_key(&source.normalized_sha256) {
                bail!("Prepared duplicate was not declared")
            }
            let body = match source.split {
                Split::Reference => {
                    let prefix = format!("## Source {}\n\n", source.id);
                    if !text
                        .get(cursor..)
                        .is_some_and(|rest| rest.starts_with(&prefix))
                    {
                        bail!("Prepared reference framing differs")
                    }
                    cursor += prefix.len();
                    let end = cursor + source.normalized_bytes;
                    let body = text
                        .get(cursor..end)
                        .ok_or_else(|| anyhow::anyhow!("Prepared reference byte boundary differs"))?
                        .to_owned();
                    if text.get(end..end + 2) != Some("\n\n") {
                        bail!("Prepared reference terminator differs")
                    }
                    cursor = end + 2;
                    body
                }
                _ => held
                    .remove(&source.id)
                    .ok_or_else(|| anyhow::anyhow!("Missing prepared held-out source"))?,
            };
            hashes.insert(
                source.normalized_sha256.clone(),
                (source.id.clone(), source.split),
            );
            body
        };
        if body.len() != source.normalized_bytes
            || sha256(body.as_bytes()) != source.normalized_sha256
            || preparation::normalize(body.as_bytes())? != body
        {
            bail!("Prepared normalized source hash or content differs")
        }
        for paragraph in body.split("\n\n").map(str::trim).filter(|p| p.len() >= 32) {
            let hash = sha256(paragraph.as_bytes());
            if paragraphs
                .get(&hash)
                .is_some_and(|split| *split != source.split)
            {
                bail!("Prepared source splits overlap")
            }
            paragraphs.insert(hash, source.split);
        }
        bodies.insert(
            source.id.clone(),
            (source.split, source.normalized_sha256.clone(), body),
        );
    }
    if cursor != text.len()
        || !held.is_empty()
        || splits.len() != 3
        || started.elapsed() > Duration::from_secs(5)
    {
        bail!("Prepared artifact coverage or deadline differs")
    }
    let source = format!("prepared-manifest-sha256:{}", report.manifest_sha256);
    super::create_import(
        cfg,
        store,
        (skill, "rag"),
        (
            super::ImportArtifact::Prepared(&artifact, std::str::from_utf8(&raw)?),
            &source,
            license,
        ),
        None,
    )
}

/// Read durable candidate association metadata without treating it as review.
pub fn provenance(store: &Store, id: &str) -> Result<Value> {
    security::authorize("human-cli", "candidate.prepare")?;
    uuid::Uuid::parse_str(id)?;
    let candidate = store.candidate(id)?;
    let db = store.lock()?;
    let started = Instant::now();
    db.progress_handler(
        1000,
        Some(move || started.elapsed() > Duration::from_secs(5)),
    );
    let result = (|| -> Result<Value> {
        let mut statement=db.prepare("SELECT task_id,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE kind='candidate_preparation' AND source='human-cli' AND CASE WHEN length(CAST(payload AS BLOB))<=262144 AND json_valid(payload) THEN json_extract(payload,'$.candidate_id')=?1 ELSE 1 END ORDER BY id LIMIT 2")?;
        let rows = statement
            .query_map([id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.is_empty() {
            return Ok(
                json!({"candidate_id":id,"prepared_provenance":null,"evaluation_authority":false}),
            );
        }
        if rows.len() != 1 {
            bail!("Ambiguous prepared candidate provenance")
        }
        uuid::Uuid::parse_str(&rows[0].0)?;
        let payload = crate::strict_json::value(rows[0].1.as_bytes(), 262144)?;
        let raw = payload["report_json"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing prepared report snapshot"))?;
        if payload["format_version"] != 1
            || payload["candidate_id"] != id
            || payload["candidate_checksum"] != candidate.checksum
            || payload["report_sha256"] != sha256(raw.as_bytes())
            || payload["permissions_granted"] != false
            || payload["evaluation_authority"] != false
            || payload["automatic_promotion"] != false
            || report(raw.as_bytes())?.artifact_sha256 != candidate.checksum
        {
            bail!("Prepared candidate provenance identity differs")
        }
        Ok(
            json!({"candidate_id":id,"provenance_task_id":rows[0].0,"prepared_provenance":crate::strict_json::value(raw.as_bytes(),65_536)?,
            "report_sha256":payload["report_sha256"],"evaluation_authority":false,"permissions_granted":false}),
        )
    })();
    db.progress_handler(0, None::<fn() -> bool>);
    result
}
