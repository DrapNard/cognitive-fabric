//! Operator-selected candidate artifacts are copied into read-only content-addressed quarantine.
use crate::{
    config::Config,
    security,
    store::{sha256, Candidate, Store},
};
use anyhow::{bail, Result};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub fn create(
    cfg: &Config,
    store: &Store,
    skill: &str,
    kind: &str,
    artifact: &Path,
    source: &str,
    license: &str,
) -> Result<Candidate> {
    security::authorize("human-cli", "candidate.create")?;
    security::validate_provenance(source, license)?;
    if !store.skills()?.iter().any(|entry| entry.id == skill) {
        bail!("Unknown skill")
    }
    let (bytes, extension, maximum) = match kind {
        "rag" => (
            security::read_reviewed_document(artifact)?.into_bytes(),
            "md",
            256_000,
        ),
        "lora" => (
            security::read_regular_bytes(artifact, 64_000_000)?,
            "safetensors",
            64_000_000,
        ),
        _ => bail!("Unsupported candidate kind"),
    };
    if bytes.is_empty() {
        bail!("Candidate artifact is empty")
    }
    cfg.prepare()?;
    // Serialize app imports across processes while checking the shared disk ceiling.
    let mut database = store.lock()?;
    let reservation =
        database.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let quarantine = cfg.home.join("artifacts");
    match fs::symlink_metadata(&quarantine) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
        Ok(_) => bail!("Quarantine must be a real directory"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new().mode(0o700).create(&quarantine)?;
        }
        Err(error) => return Err(error.into()),
    }
    fs::set_permissions(&quarantine, fs::Permissions::from_mode(0o700))?;
    let checksum = sha256(&bytes);
    let target = quarantine.join(format!("{checksum}.{extension}"));
    let exists = fs::symlink_metadata(&target).is_ok();
    let mut total = 0u64;
    let mut count = 0usize;
    for entry in fs::read_dir(&quarantine)? {
        count += 1;
        if count > 4096 {
            bail!("Quarantine entry capacity exhausted")
        }
        let metadata = fs::symlink_metadata(entry?.path())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            bail!("Invalid quarantine entry")
        }
        total = total.saturating_add(metadata.len());
    }
    if total.saturating_add(if exists { 0 } else { bytes.len() as u64 }) > cfg.max_artifact_bytes
        || (!exists && count >= 4096)
    {
        bail!("Quarantine storage budget exhausted")
    }
    let temporary = quarantine.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        if exists {
            if sha256(&security::read_regular_bytes(&target, maximum)?) != checksum {
                bail!("Quarantined artifact failed integrity verification")
            }
            return Ok(());
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        output.set_permissions(fs::Permissions::from_mode(0o400))?;
        output.sync_all()?;
        match fs::hard_link(&temporary, &target) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if sha256(&security::read_regular_bytes(&target, maximum)?) != checksum {
                    bail!("Quarantined artifact failed integrity verification")
                }
            }
            Err(error) => return Err(error.into()),
        }
        fs::File::open(&quarantine)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result?;
    reservation.commit()?;
    drop(database);
    let canonical = target.canonicalize()?;
    store.create_candidate(
        skill,
        kind,
        canonical
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Non-UTF8 quarantine path"))?,
        &checksum,
        source,
        license,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    #[test]
    fn quota_and_modified_quarantine_block_new_candidates() {
        let root = std::env::temp_dir().join(format!("fabric-quarantine-{}", uuid::Uuid::new_v4()));
        let mut cfg = Config::load(Some(root.clone()), Some("demo".into())).unwrap();
        cfg.prepare().unwrap();
        let store = Store::open(&cfg.db()).unwrap();
        store::bootstrap(&store).unwrap();
        let source = root.join("reviewed.md");
        fs::write(&source, "Rust reviewed snapshot").unwrap();
        cfg.max_artifact_bytes = 1;
        assert!(create(&cfg, &store, "rust.s1", "rag", &source, "fixture", "CC0").is_err());
        assert!(store.candidates().unwrap().is_empty());
        cfg.max_artifact_bytes = 1000;
        let candidate = create(&cfg, &store, "rust.s1", "rag", &source, "fixture", "CC0").unwrap();
        fs::set_permissions(&candidate.artifact, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&candidate.artifact, "tampered").unwrap();
        assert!(create(&cfg, &store, "rust.s1", "rag", &source, "fixture", "CC0").is_err());
        assert_eq!(store.candidates().unwrap().len(), 1);
        assert_eq!(
            crate::evaluation::evaluate(&store, &candidate.id, "integrity", None).unwrap()
                ["checksum_matches"],
            false
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_imports_deduplicate_files_and_allocate_unique_versions() {
        let root = std::env::temp_dir().join(format!("fabric-quarantine-{}", uuid::Uuid::new_v4()));
        let cfg = Config::load(Some(root.clone()), Some("demo".into())).unwrap();
        cfg.prepare().unwrap();
        let store = std::sync::Arc::new(Store::open(&cfg.db()).unwrap());
        store::bootstrap(&store).unwrap();
        let source = root.join("reviewed.md");
        fs::write(&source, "Rust reviewed snapshot").unwrap();
        let workers = (0..4)
            .map(|_| {
                let cfg = cfg.clone();
                let store = store.clone();
                let source = source.clone();
                std::thread::spawn(move || {
                    create(&cfg, &store, "rust.s1", "rag", &source, "fixture", "CC0")
                        .unwrap()
                        .version
                })
            })
            .collect::<Vec<_>>();
        let mut versions = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        versions.sort();
        assert_eq!(versions, vec![1, 2, 3, 4]);
        assert_eq!(fs::read_dir(root.join("artifacts")).unwrap().count(), 1);
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quarantine_survives_source_mutation_and_reuses_verified_content() {
        let root = std::env::temp_dir().join(format!("fabric-quarantine-{}", uuid::Uuid::new_v4()));
        let cfg = Config::load(Some(root.clone()), Some("demo".into())).unwrap();
        cfg.prepare().unwrap();
        let store = Store::open(&cfg.db()).unwrap();
        store::bootstrap(&store).unwrap();
        let source = root.join("reviewed.md");
        fs::write(&source, "Rust reviewed snapshot").unwrap();
        let first = create(&cfg, &store, "rust.s1", "rag", &source, "fixture", "CC0").unwrap();
        let second = create(&cfg, &store, "rust.s1", "rag", &source, "fixture", "CC0").unwrap();
        assert_eq!(first.artifact, second.artifact);
        assert_eq!(second.version, 2);
        fs::write(&source, "Changed source").unwrap();
        assert_eq!(
            fs::read_to_string(&first.artifact).unwrap(),
            "Rust reviewed snapshot"
        );
        assert_eq!(
            fs::metadata(&first.artifact).unwrap().permissions().mode() & 0o777,
            0o400
        );
        assert_eq!(fs::read_dir(root.join("artifacts")).unwrap().count(), 1);
        crate::evaluation::evaluate(&store, &first.id, "integrity", None).unwrap();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quarantine_rejects_symlinked_directories() {
        let root = std::env::temp_dir().join(format!("fabric-quarantine-{}", uuid::Uuid::new_v4()));
        let cfg = Config::load(Some(root.clone()), Some("demo".into())).unwrap();
        cfg.prepare().unwrap();
        let store = Store::open(&cfg.db()).unwrap();
        store::bootstrap(&store).unwrap();
        let source = root.join("reviewed.md");
        fs::write(&source, "Rust reviewed snapshot").unwrap();
        std::os::unix::fs::symlink("/tmp", root.join("artifacts")).unwrap();
        assert!(create(&cfg, &store, "rust.s1", "rag", &source, "fixture", "CC0").is_err());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
