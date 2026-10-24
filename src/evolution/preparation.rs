//! Deterministic preparation of explicitly reviewed local sources; no crawler.
use crate::{security, store::sha256};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{DirBuilderExt, OpenOptionsExt},
    },
    path::{Component, Path},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    reviewer: String,
    sources_reviewed: bool,
    sources: Vec<Source>,
    #[serde(default)]
    acquisition: Option<crate::evolution::acquisition::lineage::Binding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: String,
    path: String,
    sha256: String,
    source: String,
    license: String,
    split: Split,
}
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Split {
    Reference,
    Validation,
    Test,
}

pub struct Prepared {
    artifact: String,
    held_out: BTreeMap<String, String>,
    report: Value,
}

fn read_beneath(root: &File, path: &str) -> Result<Vec<u8>> {
    if path.len() > 240
        || !matches!(
            Path::new(path).extension().and_then(|s| s.to_str()),
            Some("md" | "txt")
        )
    {
        bail!("Source path must name a bounded local Markdown or text document")
    }
    let components = Path::new(path).components().collect::<Vec<_>>();
    if components.is_empty()
        || components.len() > 8
        || components
            .iter()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("Source paths must stay beneath the selected root")
    }
    let mut directory = root.try_clone()?;
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            unreachable!()
        };
        use std::os::unix::ffi::OsStrExt;
        let name = CString::new(name.as_bytes())?;
        let last = index + 1 == components.len();
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if last { 0 } else { libc::O_DIRECTORY };
        // Each lookup is relative to a retained directory descriptor. Renaming
        // or replacing path components cannot redirect a later lookup outside it.
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        if last {
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.len() > 64_000 {
                bail!("Source must be a regular file of at most 64 KB")
            }
            let mut bytes = Vec::new();
            file.take(64_001).read_to_end(&mut bytes)?;
            if bytes.is_empty() || bytes.len() > 64_000 {
                bail!("Source is empty or oversized")
            }
            return Ok(bytes);
        }
        directory = file;
    }
    bail!("Missing source file")
}

fn read_fixed_beneath(root: &File, name: &str, maximum: usize) -> Result<Vec<u8>> {
    if !matches!(name, "acquisition-plan.json" | "acquisition.json") {
        bail!("Unsupported acquisition evidence filename")
    }
    let name = CString::new(name)?;
    let fd = unsafe {
        libc::openat(
            root.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        bail!("Acquisition evidence must be a regular file no larger than 32 KB")
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > maximum {
        bail!("Acquisition evidence is empty or oversized")
    }
    Ok(bytes)
}

pub(super) fn normalize(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes)?
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        bail!("Source contains unsupported control characters")
    }
    let text = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim().to_owned();
    if text.is_empty() {
        bail!("Normalized source is empty")
    }
    Ok(text)
}

