//! Bounded RAG-only snapshot payloads; neural artifacts are deliberately excluded.
use crate::{config::Config, security, store::sha256};
use anyhow::{bail, Result};
use rusqlite::Connection;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};
const MAX_TOTAL: usize = 16_000_000;
const MAX_COUNT: usize = 4096;

pub(crate) fn capture(db: &Connection, cfg: &Config) -> Result<usize> {
    let started = std::time::Instant::now();
    let mut statement = db.prepare(
        "SELECT checksum,artifact FROM candidates WHERE kind='rag' ORDER BY checksum LIMIT 4097",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut artifacts = BTreeMap::new();
    let mut total = 0usize;
    for (index, row) in rows.enumerate() {
        if index >= MAX_COUNT || started.elapsed() > std::time::Duration::from_secs(10) {
            bail!("RAG archive candidate count or deadline exhausted")
        }
        let (checksum, path) = row?;
        let bytes = security::read_regular_bytes(Path::new(&path), 256_000)?;
        if bytes.is_empty() || sha256(&bytes) != checksum {
            bail!("RAG archive artifact integrity failure")
        }
        std::str::from_utf8(&bytes)?;
        if artifacts.contains_key(&checksum) {
            continue;
        }
        total += bytes.len();
        if total > MAX_TOTAL.min(cfg.max_artifact_bytes as usize) || artifacts.len() >= MAX_COUNT {
            bail!("RAG archive capacity exhausted")
        }
        artifacts.insert(checksum, bytes);
    }
    drop(statement);
    db.execute_batch("CREATE TABLE IF NOT EXISTS backup_rag_artifacts(checksum TEXT PRIMARY KEY, body BLOB NOT NULL); DELETE FROM backup_rag_artifacts;")?;
    for (checksum, body) in &artifacts {
        db.execute(
            "INSERT INTO backup_rag_artifacts VALUES(?1,?2)",
            rusqlite::params![checksum, body],
        )?;
    }
    Ok(artifacts.len())
}

pub(crate) fn restore(db: &mut Connection, home: &Path, maximum: u64) -> Result<usize> {
    let started = std::time::Instant::now();
    let transaction = db.transaction()?;
    let triggers: usize = transaction.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND tbl_name='candidates'",
        [],
        |r| r.get(0),
    )?;
    if triggers != 0 {
        bail!("Candidate triggers are not permitted during archive restoration")
    }
    let count: usize =
        transaction.query_row("SELECT count(*) FROM backup_rag_artifacts", [], |r| {
            r.get(0)
        })?;
    if count > MAX_COUNT {
        bail!("RAG archive entry limit exceeded")
    }
    let mut artifacts = Vec::new();
    let mut total = 0usize;
    {
        let mut statement = transaction.prepare("SELECT checksum,body FROM backup_rag_artifacts WHERE length(body) BETWEEN 1 AND 256000 ORDER BY checksum")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (checksum, body) = row?;
            total += body.len();
            if total > MAX_TOTAL.min(maximum as usize) || sha256(&body) != checksum {
                bail!("Invalid RAG archive integrity or capacity")
            }
            std::str::from_utf8(&body)?;
            artifacts.push((checksum, body));
        }
    }
    if artifacts.len() != count {
        bail!("Invalid RAG archive payload bounds")
    }
    let missing: usize = transaction.query_row("SELECT count(*) FROM candidates WHERE kind='rag' AND checksum NOT IN (SELECT checksum FROM backup_rag_artifacts)",[],|r|r.get(0))?;
    let extras: usize = transaction.query_row("SELECT count(*) FROM backup_rag_artifacts WHERE checksum NOT IN (SELECT checksum FROM candidates WHERE kind='rag')",[],|r|r.get(0))?;
    if missing > 0 || extras > 0 {
        bail!("RAG archive candidate coverage mismatch")
    }
    if !artifacts.is_empty() {
        let directory = home.canonicalize()?.join("artifacts");
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        for (checksum, body) in &artifacts {
            if started.elapsed() > std::time::Duration::from_secs(10) {
                bail!("RAG restoration deadline exhausted")
            }
            let path = directory.join(format!("{checksum}.md"));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)?;
            file.write_all(body)?;
            file.set_permissions(fs::Permissions::from_mode(0o400))?;
            file.sync_all()?;
            transaction.execute(
                "UPDATE candidates SET artifact=?1 WHERE kind='rag' AND checksum=?2",
                rusqlite::params![
                    path.to_str()
                        .ok_or_else(|| anyhow::anyhow!("Non-UTF8 restore path"))?,
                    checksum
                ],
            )?;
        }
        fs::File::open(directory)?.sync_all()?;
    }
    transaction.commit()?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_rag_archive_restores_quarantine_and_rejects_tampering() {
        let root =
            std::env::temp_dir().join(format!("fabric-rag-archive-{}", uuid::Uuid::new_v4()));
        let cfg = Config {
            home: root.join("source"),
            ..Config::default()
        };
        cfg.prepare().unwrap();
        let store = crate::store::Store::open(&cfg.db()).unwrap();
        crate::store::bootstrap(&store).unwrap();
        let input = root.join("reviewed.md");
        fs::write(&input, "A reviewed RAG archive fixture").unwrap();
        let candidate =
            crate::evolution::create(&cfg, &store, "rust.s1", "rag", &input, "fixture", "CC0")
                .unwrap();
        let backup = crate::backups::create_with_rag(&cfg, &store, true).unwrap();
        assert_eq!(backup.rag_artifact_count, Some(1));
        let destination = root.join("restored");
        let report = crate::restore::database_with_options(
            &cfg,
            &backup.path,
            &backup.checksum,
            &destination,
            true,
            true,
        )
        .unwrap();
        assert_eq!(report.rag_artifacts_restored, Some(1));
        let restored = crate::store::Store::open(&destination.join("fabric.sqlite3")).unwrap();
        let (path, status): (String, String) = restored
            .lock()
            .unwrap()
            .query_row(
                "SELECT artifact,status FROM candidates WHERE id=?1",
                [&candidate.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(Path::new(&path).starts_with(destination.canonicalize().unwrap()));
        assert_eq!(fs::read(&path).unwrap(), fs::read(&input).unwrap());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o400
        );
        let original: String = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT status FROM candidates WHERE id=?1",
                [&candidate.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, original);
        assert_ne!(
            sha256(&fs::read(destination.join("fabric.sqlite3")).unwrap()),
            backup.checksum
        );
        let invalid = root.join("invalid.db");
        fs::copy(&backup.path, &invalid).unwrap();
        fs::set_permissions(&invalid, fs::Permissions::from_mode(0o600)).unwrap();
        let db = Connection::open(&invalid).unwrap();
        db.execute(
            "UPDATE backup_rag_artifacts SET body=?1",
            [b"changed".as_slice()],
        )
        .unwrap();
        drop(db);
        let checksum = sha256(&fs::read(&invalid).unwrap());
        let rejected = root.join("rejected");
        assert!(crate::restore::database_with_options(
            &cfg, &invalid, &checksum, &rejected, false, true
        )
        .is_err());
        assert!(!rejected.join("fabric.sqlite3").exists());
        assert!(!rejected.join("artifacts").exists());
        for (index, sql) in [
            "DELETE FROM backup_rag_artifacts;",
            "UPDATE backup_rag_artifacts SET body=zeroblob(256001);",
            "CREATE TRIGGER malicious_restore AFTER UPDATE ON candidates BEGIN UPDATE candidates SET status='promoted'; END;",
        ].iter().enumerate() {
            let path=root.join(format!("invalid-{index}.db"));
            fs::copy(&backup.path,&path).unwrap();
            fs::set_permissions(&path,fs::Permissions::from_mode(0o600)).unwrap();
            let db=Connection::open(&path).unwrap();
            db.execute_batch(sql).unwrap();
            drop(db);
            let hash=sha256(&fs::read(&path).unwrap());
            let output=root.join(format!("rejected-{index}"));
            assert!(crate::restore::database_with_options(&cfg,&path,&hash,&output,false,true).is_err());
            assert!(!output.join("fabric.sqlite3").exists());
            assert!(!output.join("artifacts").exists());
        }
        // A default backup of a restored store must not silently retain embedded payloads.
        let restored_cfg = Config {
            home: destination.clone(),
            ..Config::default()
        };
        let database_only = crate::backups::create(&restored_cfg, &restored).unwrap();
        assert_eq!(database_only.rag_artifact_count, None);
        let db = Connection::open_with_flags(
            &database_only.path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let tables: usize = db
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='backup_rag_artifacts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
        drop(db);
        drop(restored);
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
