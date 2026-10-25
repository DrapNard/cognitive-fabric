//! Restore an operator-reviewed database snapshot into a new workspace only.
use crate::{config::Config, security};
use anyhow::{bail, Result};
use rusqlite::OpenFlags;
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
    pub source_schema_version: u32,
    pub rag_artifacts_restored: Option<usize>,
    pub format_version: u16,
    pub restore_id: String,
    pub home: PathBuf,
    pub source_checksum: String,
    pub bytes: u64,
    pub schema_version: u32,
    pub running_tasks_not_replayed: u64,
    pub invalidated_client_sessions: u64,
    pub configuration_restored: bool,
    pub external_artifacts_restored: bool,
}

const MAX_RESTORED_CLIENT_SESSIONS: usize = 10_000;
const MAX_AGENT_SESSION_SNAPSHOT_BYTES: usize = 32_768;

fn is_sha256_hex(value: &serde_json::Value) -> bool {
    value.as_str().is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

/// Clear resumable agent-tool authority from a restored database. Restoring an
/// older backup must not make a previously delivered client tool proposal live
/// again. The caller owns an unpublished staging database.
fn invalidate_restored_client_sessions(
    db: &mut rusqlite::Connection,
    started: Instant,
) -> Result<u64> {
    let rows = db
        .prepare(
            "SELECT s.id,s.task_id,s.state, \
             CASE WHEN length(CAST(s.snapshot AS BLOB))<=32768 THEN s.snapshot END, \
             s.snapshot_sha256 \
             FROM agent_sessions s WHERE s.state IN ('active','waiting') \
             ORDER BY s.updated,s.id LIMIT 10001",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > MAX_RESTORED_CLIENT_SESSIONS {
        bail!("Restore contains too many unfinished client collective sessions")
    }

    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for (session_id, task_id, previous_state, snapshot, snapshot_sha256) in &rows {
        if started.elapsed() > Duration::from_secs(10) {
            bail!("Restore session invalidation deadline exhausted")
        }

        let sanitized = snapshot
            .as_deref()
            .and_then(|raw| sanitize_agent_snapshot(raw, snapshot_sha256));
        let updated = if let Some((raw, checksum)) = sanitized.as_ref() {
            tx.execute(
                "UPDATE agent_sessions SET state='failed',snapshot=?2,snapshot_sha256=?3,updated=unixepoch() \
                 WHERE id=?1 AND state IN ('active','waiting')",
                rusqlite::params![session_id, raw, checksum],
            )?
        } else {
            tx.execute(
                "UPDATE agent_sessions SET state='failed',updated=unixepoch() \
                 WHERE id=?1 AND state IN ('active','waiting')",
                [session_id],
            )?
        };
        if updated != 1 {
            bail!("Restored client session changed during invalidation")
        }

        tx.execute(
            "UPDATE tasks SET status='interrupted',answer='Client collective session invalidated during restore; no automatic replay',updated=unixepoch() \
             WHERE id=?1 AND status IN ('running','waiting_client')",
            [task_id],
        )?;
        tx.execute(
            "DELETE FROM agent_session_calls WHERE session_id=?1",
            [session_id],
        )?;
        let payload = serde_json::json!({
            "format_version": 1,
            "session_id": session_id,
            "previous_state": previous_state,
            "state": "failed",
            "reason": "explicit_database_restore",
            "automatic_replay": false,
            "refund": false,
            "pending_call_mappings_removed": true,
            "snapshot_context_scrubbed": sanitized.is_some()
        });
        let raw_payload = serde_json::to_string(&payload)?;
        if raw_payload.len() > 8192 {
            bail!("Restore session invalidation audit exceeds bounds")
        }
        tx.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'agent_session_restore_invalidated','human-cli',?2,unixepoch())",
            rusqlite::params![task_id, raw_payload],
        )?;
    }
    tx.commit()?;
    Ok(rows.len() as u64)
}

