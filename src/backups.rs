//! Consistent operator SQLite backups with checked configuration metadata.
use crate::{config::Config, security, store::Store};
use anyhow::{bail, Result};
use rusqlite::{
    backup::{Backup, StepResult},
    OpenFlags,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::os::fd::AsRawFd;
use std::{
    fs,
    io::Read,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub rag_artifact_count: Option<usize>,
    pub path: PathBuf,
    pub checksum: String,
    pub bytes: u64,
    pub schema_version: u32,
    pub includes_external_artifacts: bool,
    pub includes_configuration: bool,
    pub automatic_task_replay: bool,
}

pub fn create(cfg: &Config, store: &Store) -> Result<Snapshot> {
    create_with_rag(cfg, store, false)
}
pub fn create_with_rag(cfg: &Config, store: &Store, include_rag: bool) -> Result<Snapshot> {
    security::authorize("human-cli", "storage.backup")?;
    cfg.validate()?;
    cfg.prepare()?;
    let directory = cfg.home.join("backups");
    match fs::symlink_metadata(&directory) {
        Ok(metadata)
            if metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == unsafe { libc::geteuid() } => {}
        Ok(_) => bail!("Backup directory must be owner-controlled and not a symlink"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new().mode(0o700).create(&directory)?
        }
        Err(e) => return Err(e.into()),
    }
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    // Serialize disk admission without taking SQLite's write reservation.
    let lease = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(cfg.home.join("backups.lock"))?;
    let lease_metadata = lease.metadata()?;
    if !lease_metadata.is_file()
        || lease_metadata.nlink() != 1
        || lease_metadata.mode() & 0o077 != 0
        || lease_metadata.uid() != unsafe { libc::geteuid() }
    {
        bail!("Invalid backup lease file")
    }
    if unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("Another backup is in progress")
    }
    let mut source = store.lock()?;
    let transaction = source.transaction()?;
    transaction.query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
        row.get::<_, u64>(0)
    })?;
    let pages: u64 = transaction.pragma_query_value(None, "page_count", |r| r.get(0))?;
    let page_size: u64 = transaction.pragma_query_value(None, "page_size", |r| r.get(0))?;
    let size = pages.saturating_mul(page_size);
    if size > cfg.max_database_bytes {
        bail!("Backup exceeds configured database limit")
    }
    let mut count = 0;
    let mut used = 0u64;
    for entry in fs::read_dir(&directory)? {
        let metadata = fs::symlink_metadata(entry?.path())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            bail!("Invalid backup directory entry")
        }
        count += 1;
        used = used.saturating_add(metadata.len());
        if count >= 8 {
            bail!("Backup count capacity exhausted")
        }
    }
    if used.saturating_add(size) > cfg.max_database_bytes.saturating_mul(2).min(2_000_000_000) {
        bail!("Backup byte capacity exhausted")
    }
    let id = uuid::Uuid::new_v4();
    let temporary = directory.join(format!("{id}.tmp"));
    let destination = directory.join(format!("{id}.db"));
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)?;
    let identity = file.metadata()?;
    let result = (|| -> Result<Snapshot> {
        let mut target = crate::storage::wal_vfs::open(
            &temporary,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        let started = Instant::now();
        {
            let backup = Backup::new(&transaction, &mut target)?;
            loop {
                if started.elapsed() > Duration::from_secs(10) {
                    bail!("Backup copy deadline exhausted")
                }
                match backup.step(128)? {
                    StepResult::Done => break,
                    StepResult::More => {}
                    _ => bail!("Backup could not acquire its database view"),
                }
            }
        }
        let mut configuration = cfg.clone();
        configuration.home = PathBuf::new();
        let configuration = serde_json::to_string(&configuration)?;
        if configuration.len() > 64_000 {
            bail!("Backup configuration exceeds bounds")
        }
        target.pragma_update(None, "trusted_schema", false)?;
        let rag_artifact_count = if include_rag {
            Some(crate::rag_archive::capture(&target, cfg)?)
        } else {
            // Do not carry stale payloads when backing up a previously restored database.
            target.execute_batch("DROP TABLE IF EXISTS backup_rag_artifacts;")?;
            None
        };
        target.execute_batch("CREATE TABLE IF NOT EXISTS backup_configuration (id INTEGER PRIMARY KEY CHECK(id=1), format_version INTEGER NOT NULL, body TEXT NOT NULL);")?;
        target.execute(
            "INSERT OR REPLACE INTO backup_configuration(id,format_version,body) VALUES(1,1,?1)",
            [&configuration],
        )?;
        target.pragma_update(None, "journal_mode", "DELETE")?;
        let check: String = target.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
        if check != "ok" {
            bail!("Backup failed SQLite consistency check")
        }
        let schema = target.pragma_query_value(None, "user_version", |r| r.get(0))?;
        drop(target);
        let metadata = fs::symlink_metadata(&temporary)?;
        if metadata.ino() != identity.ino()
            || metadata.dev() != identity.dev()
            || metadata.nlink() != 1
            || !metadata.is_file()
            || metadata.len() > cfg.max_database_bytes
        {
            bail!("Backup file identity or bounds changed")
        }
        let mut reader = &file;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            if started.elapsed() > Duration::from_secs(10) {
                bail!("Backup verification deadline exhausted")
            }
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            if bytes > cfg.max_database_bytes {
                bail!("Backup grew beyond byte limit")
            }
            hash.update(&buffer[..n]);
        }
        file.set_permissions(fs::Permissions::from_mode(0o400))?;
        if used.saturating_add(bytes) > cfg.max_database_bytes.saturating_mul(2).min(2_000_000_000)
        {
            bail!("Backup byte capacity exhausted after metadata insertion")
        }
        file.sync_all()?;
        fs::hard_link(&temporary, &destination)?;
        fs::remove_file(&temporary)?;
        fs::File::open(&directory)?.sync_all()?;
        Ok(Snapshot {
            rag_artifact_count,
            path: destination.clone(),
            checksum: hex::encode(hash.finalize()),
            bytes,
            schema_version: schema,
            includes_external_artifacts: include_rag,
            includes_configuration: true,
            automatic_task_replay: false,
        })
    })();
    let _ = fs::remove_file(&temporary);
    // No database state was changed; dropping the transaction releases the reservation.
    drop(transaction);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    #[test]
    fn backup_preserves_committed_wal_and_rejects_symlinks_and_capacity() {
        let home = std::env::temp_dir().join(format!("fabric-backup-{}", uuid::Uuid::new_v4()));
        let cfg = Config::load(Some(home.clone()), Some("demo".into())).unwrap();
        cfg.prepare().unwrap();
        let store = Store::open(&cfg.db()).unwrap();
        crate::store::bootstrap(&store).unwrap();
        let task = store.create_task("Backup WAL sentinel", "fast").unwrap();
        store
            .event(
                &task,
                "intent",
                "interface",
                serde_json::json!({"sentinel":"committed WAL"}),
            )
            .unwrap();
        let snapshot = create(&cfg, &store).unwrap();
        let bytes = security::read_regular_bytes(&snapshot.path, 1_000_000).unwrap();
        assert_eq!(crate::store::sha256(&bytes), snapshot.checksum);
        assert_eq!(snapshot.bytes, bytes.len() as u64);
        let lease = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(home.join("backups.lock"))
            .unwrap();
        assert_eq!(
            unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        assert!(create(&cfg, &store)
            .unwrap_err()
            .to_string()
            .contains("in progress"));
        drop(lease);
        let oversized = home.join("backups").join("oversized.db");
        fs::File::create(&oversized)
            .unwrap()
            .set_len(cfg.max_database_bytes * 2)
            .unwrap();
        assert!(create(&cfg, &store)
            .unwrap_err()
            .to_string()
            .contains("byte capacity"));
        fs::remove_file(oversized).unwrap();
        assert_eq!(
            fs::metadata(&snapshot.path).unwrap().permissions().mode() & 0o777,
            0o400
        );
        let restored =
            Connection::open_with_flags(&snapshot.path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(
            restored
                .query_row("SELECT query FROM tasks WHERE id=?1", [&task], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "Backup WAL sentinel"
        );
        assert_eq!(
            restored
                .query_row(
                    "SELECT count(*) FROM events WHERE task_id=?1",
                    [&task],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            1
        );
        store
            .finish_task(&task, "completed", "later write")
            .unwrap();
        assert_eq!(
            restored
                .query_row("SELECT status FROM tasks WHERE id=?1", [&task], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "running"
        );
        for i in 0..7 {
            fs::write(home.join("backups").join(format!("fixture-{i}.db")), []).unwrap();
        }
        assert!(create(&cfg, &store).is_err());
        drop(restored);
        drop(store);
        fs::remove_dir_all(&home).unwrap();
        cfg.prepare().unwrap();
        std::os::unix::fs::symlink("/tmp", home.join("backups")).unwrap();
        assert!(create(&cfg, &Store::in_memory().unwrap()).is_err());
        fs::remove_dir_all(home).unwrap();
        assert!(security::authorize("jev-r", "storage.backup").is_err());
    }
}
