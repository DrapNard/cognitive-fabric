//! Bounded local document inventory; draft metadata never asserts review.
use crate::{security, store::sha256};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::Path;

pub struct Draft {
    manifest: Value,
    report: Value,
}

/// Discover local text files only. Provenance/license labels are operator
/// assertions. No source text, inferred test split or permission is published.
pub fn discover(root: &Path, source: &str, license: &str) -> Result<Draft> {
    security::authorize("human-cli", "candidate.prepare")?;
    security::validate_provenance(source, license)?;
    #[cfg(target_os = "linux")]
    {
        discover_linux(root, source, license)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (root, source, license);
        bail!("Descriptor-anchored source discovery requires Linux procfs")
    }
}

#[cfg(target_os = "linux")]
fn discover_linux(root: &Path, source: &str, license: &str) -> Result<Draft> {
    use std::{
        collections::{BTreeMap, BTreeSet},
        ffi::CString,
        fs::{self, File},
        io::Read,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::OpenOptionsExt,
        },
        time::{Duration, Instant},
    };
    let started = Instant::now();
    let root = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)?;
    let mut pending = vec![(root, String::new(), 0usize)];
    let mut records = BTreeMap::new();
    let mut identities = BTreeSet::new();
    let mut inspected = 0usize;
    let mut hidden = 0usize;
    let mut bytes_read = 0usize;
    while let Some((directory, prefix, depth)) = pending.pop() {
        if started.elapsed() > Duration::from_secs(5) {
            bail!("Source discovery deadline exhausted")
        }
        // procfs resolves this held descriptor, not a mutable user pathname.
        for entry in fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
            let entry = entry?;
            inspected += 1;
            if inspected > 256 || started.elapsed() > Duration::from_secs(5) {
                bail!("Source discovery entry/deadline bound exhausted")
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Source discovery requires UTF-8 paths"))?;
            if name.starts_with('.') {
                hidden += 1;
                continue;
            }
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if relative.len() > 240 {
                bail!("Discovered source path exceeds bounds")
            }
            let c_name = CString::new(name)?;
            let fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    c_name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let file = unsafe { File::from_raw_fd(fd) };
            let metadata = file.metadata()?;
            if metadata.is_dir() {
                if depth >= 7 {
                    bail!("Source discovery depth bound exhausted")
                }
                pending.push((file, relative, depth + 1));
            } else if metadata.is_file() {
                if !matches!(
                    Path::new(&relative).extension().and_then(|s| s.to_str()),
                    Some("md" | "txt")
                ) {
                    continue;
                }
                if metadata.len() > 64_000 || records.len() >= 32 {
                    bail!("Source discovery document size/count exhausted")
                }
                let mut bytes = Vec::new();
                file.take(64_001).read_to_end(&mut bytes)?;
                bytes_read += bytes.len();
                if bytes.is_empty() || bytes.len() > 64_000 || bytes_read > 512_000 {
                    bail!("Source discovery byte bound exhausted")
                }
                super::preparation::normalize(&bytes)?;
                let provenance = format!("{source}#{relative}");
                security::validate_provenance(&provenance, license)?;
                let id = format!("src.{}", &sha256(relative.as_bytes())[..32]);
                if !identities.insert(id.clone()) {
                    bail!("Discovered source identity collision")
                }
                records.insert(
                    relative.clone(),
                    json!({"id":id,"path":relative,"sha256":sha256(&bytes),
                    "source":provenance,"license":license,"split":"reference"}),
                );
            } else {
                bail!("Source discovery refuses nonregular entries")
            }
        }
    }
    if records.is_empty() || started.elapsed() > Duration::from_secs(5) {
        bail!("Source discovery is empty or expired")
    }
    let source_count = records.len();
    let manifest = json!({"format_version":1,"reviewer":"","sources_reviewed":false,"sources":records.into_values().collect::<Vec<_>>()});
    let raw = serde_json::to_vec_pretty(&manifest)?;
    if raw.len() > 32_000 {
        bail!("Discovered manifest exceeds preparation input bound")
    }
    let report = json!({"format_version":1,"manifest_sha256":sha256(&raw),"entries_inspected":inspected,
        "hidden_entries_skipped":hidden,"input_bytes":bytes_read,"sources":source_count,
        "discovery_policy":"local-visible-md-txt/v1","sources_reviewed":false,"split_assignment":"reference-only-draft",
        "candidate_created":false,"permissions_granted":false,"independent_review_proven":false,"network_used":false});
    Ok(Draft { manifest, report })
}

impl Draft {
    pub fn write(&self, output: &Path) -> Result<Value> {
        super::preparation::write_bundle(
            output,
            vec![
                ("sources.json", serde_json::to_vec_pretty(&self.manifest)?),
                ("discovery.json", serde_json::to_vec_pretty(&self.report)?),
            ],
        )?;
        Ok(self.report.clone())
    }
}