/// Hash pins apply to original bytes. Cleaning and overlap checks cannot attest
/// licensing, relevance, semantic independence, poisoning resistance or safety.
pub fn prepare(root: &Path, manifest: &Path, expected_sha256: &str) -> Result<Prepared> {
    security::authorize("human-cli", "candidate.prepare")?;
    let started = Instant::now();
    let raw = security::read_regular_bytes(manifest, 32_000)?;
    if expected_sha256.len() != 64 || sha256(&raw) != expected_sha256 {
        bail!("Source manifest hash mismatch")
    }
    let parsed: Manifest = serde_json::from_value(crate::strict_json::value(&raw, 32_000)?)?;
    if parsed.format_version != 1
        || !parsed.sources_reviewed
        || parsed.reviewer.trim().is_empty()
        || parsed.reviewer.len() > 100
        || parsed.sources.len() > 32
        || parsed.sources.len() < 3
    {
        bail!("Source manifest lacks bounded explicit review or split coverage")
    }
    let root = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)?;
    let acquisition_evidence = parsed
        .acquisition
        .as_ref()
        .map(|binding| {
            let plan = read_fixed_beneath(&root, "acquisition-plan.json", 32_000)?;
            let acquisition = read_fixed_beneath(&root, "acquisition.json", 32_000)?;
            let evidence = crate::evolution::acquisition::lineage::validate_binding(
                binding,
                &plan,
                &acquisition,
            )?;
            let origins = evidence.validate()?;
            let evidence_value = json!({
                "format_version": evidence.format_version,
                "plan_sha256": evidence.plan_sha256,
                "report_sha256": evidence.report_sha256,
                "plan_json": evidence.plan_json,
                "report_json": evidence.report_json
            });
            Ok::<_, anyhow::Error>((evidence_value, origins))
        })
        .transpose()?;
    let mut ids = BTreeSet::new();
    let mut documents = BTreeMap::<String, (String, Split)>::new();
    let mut paragraphs = BTreeMap::<String, Split>::new();
    let mut artifact = String::new();
    let mut held_out = BTreeMap::new();
    let mut records = Vec::new();
    let mut seen_splits = BTreeSet::new();
    let mut total = 0usize;
    for source in parsed.sources {
        if started.elapsed() > Duration::from_secs(5) {
            bail!("Source preparation deadline exhausted")
        }
        if source.id.len() > 64
            || !crate::registry::identifier(&source.id)
            || !ids.insert(source.id.clone())
        {
            bail!("Invalid or duplicate source identity")
        }
        security::validate_provenance(&source.source, &source.license)?;
        let bytes = read_beneath(&root, &source.path)?;
        if let Some((_, origins)) = &acquisition_evidence {
            let origin = origins
                .iter()
                .find(|origin| origin.id == source.id)
                .ok_or_else(|| {
                    anyhow::anyhow!("Selected source is absent from acquisition evidence")
                })?;
            if source.path != format!("{}.txt", origin.id)
                || source.sha256 != origin.original_sha256
                || source.source != origin.source
                || source.license != origin.license
                || bytes.len() != origin.bytes
            {
                bail!("Selected source does not match its acquired origin")
            }
        }
        total += bytes.len();
        if total > 512_000 || sha256(&bytes) != source.sha256 {
            bail!("Source hash or total input bound failed")
        }
        let cleaned = normalize(&bytes)?;
        let hash = sha256(cleaned.as_bytes());
        let duplicate = documents.get(&hash).map(|(id, split)| (id.clone(), *split));
        if duplicate
            .as_ref()
            .is_some_and(|(_, split)| *split != source.split)
        {
            bail!("Duplicate content crosses source splits")
        }
        for paragraph in cleaned
            .split("\n\n")
            .map(str::trim)
            .filter(|p| p.len() >= 32)
        {
            let hash = sha256(paragraph.as_bytes());
            if paragraphs
                .get(&hash)
                .is_some_and(|split| *split != source.split)
            {
                bail!("Exact paragraph overlap crosses source splits")
            }
            paragraphs.insert(hash, source.split);
        }
        seen_splits.insert(match source.split {
            Split::Reference => 0,
            Split::Validation => 1,
            Split::Test => 2,
        });
        if duplicate.is_none() {
            documents.insert(hash.clone(), (source.id.clone(), source.split));
            match source.split {
                Split::Reference => {
                    artifact.push_str(&format!("## Source {}\n\n{}\n\n", source.id, cleaned));
                    if artifact.len() > 256_000 {
                        bail!("Prepared RAG artifact exceeds 256 KB")
                    }
                }
                _ => {
                    held_out.insert(source.id.clone(), cleaned.clone());
                }
            }
        }
        records.push(json!({"id":source.id,"path":source.path,"source":source.source,"license":source.license,
            "split":source.split,"original_sha256":source.sha256,"normalized_sha256":hash,"normalized_bytes":cleaned.len(),
            "duplicate_of":duplicate.map(|(id,_)|id)}));
    }
    if seen_splits.len() != 3 || artifact.is_empty() {
        bail!("Reference, validation and test sources are all required")
    }
    if started.elapsed() > Duration::from_secs(5) {
        bail!("Source preparation deadline exhausted")
    }
    let mut report = json!({"format_version":2,"manifest_sha256":expected_sha256,"reviewer":parsed.reviewer,
        "sources_reviewed_by_operator":true,"independent_review_proven":false,"untrusted_data":true,
        "permissions_granted":false,"candidate_created":false,"model_promoted":false,
        "artifact_sha256":sha256(artifact.as_bytes()),"artifact_bytes":artifact.len(),
        "held_out_documents":held_out.len(),"held_out_sha256":sha256(&serde_json::to_vec_pretty(&held_out)?),"input_bytes":total,"sources":records,
        "cleaning":"utf8-newlines-trailing-whitespace/v1","deduplication":"normalized-document-sha256/v1",
        "split_overlap_check":"exact-normalized-documents-and-paragraphs-at-least-32-bytes/v1"});
    if let Some((evidence, _)) = acquisition_evidence {
        report["format_version"] = json!(3);
        report["acquisition"] = evidence;
    }
    if serde_json::to_vec_pretty(&report)?.len() > 65_536 {
        bail!("Preparation report exceeds the 64 KB publication limit")
    }
    Ok(Prepared {
        artifact,
        held_out,
        report,
    })
}

impl Prepared {
    /// Create a new private bundle; never overwrite existing operator files.
    /// The artifact excludes validation/test text, which is stored separately.
    pub fn write(&self, output: &Path) -> Result<Value> {
        let report_bytes = serde_json::to_vec_pretty(&self.report)?;
        if report_bytes.len() > 65_536 {
            bail!("Preparation report exceeds the 64 KB publication limit")
        }
        let payloads = vec![
            ("artifact.md", self.artifact.as_bytes().to_vec()),
            ("held-out.json", serde_json::to_vec_pretty(&self.held_out)?),
            ("report.json", report_bytes),
        ];
        write_bundle(output, payloads)?;
        Ok(self.report.clone())
    }
}

pub(super) fn write_bundle(output: &Path, payloads: Vec<(&str, Vec<u8>)>) -> Result<()> {
    fs::DirBuilder::new()
        .mode(0o700)
        .create(output)
        .context("Preparation output directory must not already exist")?;
    let mut created = Vec::new();
    let result = (|| -> Result<()> {
        for (name, bytes) in payloads {
            let path = output.join(name);
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)?;
            created.push(path);
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        File::open(output)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        for path in created {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(output);
    }
    result
}
