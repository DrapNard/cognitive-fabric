//! Restore an operator-reviewed database snapshot into a new workspace only.
use crate::{config::Config, security};
use anyhow::{bail, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Serialize)]
pub struct Restored {
    pub rag_artifacts_restored: Option<usize>,
    pub format_version: u16,
    pub restore_id: String,
    pub home: PathBuf,
    pub source_checksum: String,
    pub bytes: u64,
    pub schema_version: u32,
    pub running_tasks_not_replayed: u64,
    pub configuration_restored: bool,
    pub external_artifacts_restored: bool,
}

pub fn database(
    cfg: &Config,
    snapshot: &Path,
    checksum: &str,
    destination: &Path,
) -> Result<Restored> {
    database_with_configuration(cfg, snapshot, checksum, destination, false)
}

pub fn database_with_configuration(
    cfg: &Config,
    snapshot: &Path,
    checksum: &str,
    destination: &Path,
    restore_configuration: bool,
) -> Result<Restored> {
    database_with_options(
        cfg,
        snapshot,
        checksum,
        destination,
        restore_configuration,
        false,
    )
}
pub fn database_with_options(
    cfg: &Config,
    snapshot: &Path,
    checksum: &str,
    destination: &Path,
    restore_configuration: bool,
    restore_rag: bool,
) -> Result<Restored> {
    security::authorize("human-cli", "storage.restore")?;
    cfg.validate()?;
    if checksum.len() != 64
        || !checksum
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        bail!("Restore requires the saved lowercase SHA-256 checksum")
    }
    let mut source = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(snapshot)?;
    let original = source.metadata()?;
    if !original.is_file()
        || original.nlink() != 1
        || original.len() == 0
        || original.len() > cfg.max_database_bytes
    {
        bail!("Snapshot must be a bounded regular file with one link")
    }
    // Exclusive mkdir prevents any existing workspace (including empty ones) being replaced.
    fs::DirBuilder::new().mode(0o700).create(destination)?;
    let directory = fs::symlink_metadata(destination)?;
    let temporary = destination.join("restore.pending");
    let published = destination.join("fabric.sqlite3");
    let mut staging_identity = None;
    let result = (|| {
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)?;
        let metadata = target.metadata()?;
        staging_identity = Some((metadata.dev(), metadata.ino()));
        let started = Instant::now();
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            if started.elapsed() > Duration::from_secs(10) {
                bail!("Restore copy deadline exhausted")
            }
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            bytes += read as u64;
            if bytes > cfg.max_database_bytes || bytes > original.len() {
                bail!("Snapshot grew beyond restore bounds")
            }
            hash.update(&buffer[..read]);
            target.write_all(&buffer[..read])?;
        }
        let after = source.metadata()?;
        let actual = hex::encode(hash.finalize());
        if actual != checksum
            || bytes != original.len()
            || after.mtime_nsec() != original.mtime_nsec()
            || after.mtime() != original.mtime()
        {
            bail!("Snapshot checksum or source identity changed")
        }
        target.sync_all()?;
        let target_identity = target.metadata()?;
        let entry = fs::symlink_metadata(&temporary)?;
        if entry.ino() != target_identity.ino()
            || entry.dev() != target_identity.dev()
            || !entry.is_file()
            || entry.file_type().is_symlink()
            || entry.nlink() != 1
            || entry.len() != bytes
        {
            bail!("Restore staging file identity changed")
        }
        let db = Connection::open_with_flags(
            &temporary,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.pragma_update(None, "trusted_schema", false)?;
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(10)),
        );
        let journal: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        if journal != "delete" {
            bail!("Restore requires a standalone DELETE-journal backup, not a live WAL database")
        }
        let check: String = db.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if check != "ok" {
            bail!("Snapshot failed SQLite consistency checking")
        }
        let schema: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema != 6 {
            bail!("Restore requires the current schema version 6; older snapshots need a separately reviewed migration")
        }
        let running = db.query_row(
            "SELECT count(*) FROM tasks WHERE status='running'",
            [],
            |row| row.get::<_, u64>(0),
        )?;
        let mut artifact_limit = cfg.max_artifact_bytes;
        let configuration = if restore_configuration {
            let (version, body): (u16, String) = db.query_row(
                "SELECT format_version, body FROM backup_configuration WHERE id=1 AND length(CAST(body AS BLOB))<=64000",
                [], |row| Ok((row.get(0)?,row.get(1)?)),
            )?;
            if version != 1 {
                bail!("Unsupported backup configuration version")
            }
            let mut saved: Config = serde_json::from_str(&body)?;
            saved.home = destination.into();
            saved.validate()?;
            artifact_limit = artifact_limit.min(saved.max_artifact_bytes);
            Some(serde_json::to_vec_pretty(&saved)?)
        } else {
            None
        };
        drop(db);
        if started.elapsed() > Duration::from_secs(10) {
            bail!("Restore validation deadline exhausted")
        }
        let entry = fs::symlink_metadata(&temporary)?;
        if entry.ino() != target_identity.ino()
            || entry.dev() != target_identity.dev()
            || entry.nlink() != 1
            || entry.len() != bytes
            || entry.mtime() != target_identity.mtime()
            || entry.mtime_nsec() != target_identity.mtime_nsec()
        {
            bail!("Restore staging file changed during validation")
        }
        let current = fs::symlink_metadata(destination)?;
        if current.ino() != directory.ino()
            || current.dev() != directory.dev()
            || current.file_type().is_symlink()
        {
            bail!("Restore directory identity changed")
        }
        let rag_artifacts_restored = if restore_rag {
            let mut writable = Connection::open_with_flags(
                &temporary,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )?;
            writable.pragma_update(None, "trusted_schema", false)?;
            writable.progress_handler(
                1000,
                Some(move || started.elapsed() > Duration::from_secs(10)),
            );
            let count = crate::rag_archive::restore(&mut writable, destination, artifact_limit)?;
            drop(writable);
            target.sync_all()?;
            if target.metadata()?.len() > cfg.max_database_bytes
                || started.elapsed() > Duration::from_secs(10)
            {
                bail!("RAG restoration exceeds resource bounds")
            }
            Some(count)
        } else {
            None
        };
        fs::hard_link(&temporary, &published)?;
        fs::remove_file(&temporary)?;
        if let Some(configuration) = configuration {
            let mut config_file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(destination.join("config.json"))?;
            config_file.write_all(&configuration)?;
            config_file.sync_all()?;
        }
        fs::File::open(destination)?.sync_all()?;
        let report = Restored {
            rag_artifacts_restored,
            format_version: 1,
            restore_id: uuid::Uuid::new_v4().to_string(),
            home: destination.into(),
            source_checksum: actual,
            bytes,
            schema_version: schema,
            running_tasks_not_replayed: running,
            configuration_restored: restore_configuration,
            external_artifacts_restored: false,
        };
        let mut record = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(destination.join("restore-manifest.json"))?;
        record.write_all(&serde_json::to_vec_pretty(&report)?)?;
        record.sync_all()?;
        fs::File::open(destination)?.sync_all()?;
        Ok(report)
    })();
    if result.is_err() {
        if let (Some((dev, ino)), Ok(parent), Ok(entry)) = (
            staging_identity,
            fs::symlink_metadata(destination),
            fs::symlink_metadata(&temporary),
        ) {
            if parent.dev() == directory.dev()
                && parent.ino() == directory.ino()
                && entry.dev() == dev
                && entry.ino() == ino
                && !entry.file_type().is_symlink()
            {
                let _ = fs::remove_file(&temporary);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_explicit_configuration_and_rejects_invalid_embedded_settings() {
        let root =
            std::env::temp_dir().join(format!("fabric-restore-config-{}", uuid::Uuid::new_v4()));
        let cfg = Config {
            home: root.join("source"),
            max_tokens: 1024,
            inference_auth_scope: Some("http://127.0.0.1:1234/v1".into()),
            ..Config::default()
        };
        cfg.prepare().unwrap();
        let store = crate::store::Store::open(&cfg.db()).unwrap();
        let backup = crate::backups::create(&cfg, &store).unwrap();
        assert!(backup.includes_configuration);
        let destination = root.join("restored");
        let report =
            database_with_configuration(&cfg, &backup.path, &backup.checksum, &destination, true)
                .unwrap();
        assert!(report.configuration_restored);
        let saved: Config =
            serde_json::from_slice(&fs::read(destination.join("config.json")).unwrap()).unwrap();
        // Home is deliberately omitted from serialized configuration; the loader
        // binds it to the selected workspace rather than an archived source path.
        assert!(saved.home.as_os_str().is_empty());
        assert_eq!(
            Config::load(Some(destination.clone()), None).unwrap().home,
            destination
        );
        assert_eq!(saved.max_tokens, 1024);
        assert_eq!(saved.inference_auth_scope, cfg.inference_auth_scope);
        assert_eq!(
            fs::metadata(destination.join("config.json"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        let corrupt = root.join("invalid.db");
        fs::copy(&backup.path, &corrupt).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&corrupt, fs::Permissions::from_mode(0o600)).unwrap();
        let db = Connection::open(&corrupt).unwrap();
        let mut invalid = cfg.clone();
        invalid.api_base = "http://192.0.2.1/v1".into();
        db.execute(
            "UPDATE backup_configuration SET body=?1",
            [serde_json::to_string(&invalid).unwrap()],
        )
        .unwrap();
        drop(db);
        let hash = crate::store::sha256(&fs::read(&corrupt).unwrap());
        let rejected = root.join("rejected");
        assert!(database_with_configuration(&cfg, &corrupt, &hash, &rejected, true).is_err());
        assert!(!rejected.join("fabric.sqlite3").exists());
        assert!(!rejected.join("config.json").exists());
        let db = Connection::open(&corrupt).unwrap();
        db.execute_batch("DROP TABLE backup_configuration;")
            .unwrap();
        drop(db);
        let hash = crate::store::sha256(&fs::read(&corrupt).unwrap());
        assert!(database_with_configuration(
            &cfg,
            &corrupt,
            &hash,
            &root.join("legacy-request"),
            true
        )
        .is_err());
        assert!(database(&cfg, &corrupt, &hash, &root.join("legacy-default")).is_ok());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restores_checked_state_without_overwrite_or_task_replay() {
        let root = std::env::temp_dir().join(format!("fabric-restore-{}", uuid::Uuid::new_v4()));
        let cfg = Config {
            home: root.join("source"),
            ..Config::default()
        };
        cfg.prepare().unwrap();
        let store = crate::store::Store::open(&cfg.db()).unwrap();
        let task = store.create_task("unfinished", "fast").unwrap();
        let backup = crate::backups::create(&cfg, &store).unwrap();
        let destination = root.join("restored");
        let report = database(&cfg, &backup.path, &backup.checksum, &destination).unwrap();
        assert_eq!(report.running_tasks_not_replayed, 1);
        assert_eq!(
            fs::metadata(destination.join("fabric.sqlite3"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            crate::store::sha256(&fs::read(destination.join("fabric.sqlite3")).unwrap()),
            backup.checksum
        );
        assert_eq!(
            crate::store::sha256(&fs::read(&backup.path).unwrap()),
            backup.checksum
        );
        let recorded: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("restore-manifest.json")).unwrap())
                .unwrap();
        assert_eq!(recorded["source_checksum"], backup.checksum);
        assert_eq!(recorded["restore_id"], report.restore_id);
        let restored = crate::store::Store::open(&destination.join("fabric.sqlite3")).unwrap();
        let status: String = restored
            .lock()
            .unwrap()
            .query_row("SELECT status FROM tasks WHERE id=?1", [&task], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(status, "running");
        assert!(!destination.join("config.json").exists());
        assert_eq!(fs::metadata(&destination).unwrap().mode() & 0o777, 0o700);
        assert!(database(&cfg, &backup.path, &backup.checksum, &destination).is_err());
        let bad = root.join("bad");
        assert!(database(&cfg, &backup.path, &"0".repeat(64), &bad).is_err());
        assert!(!bad.join("fabric.sqlite3").exists());
        let link = root.join("snapshot-link");
        std::os::unix::fs::symlink(&backup.path, &link).unwrap();
        assert!(database(&cfg, &link, &backup.checksum, &root.join("linked")).is_err());
        assert!(security::authorize("jev-r", "storage.restore").is_err());
        drop(restored);
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_future_corrupt_and_live_wal_files_before_publication() {
        let root =
            std::env::temp_dir().join(format!("fabric-restore-negative-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let cfg = Config::default();
        let future = root.join("future.db");
        let db = Connection::open(&future).unwrap();
        db.execute_batch("CREATE TABLE tasks(status TEXT); PRAGMA user_version=7;")
            .unwrap();
        drop(db);
        let checksum = crate::store::sha256(&fs::read(&future).unwrap());
        let destination = root.join("future-output");
        assert!(database(&cfg, &future, &checksum, &destination)
            .unwrap_err()
            .to_string()
            .contains("schema version"));
        assert!(!destination.join("fabric.sqlite3").exists());
        let corrupt = root.join("corrupt.db");
        let oversized = root.join("oversized.db");
        fs::File::create(&oversized)
            .unwrap()
            .set_len(cfg.max_database_bytes + 1)
            .unwrap();
        assert!(database(
            &cfg,
            &oversized,
            &"a".repeat(64),
            &root.join("oversized-output")
        )
        .unwrap_err()
        .to_string()
        .contains("bounded regular"));
        fs::write(&corrupt, b"not a database").unwrap();
        let checksum = crate::store::sha256(&fs::read(&corrupt).unwrap());
        assert!(database(&cfg, &corrupt, &checksum, &root.join("corrupt-output")).is_err());
        let wal = root.join("live.db");
        let db = Connection::open(&wal).unwrap();
        db.execute_batch(
            "PRAGMA journal_mode=WAL; CREATE TABLE tasks(status TEXT); PRAGMA user_version=6;",
        )
        .unwrap();
        let checksum = crate::store::sha256(&fs::read(&wal).unwrap());
        assert!(database(&cfg, &wal, &checksum, &root.join("wal-output")).is_err());
        let existing = root.join("existing");
        fs::create_dir(&existing).unwrap();
        assert!(database(&cfg, &future, &"a".repeat(64), &existing).is_err());
        assert!(existing.read_dir().unwrap().next().is_none());
        drop(db);
        fs::remove_dir_all(root).unwrap();
    }
}