/// Return a rechecksummed checkpoint with all user-derived context and replay
/// handles removed, or None when the original bounded checkpoint is not valid.
fn sanitize_agent_snapshot(raw: &str, expected_checksum: &str) -> Option<(String, String)> {
    if raw.len() > MAX_AGENT_SESSION_SNAPSHOT_BYTES
        || crate::store::sha256(raw.as_bytes()) != expected_checksum
    {
        return None;
    }
    let mut value =
        crate::strict_json::value(raw.as_bytes(), MAX_AGENT_SESSION_SNAPSHOT_BYTES).ok()?;
    let object = value.as_object()?;
    const KEYS: [&str; 26] = [
        "version",
        "configuration",
        "registration",
        "effort",
        "temperature",
        "mode",
        "clock",
        "wall_seconds",
        "objective_index",
        "references",
        "reference_pins",
        "internal_tools",
        "call_limit",
        "cycle_limit",
        "output_limit",
        "input_limit",
        "tool_limit",
        "calls",
        "cycles",
        "reserved",
        "prompt_bytes",
        "proposals",
        "advice",
        "history_len",
        "history_sha256",
        "pending",
    ];
    if object.len() != KEYS.len() || KEYS.iter().any(|key| !object.contains_key(*key)) {
        return None;
    }

    let string_fields = [
        "configuration",
        "registration",
        "mode",
        "references",
        "advice",
        "history_sha256",
    ];
    if string_fields
        .iter()
        .any(|key| object.get(*key).is_none_or(|value| !value.is_string()))
        || object.get("version")?.as_u64()? != 1
        || object.get("wall_seconds")?.as_u64()?.clamp(1, 3600)
            != object.get("wall_seconds")?.as_u64()?
        || !object.get("temperature")?.as_f64()?.is_finite()
        || !(object.get("effort")?.is_null() || object.get("effort")?.is_string())
        || !object.get("reference_pins")?.is_array()
        || !object.get("pending")?.is_array()
        || !object.get("clock")?.is_object()
        || !is_sha256_hex(object.get("configuration")?)
        || !is_sha256_hex(object.get("registration")?)
        || !is_sha256_hex(object.get("history_sha256")?)
    {
        return None;
    }
    for key in [
        "objective_index",
        "internal_tools",
        "call_limit",
        "cycle_limit",
        "output_limit",
        "input_limit",
        "tool_limit",
        "calls",
        "cycles",
        "reserved",
        "prompt_bytes",
        "proposals",
        "history_len",
    ] {
        object.get(key)?.as_u64()?;
    }
    let wall_seconds = object.get("wall_seconds")?.as_u64()?;
    let clock =
        serde_json::from_value::<crate::execution_clock::Snapshot>(object.get("clock")?.clone())
            .ok()?;
    clock.validate(wall_seconds).ok()?;
    let temperature = object.get("temperature")?.as_f64()?;
    let history_len = object.get("history_len")?.as_u64()?;
    let objective_index = object.get("objective_index")?.as_u64()?;
    let tool_limit = object.get("tool_limit")?.as_u64()?;
    let proposals = object.get("proposals")?.as_u64()?;
    let internal_tools = object.get("internal_tools")?.as_u64()?;
    let call_limit = object.get("call_limit")?.as_u64()?;
    let cycle_limit = object.get("cycle_limit")?.as_u64()?;
    let calls = object.get("calls")?.as_u64()?;
    let cycles = object.get("cycles")?.as_u64()?;
    let reserved = object.get("reserved")?.as_u64()?;
    let output_limit = object.get("output_limit")?.as_u64()?;
    let prompt_bytes = object.get("prompt_bytes")?.as_u64()?;
    let input_limit = object.get("input_limit")?.as_u64()?;
    let pending = object.get("pending")?.as_array()?;
    if !(-0.0..=2.0).contains(&temperature)
        || history_len > 255
        || (history_len > 0 && objective_index >= history_len)
        || proposals.saturating_add(internal_tools) > tool_limit
        || calls > call_limit
        || cycles > cycle_limit
        || reserved > output_limit
        || prompt_bytes > input_limit
        || pending.iter().any(|id| {
            id.as_str().is_none_or(|id| {
                !id.strip_prefix("cgf_").is_some_and(|id| {
                    id.len() == 32
                        && id
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
            })
        })
        || object.get("references")?.as_str()?.len() > 8000
        || object.get("advice")?.as_str()?.len() > 2000
        || pending.len() > 16
        || serde_json::to_vec(object.get("reference_pins")?)
            .ok()?
            .len()
            > 4096
    {
        return None;
    }

    let object = value.as_object_mut()?;
    object.insert(
        "references".into(),
        serde_json::Value::String(String::new()),
    );
    object.insert("reference_pins".into(), serde_json::json!([]));
    object.insert("advice".into(), serde_json::Value::String(String::new()));
    object.insert("pending".into(), serde_json::json!([]));
    object.insert(
        "history_sha256".into(),
        serde_json::Value::String(String::new()),
    );
    let sanitized = serde_json::to_string(&value).ok()?;
    if sanitized.len() > MAX_AGENT_SESSION_SNAPSHOT_BYTES {
        return None;
    }
    let checksum = crate::store::sha256(sanitized.as_bytes());
    Some((sanitized, checksum))
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
    database_with_migration(
        cfg,
        snapshot,
        checksum,
        destination,
        restore_configuration,
        restore_rag,
        false,
    )
}
pub fn database_with_migration(
    cfg: &Config,
    snapshot: &Path,
    checksum: &str,
    destination: &Path,
    restore_configuration: bool,
    restore_rag: bool,
    migrate: bool,
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
        let db = crate::storage::wal_vfs::open(
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
        let current_schema = crate::store::CURRENT_SCHEMA_VERSION as u32;
        if schema != current_schema && (!migrate || !(1..current_schema).contains(&schema)) {
            bail!("Restore requires the current schema version or explicit migration of recognized older schema versions")
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
        if schema != current_schema {
            crate::store::Store::migrate_snapshot(&temporary, cfg.max_database_bytes, started)?;
            target.sync_all()?;
            if target.metadata()?.len() > cfg.max_database_bytes
                || started.elapsed() > Duration::from_secs(10)
            {
                bail!("Schema migration exceeds restore bounds")
            }
        }
        let invalidated_client_sessions = {
            let mut writable = crate::storage::wal_vfs::open(
                &temporary,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )?;
            writable.pragma_update(None, "trusted_schema", false)?;
            writable.progress_handler(
                1000,
                Some(move || started.elapsed() > Duration::from_secs(10)),
            );
            let count = invalidate_restored_client_sessions(&mut writable, started)?;
            writable.progress_handler(0, None::<fn() -> bool>);
            drop(writable);
            target.sync_all()?;
            if target.metadata()?.len() > cfg.max_database_bytes
                || started.elapsed() > Duration::from_secs(10)
            {
                bail!("Restore session invalidation exceeds resource bounds")
            }
            count
        };
        let rag_artifacts_restored = if restore_rag {
            let mut writable = crate::storage::wal_vfs::open(
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
            source_schema_version: schema,
            rag_artifacts_restored,
            format_version: 1,
            restore_id: uuid::Uuid::new_v4().to_string(),
            home: destination.into(),
            source_checksum: actual,
            bytes,
            schema_version: current_schema,
            running_tasks_not_replayed: running,
            invalidated_client_sessions,
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
    use rusqlite::Connection;
    #[test]
    fn migrates_known_older_snapshots_only_with_explicit_opt_in() {
        use std::os::unix::fs::PermissionsExt;
        let current_schema = crate::store::CURRENT_SCHEMA_VERSION as u32;
        let root =
            std::env::temp_dir().join(format!("fabric-restore-migration-{}", uuid::Uuid::new_v4()));
        let cfg = Config {
            home: root.join("source"),
            ..Config::default()
        };
        cfg.prepare().unwrap();
        let store = crate::store::Store::open(&cfg.db()).unwrap();
        let task = store
            .create_task("Do not replay this task", "fast")
            .unwrap();
        store
            .event(
                &task,
                "model_residency",
                "human-cli",
                serde_json::json!({"state":"confirmed","adapter":"lmstudio-native-v1","action":{"operation":"load"}}),
            )
            .unwrap();
        let backup = crate::backups::create(&cfg, &store).unwrap();
        let legacy = root.join("schema-six.db");
        fs::copy(&backup.path, &legacy).unwrap();
        fs::set_permissions(&legacy, fs::Permissions::from_mode(0o600)).unwrap();
        let db = Connection::open(&legacy).unwrap();
        db.execute_batch("DROP TABLE agent_session_calls; DROP TABLE agent_sessions; DROP TABLE project_profiles; DROP INDEX native_task_lifecycle_history; DROP TABLE managed_residency_resolution_members; DROP TABLE managed_residency_resolutions; DROP TABLE managed_residency_receipt_archive; DROP TRIGGER managed_residency_hot_no_archive_duplicate; DROP TRIGGER managed_residency_identity_immutable; DROP INDEX managed_residency_active_identity; DROP INDEX managed_residency_state; DROP INDEX managed_residency_event_scan; DROP TABLE managed_residency_receipts; DROP INDEX idx_tasks_status; DROP INDEX event_conversation_task; DROP TABLE retention_audit; PRAGMA user_version=6;").unwrap();
        drop(db);
        let bytes = fs::read(&legacy).unwrap();
        let hash = crate::store::sha256(&bytes);
        assert!(database(&cfg, &legacy, &hash, &root.join("default-rejected")).is_err());
        let destination = root.join("migrated");
        let report =
            database_with_migration(&cfg, &legacy, &hash, &destination, false, false, true)
                .unwrap();
        assert_eq!(report.source_schema_version, 6);
        assert_eq!(report.schema_version, current_schema);
        assert_eq!(report.running_tasks_not_replayed, 1);
        assert_eq!(fs::read(&legacy).unwrap(), bytes);
        let db = Connection::open_with_flags(
            destination.join("fabric.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let schema: u32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(schema, current_schema);
        let journal: String = db
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(journal, "delete");
        let status: String = db
            .query_row("SELECT status FROM tasks WHERE id=?1", [&task], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(status, "running");
        let receipt_count: u64 = db
            .query_row("SELECT count(*) FROM managed_residency_receipts", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            receipt_count, 0,
            "legacy provider observations must not be adopted"
        );
        let legacy_events: u64 = db
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='model_residency'",
                [&task],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            legacy_events, 1,
            "legacy audit history is preserved verbatim"
        );
        drop(db);
        for version in 1..6 {
            let source = root.join(format!("schema-{version}.db"));
            fs::write(&source, &bytes).unwrap();
            let db = Connection::open(&source).unwrap();
            db.execute_batch("DROP TABLE model_audit; DROP TABLE model_manifests;")
                .unwrap();
            if version < 4 {
                db.execute_batch(
                    "DROP TRIGGER memory_insert; DROP TABLE memory_index; DROP TABLE memories;",
                )
                .unwrap();
            }
            if version < 3 {
                db.execute_batch("DROP TABLE evaluation_datasets;").unwrap();
            }
            if version < 2 {
                db.execute_batch("DROP TABLE expert_audit; DROP TABLE expert_manifests; ALTER TABLE skills DROP COLUMN enabled;").unwrap();
            }
            db.pragma_update(None, "user_version", version).unwrap();
            drop(db);
            let original = fs::read(&source).unwrap();
            let hash = crate::store::sha256(&original);
            let output = root.join(format!("migrated-{version}"));
            let report =
                database_with_migration(&cfg, &source, &hash, &output, false, false, true).unwrap();
            assert_eq!(report.source_schema_version, version);
            assert_eq!(report.schema_version, current_schema);
            assert_eq!(fs::read(&source).unwrap(), original);
            let db = Connection::open_with_flags(
                output.join("fabric.sqlite3"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
                    .unwrap(),
                current_schema
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM managed_residency_receipts", [], |r| r
                    .get::<_, u64>(0))
                    .unwrap(),
                0,
                "migration from schema {version} must not adopt manual residency history"
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM events WHERE task_id=?1 AND kind='model_residency'",
                    [&task],
                    |r| r.get::<_, u64>(0),
                )
                .unwrap(),
                1,
                "migration from schema {version} must preserve residency audit history"
            );
        }
        let db = Connection::open(&legacy).unwrap();
        db.execute_batch("CREATE INDEX managed_residency_active_identity ON tasks(id);")
            .unwrap();
        drop(db);
        let hash = crate::store::sha256(&fs::read(&legacy).unwrap());
        let rejected = root.join("migration-failed");
        assert!(
            database_with_migration(&cfg, &legacy, &hash, &rejected, false, false, true).is_err()
        );
        assert!(!rejected.join("fabric.sqlite3").exists());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
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
    fn restore_invalidates_a_backed_up_waiting_tool_checkpoint() {
        let root = std::env::temp_dir().join(format!(
            "fabric-restore-agent-session-{}",
            uuid::Uuid::new_v4()
        ));
        let cfg = Config {
            home: root.join("source"),
            ..Config::default()
        };
        cfg.prepare().unwrap();
        let store = crate::store::Store::open(&cfg.db()).unwrap();
        let task = store
            .create_task("Continue a client tool loop", "research")
            .unwrap();
        store.finish_task(&task, "waiting_client", "").unwrap();
        let session_id = uuid::Uuid::new_v4().to_string();
        let claim = uuid::Uuid::new_v4().to_string();
        let call_id = format!("cgf_{}", "a".repeat(32));
        let clock = crate::execution_clock::ExecutionStart::new(120)
            .clock
            .expect("test host needs a same-boot execution clock");
        let checkpoint = serde_json::json!({
            "version": 1,
            "configuration": "b".repeat(64),
            "registration": "c".repeat(64),
            "effort": "high",
            "temperature": 0.2,
            "mode": "research",
            "clock": clock,
            "wall_seconds": 120,
            "objective_index": 0,
            "references": "private retrieved context",
            "reference_pins": [{"id":"private-reference"}],
            "internal_tools": 1,
            "call_limit": 8,
            "cycle_limit": 4,
            "output_limit": 4096,
            "input_limit": 512000,
            "tool_limit": 4,
            "calls": 3,
            "cycles": 1,
            "reserved": 352,
            "prompt_bytes": 1234,
            "proposals": 1,
            "advice": "private derived advice",
            "history_len": 2,
            "history_sha256": "d".repeat(64),
            "pending": [call_id]
        });
        let raw_checkpoint = serde_json::to_string(&checkpoint).unwrap();
        store
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO agent_sessions(id,task_id,state,generation,claim,snapshot,snapshot_sha256,updated) \
                 VALUES (?1,?2,'waiting',1,?3,?4,?5,unixepoch())",
                rusqlite::params![
                    session_id,
                    task,
                    claim,
                    raw_checkpoint,
                    crate::store::sha256(raw_checkpoint.as_bytes())
                ],
            )
            .unwrap();
        store
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO agent_session_calls(id,session_id,generation,call_sha256) VALUES (?1,?2,1,?3)",
                rusqlite::params![call_id, session_id, "e".repeat(64)],
            )
            .unwrap();

        let backup = crate::backups::create(&cfg, &store).unwrap();
        let destination = root.join("restored");
        // Simulate the live source progressing after the backup captured its
        // pending proposal. Restore must not roll that checkpoint back to live.
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE agent_sessions SET state='closed' WHERE id=?1",
                [&session_id],
            )
            .unwrap();
        store
            .lock()
            .unwrap()
            .execute("UPDATE tasks SET status='unverified' WHERE id=?1", [&task])
            .unwrap();
        store
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM agent_session_calls WHERE session_id=?1",
                [&session_id],
            )
            .unwrap();

        let report = database(&cfg, &backup.path, &backup.checksum, &destination).unwrap();
        assert_eq!(report.invalidated_client_sessions, 1);
        let restore_manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("restore-manifest.json")).unwrap())
                .unwrap();
        assert_eq!(restore_manifest["invalidated_client_sessions"], 1);
        assert_eq!(
            crate::store::sha256(&fs::read(&backup.path).unwrap()),
            backup.checksum,
            "restore leaves its source backup unchanged"
        );
        let restored = crate::store::Store::open(&destination.join("fabric.sqlite3")).unwrap();
        assert_eq!(
            restored.task(&task).unwrap()["task"]["status"],
            "interrupted"
        );
        let db = restored.lock().unwrap();
        let (session_state, sanitized, checksum): (String, String, String) = db
            .query_row(
                "SELECT state,snapshot,snapshot_sha256 FROM agent_sessions WHERE id=?1",
                [&session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(session_state, "failed");
        assert_eq!(
            crate::store::sha256(sanitized.as_bytes()),
            checksum,
            "the scrubbed checkpoint checksum is refreshed"
        );
        let sanitized: serde_json::Value = serde_json::from_str(&sanitized).unwrap();
        assert_eq!(sanitized["references"], "");
        assert_eq!(sanitized["reference_pins"], serde_json::json!([]));
        assert_eq!(sanitized["advice"], "");
        assert_eq!(sanitized["pending"], serde_json::json!([]));
        assert_eq!(sanitized["history_sha256"], "");
        assert_eq!(sanitized["calls"], 3, "charges remain preserved");
        assert_eq!(
            sanitized["reserved"], 352,
            "reserved tokens are not refunded"
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM agent_session_calls WHERE session_id=?1",
                [&session_id],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='agent_session_restore_invalidated'",
                [&task],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
            1
        );
        drop(db);
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
        db.execute_batch("CREATE TABLE tasks(status TEXT);")
            .unwrap();
        db.pragma_update(
            None,
            "user_version",
            crate::store::CURRENT_SCHEMA_VERSION + 1,
        )
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
