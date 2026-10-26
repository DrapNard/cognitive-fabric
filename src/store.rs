//! Persistent task, event, document and candidate repository.
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
#[cfg(test)]
#[path = "store/agent_sessions_migration_tests.rs"]
mod agent_sessions_migration_tests;
#[cfg(test)]
mod skill_inventory_tests;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
 id TEXT PRIMARY KEY, query TEXT NOT NULL, mode TEXT NOT NULL,
 status TEXT NOT NULL, answer TEXT NOT NULL DEFAULT '', created REAL NOT NULL, updated REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
 id INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT NOT NULL,
 kind TEXT NOT NULL, source TEXT NOT NULL, payload TEXT NOT NULL, created REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_task ON events(task_id, id);
CREATE TABLE IF NOT EXISTS skills (
 id TEXT PRIMARY KEY, description TEXT NOT NULL, tags_json TEXT NOT NULL,
 active_version INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS documents (
 id TEXT PRIMARY KEY, skill_id TEXT NOT NULL, title TEXT NOT NULL,
 body TEXT NOT NULL, source TEXT NOT NULL, license TEXT NOT NULL,
 sha256 TEXT NOT NULL UNIQUE, FOREIGN KEY(skill_id) REFERENCES skills(id)
);
CREATE VIRTUAL TABLE IF NOT EXISTS document_index USING fts5(title, body, content='documents', content_rowid='rowid');
CREATE TRIGGER IF NOT EXISTS documents_insert AFTER INSERT ON documents BEGIN
 INSERT INTO document_index(rowid,title,body) VALUES (new.rowid,new.title,new.body);
END;
CREATE TABLE IF NOT EXISTS candidates (
 id TEXT PRIMARY KEY, skill_id TEXT NOT NULL, kind TEXT NOT NULL,
 version INTEGER NOT NULL, artifact TEXT NOT NULL, checksum TEXT NOT NULL,
 status TEXT NOT NULL, source TEXT NOT NULL, license TEXT NOT NULL,
 created REAL NOT NULL, UNIQUE(skill_id,version), FOREIGN KEY(skill_id) REFERENCES skills(id)
);
CREATE TABLE IF NOT EXISTS evaluations (
 id INTEGER PRIMARY KEY AUTOINCREMENT, candidate_id TEXT NOT NULL,
 suite TEXT NOT NULL, success INTEGER NOT NULL, score REAL NOT NULL,
 details TEXT NOT NULL, created REAL NOT NULL,
 FOREIGN KEY(candidate_id) REFERENCES candidates(id)
);
"#;

/// Latest schema understood by writable and read-only Store entry points.
pub(crate) const CURRENT_SCHEMA_VERSION: i64 = 11;

const MANAGED_RESIDENCY_SCHEMA: &str = r#"
CREATE TABLE managed_residency_receipts (
 receipt_id TEXT PRIMARY KEY CHECK(length(CAST(receipt_id AS BLOB)) BETWEEN 1 AND 64),
 endpoint TEXT NOT NULL CHECK(length(CAST(endpoint AS BLOB)) BETWEEN 1 AND 512),
 instance_id TEXT NOT NULL CHECK(length(CAST(instance_id AS BLOB)) BETWEEN 1 AND 256),
 receipt_json TEXT NOT NULL CHECK(length(CAST(receipt_json AS BLOB)) BETWEEN 2 AND 8192),
 receipt_sha256 TEXT NOT NULL CHECK(length(receipt_sha256)=64 AND receipt_sha256 NOT GLOB '*[^0-9a-f]*'),
 current_inventory_sha256 TEXT NOT NULL CHECK(length(current_inventory_sha256)=64 AND current_inventory_sha256 NOT GLOB '*[^0-9a-f]*'),
 state TEXT NOT NULL CHECK(state IN ('active','release_requested','released','quarantined')),
 load_task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
 release_task_id TEXT REFERENCES tasks(id) ON DELETE RESTRICT,
 created REAL NOT NULL,
 updated REAL NOT NULL
);
CREATE UNIQUE INDEX managed_residency_active_identity ON managed_residency_receipts(endpoint,instance_id)
 WHERE state!='released';
CREATE INDEX managed_residency_state ON managed_residency_receipts(state,endpoint,updated);
CREATE INDEX managed_residency_event_scan ON events(kind,source,id)
 WHERE kind='model_residency' AND source='human-cli';
CREATE TRIGGER managed_residency_identity_immutable
BEFORE UPDATE OF receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,load_task_id,created
ON managed_residency_receipts
BEGIN
 SELECT RAISE(ABORT,'managed residency receipt identity is immutable');
END;
PRAGMA user_version=7;
"#;

const MANAGED_RESIDENCY_SCHEMA_V8: &str = r#"
-- Copy schema-7 receipt rows verbatim while adding an explicit retired state.
-- Existing external observations are not adopted or rewritten.
DROP INDEX managed_residency_active_identity;
DROP INDEX managed_residency_state;
DROP TRIGGER managed_residency_identity_immutable;
CREATE TABLE managed_residency_receipts_v8 (
 receipt_id TEXT PRIMARY KEY CHECK(length(CAST(receipt_id AS BLOB)) BETWEEN 1 AND 64),
 endpoint TEXT NOT NULL CHECK(length(CAST(endpoint AS BLOB)) BETWEEN 1 AND 512),
 instance_id TEXT NOT NULL CHECK(length(CAST(instance_id AS BLOB)) BETWEEN 1 AND 256),
 receipt_json TEXT NOT NULL CHECK(length(CAST(receipt_json AS BLOB)) BETWEEN 2 AND 8192),
 receipt_sha256 TEXT NOT NULL CHECK(length(receipt_sha256)=64 AND receipt_sha256 NOT GLOB '*[^0-9a-f]*'),
 current_inventory_sha256 TEXT NOT NULL CHECK(length(current_inventory_sha256)=64 AND current_inventory_sha256 NOT GLOB '*[^0-9a-f]*'),
 state TEXT NOT NULL CHECK(state IN ('active','release_requested','released','quarantined','retired')),
 load_task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
 release_task_id TEXT REFERENCES tasks(id) ON DELETE RESTRICT,
 created REAL NOT NULL,
 updated REAL NOT NULL
);
INSERT INTO managed_residency_receipts_v8
 SELECT receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,current_inventory_sha256,state,load_task_id,release_task_id,created,updated
 FROM managed_residency_receipts;
DROP TABLE managed_residency_receipts;
ALTER TABLE managed_residency_receipts_v8 RENAME TO managed_residency_receipts;
CREATE UNIQUE INDEX managed_residency_active_identity ON managed_residency_receipts(endpoint,instance_id)
 WHERE state NOT IN ('released','retired');
CREATE INDEX managed_residency_state ON managed_residency_receipts(state,endpoint,updated);
CREATE TRIGGER managed_residency_identity_immutable
BEFORE UPDATE OF receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,load_task_id,created
ON managed_residency_receipts
BEGIN
 SELECT RAISE(ABORT,'managed residency receipt identity is immutable');
END;

CREATE TABLE managed_residency_resolutions (
 resolution_id TEXT PRIMARY KEY CHECK(length(CAST(resolution_id AS BLOB))=36),
 original_task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id) ON DELETE RESTRICT,
 resolver_task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id) ON DELETE RESTRICT,
 endpoint TEXT NOT NULL CHECK(length(CAST(endpoint AS BLOB)) BETWEEN 1 AND 512),
 operation TEXT NOT NULL CHECK(operation IN ('load_absent','release_absent')),
 query_evidence_sha256 TEXT NOT NULL CHECK(length(query_evidence_sha256)=64 AND query_evidence_sha256 NOT GLOB '*[^0-9a-f]*'),
 intent_sha256 TEXT NOT NULL CHECK(length(intent_sha256)=64 AND intent_sha256 NOT GLOB '*[^0-9a-f]*'),
 inventory_sha256 TEXT NOT NULL CHECK(length(inventory_sha256)=64 AND inventory_sha256 NOT GLOB '*[^0-9a-f]*'),
 model_key TEXT CHECK(model_key IS NULL OR length(CAST(model_key AS BLOB)) BETWEEN 1 AND 256),
 model_type TEXT CHECK(model_type IS NULL OR model_type IN ('llm','embedding','decision')),
 context_length INTEGER CHECK(context_length IS NULL OR context_length BETWEEN 1 AND 32768),
 receipt_id TEXT CHECK(receipt_id IS NULL OR length(CAST(receipt_id AS BLOB)) BETWEEN 1 AND 64),
 instance_id TEXT CHECK(instance_id IS NULL OR length(CAST(instance_id AS BLOB)) BETWEEN 1 AND 256),
 operator_approved INTEGER NOT NULL CHECK(operator_approved=1),
 provider_stop_acknowledged INTEGER NOT NULL CHECK(provider_stop_acknowledged=1),
 payload_json TEXT NOT NULL CHECK(length(CAST(payload_json AS BLOB)) BETWEEN 2 AND 4096),
 payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64 AND payload_sha256 NOT GLOB '*[^0-9a-f]*'),
 created REAL NOT NULL,
 CHECK((operation='load_absent' AND model_key IS NOT NULL AND model_type IS NOT NULL AND context_length IS NOT NULL AND receipt_id IS NULL AND instance_id IS NULL)
    OR (operation='release_absent' AND model_key IS NULL AND model_type IS NULL AND context_length IS NULL AND receipt_id IS NOT NULL AND instance_id IS NOT NULL))
);
CREATE INDEX native_task_lifecycle_history ON events(kind,source,id) WHERE kind='native_task_lifecycle' AND source='human-cli';
CREATE INDEX managed_residency_resolution_task_endpoint ON managed_residency_resolutions(original_task_id,endpoint);
CREATE TRIGGER managed_residency_resolution_no_update
BEFORE UPDATE ON managed_residency_resolutions BEGIN
 SELECT RAISE(ABORT,'managed residency resolutions are append-only');
END;
CREATE TRIGGER managed_residency_resolution_no_delete
BEFORE DELETE ON managed_residency_resolutions BEGIN
 SELECT RAISE(ABORT,'managed residency resolutions are append-only');
END;

CREATE TABLE managed_residency_resolution_members (
 resolution_id TEXT NOT NULL REFERENCES managed_residency_resolutions(resolution_id) ON DELETE RESTRICT,
 receipt_id TEXT NOT NULL,
 member_json TEXT NOT NULL CHECK(length(CAST(member_json AS BLOB)) BETWEEN 2 AND 16384),
 member_sha256 TEXT NOT NULL CHECK(length(member_sha256)=64 AND member_sha256 NOT GLOB '*[^0-9a-f]*'),
 PRIMARY KEY(resolution_id,receipt_id)
);
CREATE INDEX managed_resolution_member_receipt ON managed_residency_resolution_members(receipt_id,resolution_id);
CREATE TRIGGER managed_resolution_member_no_update BEFORE UPDATE ON managed_residency_resolution_members BEGIN
 SELECT RAISE(ABORT,'resolution members are immutable'); END;
CREATE TRIGGER managed_resolution_member_no_delete BEFORE DELETE ON managed_residency_resolution_members BEGIN
 SELECT RAISE(ABORT,'resolution members are immutable'); END;

CREATE TABLE managed_residency_receipt_archive (
 receipt_id TEXT PRIMARY KEY CHECK(length(CAST(receipt_id AS BLOB)) BETWEEN 1 AND 64),
 endpoint TEXT NOT NULL CHECK(length(CAST(endpoint AS BLOB)) BETWEEN 1 AND 512),
 instance_id TEXT NOT NULL CHECK(length(CAST(instance_id AS BLOB)) BETWEEN 1 AND 256),
 receipt_json TEXT NOT NULL CHECK(length(CAST(receipt_json AS BLOB)) BETWEEN 2 AND 8192),
 receipt_sha256 TEXT NOT NULL CHECK(length(receipt_sha256)=64 AND receipt_sha256 NOT GLOB '*[^0-9a-f]*'),
 current_inventory_sha256 TEXT NOT NULL CHECK(length(current_inventory_sha256)=64 AND current_inventory_sha256 NOT GLOB '*[^0-9a-f]*'),
 state TEXT NOT NULL CHECK(state IN ('released','retired')),
 load_task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
 release_task_id TEXT REFERENCES tasks(id) ON DELETE RESTRICT,
 created REAL NOT NULL,
 updated REAL NOT NULL,
 archived_at REAL NOT NULL,
 retirement_task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
 archive_sha256 TEXT NOT NULL CHECK(length(archive_sha256)=64 AND archive_sha256 NOT GLOB '*[^0-9a-f]*'),
 CHECK(state!='released' OR release_task_id IS NOT NULL)
);
CREATE INDEX managed_residency_archive_endpoint ON managed_residency_receipt_archive(endpoint,receipt_id);
CREATE INDEX managed_residency_archive_load_task ON managed_residency_receipt_archive(load_task_id);
CREATE INDEX managed_residency_archive_release_task ON managed_residency_receipt_archive(release_task_id);
CREATE TRIGGER managed_residency_archive_no_hot_duplicate
BEFORE INSERT ON managed_residency_receipt_archive
WHEN EXISTS(SELECT 1 FROM managed_residency_receipts WHERE receipt_id=NEW.receipt_id)
BEGIN SELECT RAISE(ABORT,'managed residency receipt already exists in hot storage'); END;
CREATE TRIGGER managed_residency_hot_no_archive_duplicate
BEFORE INSERT ON managed_residency_receipts
WHEN EXISTS(SELECT 1 FROM managed_residency_receipt_archive WHERE receipt_id=NEW.receipt_id)
BEGIN SELECT RAISE(ABORT,'managed residency receipt already exists in archive'); END;
CREATE TRIGGER managed_residency_archive_no_update
BEFORE UPDATE ON managed_residency_receipt_archive BEGIN
 SELECT RAISE(ABORT,'managed residency receipt archive is immutable');
END;
CREATE TRIGGER managed_residency_archive_no_delete
BEFORE DELETE ON managed_residency_receipt_archive BEGIN
 SELECT RAISE(ABORT,'managed residency receipt archive is immutable');
END;
PRAGMA user_version=8;
"#;

const PROJECT_PROFILE_SCHEMA_V9: &str = r#"
CREATE TABLE project_profiles (
 id TEXT NOT NULL CHECK(length(CAST(id AS BLOB)) BETWEEN 3 AND 100),
 version INTEGER NOT NULL CHECK(version BETWEEN 1 AND 32),
 format_version INTEGER NOT NULL CHECK(format_version=1),
 name TEXT NOT NULL CHECK(length(CAST(name AS BLOB)) BETWEEN 1 AND 120),
 goal TEXT NOT NULL CHECK(length(CAST(goal AS BLOB)) BETWEEN 1 AND 2000),
 preferences_json TEXT NOT NULL CHECK(length(CAST(preferences_json AS BLOB)) BETWEEN 2 AND 128),
 checksum TEXT NOT NULL CHECK(length(checksum)=64 AND checksum NOT GLOB '*[^0-9a-f]*'),
 created REAL NOT NULL,
 PRIMARY KEY(id,version)
);
CREATE INDEX project_profiles_inventory ON project_profiles(id,version);
CREATE TRIGGER project_profiles_no_update BEFORE UPDATE ON project_profiles BEGIN
 SELECT RAISE(ABORT,'project profile versions are immutable');
END;
CREATE TRIGGER project_profiles_no_delete BEFORE DELETE ON project_profiles BEGIN
 SELECT RAISE(ABORT,'project profile versions are immutable');
END;
PRAGMA user_version=9;
"#;

const AGENT_SESSIONS_SCHEMA_V10: &str = r#"
CREATE TABLE agent_sessions (
 id TEXT PRIMARY KEY CHECK(length(CAST(id AS BLOB))=36),
 task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id) ON DELETE CASCADE,
 state TEXT NOT NULL CHECK(state IN ('active','waiting','closed','failed')),
 generation INTEGER NOT NULL CHECK(generation BETWEEN 0 AND 64),
 claim TEXT NOT NULL CHECK(length(CAST(claim AS BLOB))=36),
 snapshot TEXT NOT NULL CHECK(length(CAST(snapshot AS BLOB)) BETWEEN 2 AND 32768),
 snapshot_sha256 TEXT NOT NULL CHECK(length(CAST(snapshot_sha256 AS BLOB))=64 AND snapshot_sha256 NOT GLOB '*[^0-9a-f]*'),
 updated REAL NOT NULL
);
CREATE INDEX agent_sessions_waiting ON agent_sessions(state,updated,id);
CREATE TABLE agent_session_calls (
 id TEXT PRIMARY KEY CHECK(length(CAST(id AS BLOB))=36),
 session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
 generation INTEGER NOT NULL CHECK(generation BETWEEN 0 AND 64),
 call_sha256 TEXT NOT NULL CHECK(length(CAST(call_sha256 AS BLOB))=64 AND call_sha256 NOT GLOB '*[^0-9a-f]*')
);
CREATE INDEX agent_session_calls_generation ON agent_session_calls(session_id,generation);
PRAGMA user_version=10;
"#;

const PROJECT_MEMORY_SCHEMA_V11: &str = r#"
CREATE TABLE project_memory_records (
 id TEXT PRIMARY KEY CHECK(length(CAST(id AS BLOB))=36),
 project_id TEXT NOT NULL,
 profile_version INTEGER NOT NULL CHECK(profile_version BETWEEN 1 AND 32),
 revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 32),
 format_version INTEGER NOT NULL CHECK(format_version=1),
 record_json TEXT NOT NULL CHECK(length(CAST(record_json AS BLOB)) BETWEEN 2 AND 16384),
 record_sha256 TEXT NOT NULL CHECK(length(record_sha256)=64 AND record_sha256 NOT GLOB '*[^0-9a-f]*'),
 profile_sha256 TEXT NOT NULL CHECK(length(profile_sha256)=64 AND profile_sha256 NOT GLOB '*[^0-9a-f]*'),
 publisher TEXT NOT NULL CHECK(publisher='human-cli'),
 created REAL NOT NULL,
 UNIQUE(project_id,profile_version,revision),
 FOREIGN KEY(project_id,profile_version) REFERENCES project_profiles(id,version) ON DELETE RESTRICT
);
CREATE INDEX project_memory_project_revision ON project_memory_records(project_id,profile_version,revision);
CREATE TRIGGER project_memory_records_no_update BEFORE UPDATE ON project_memory_records BEGIN
 SELECT RAISE(ABORT,'project memory revisions are immutable');
END;
CREATE TRIGGER project_memory_records_no_delete BEFORE DELETE ON project_memory_records BEGIN
 SELECT RAISE(ABORT,'project memory revisions are immutable');
END;
PRAGMA user_version=11;
"#;

pub(crate) const MAX_MANAGED_RESIDENCY_RESOLUTIONS: u64 = 1024;
pub(crate) const MAX_MANAGED_RESIDENCY_ARCHIVE: u64 = 8192;

pub(crate) fn admit_managed_residency_resolution(db: &Connection) -> Result<()> {
    let rows: u64 = db.query_row(
        "SELECT count(*) FROM managed_residency_resolutions",
        [],
        |r| r.get(0),
    )?;
    if rows >= MAX_MANAGED_RESIDENCY_RESOLUTIONS {
        bail!("Managed residency resolution history is full")
    }
    Ok(())
}

pub(crate) fn admit_managed_residency_archive(db: &Connection) -> Result<()> {
    let rows: u64 = db.query_row(
        "SELECT count(*) FROM managed_residency_receipt_archive",
        [],
        |r| r.get(0),
    )?;
    if rows >= MAX_MANAGED_RESIDENCY_ARCHIVE {
        bail!("Managed residency receipt archive is full")
    }
    Ok(())
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn project_profile_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<crate::projects::ProjectProfile> {
    let version: i64 = row.get(1)?;
    let format_version: i64 = row.get(2)?;
    let preferences_json: String = row.get(5)?;
    let preferences = serde_json::from_str(&preferences_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(crate::projects::ProjectProfile {
        id: row.get(0)?,
        version: u64::try_from(version)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, version))?,
        format_version: u16::try_from(format_version)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(2, format_version))?,
        name: row.get(3)?,
        goal: row.get(4)?,
        preferences,
        checksum: row.get(6)?,
    })
}

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub description: String,
    pub tags: Vec<String>,
    pub active_version: i64,
    pub enabled: bool,
    pub manifest_version: Option<String>,
}
pub const MAX_SKILLS: usize = 4096;

fn validate_skill_fields(
    id: &str,
    description: &str,
    tags: &[String],
    allow_empty_tags: bool,
) -> Result<()> {
    let mut unique = std::collections::BTreeSet::new();
    if !crate::registry::identifier(id)
        || description.trim().is_empty()
        || description.len() > 500
        || (!allow_empty_tags && tags.is_empty())
        || tags.len() > 32
        || tags.iter().any(|tag| {
            !(2..=40).contains(&tag.len())
                || !tag
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_+-".contains(&b))
                || !unique.insert(tag)
        })
    {
        bail!("Invalid skill identity, description or tags")
    }
    Ok(())
}

pub(crate) fn admit_skill_id(db: &Connection, id: &str) -> Result<()> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM skills WHERE id=?1)",
        [id],
        |r| r.get(0),
    )?;
    if !exists {
        let count: u64 = db.query_row("SELECT count(*) FROM skills", [], |r| r.get(0))?;
        if count >= MAX_SKILLS as u64 {
            bail!("Skill identity capacity exhausted")
        }
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub skill_id: String,
    pub title: String,
    pub body: String,
    pub source: String,
    pub license: String,
}

pub struct TaskContext {
    pub memories: Vec<crate::memory::Memory>,
    pub skills: Vec<Skill>,
    pub documents: Vec<Document>,
    pub manifests: Vec<crate::registry::Manifest>,
    pub expert_models: std::collections::BTreeMap<String, crate::models::Registered>,
}

pub(crate) struct RagBaseline {
    pub candidate_id: String,
    pub version: i64,
    pub body: String,
    pub dataset_content: String,
    pub source: String,
    pub license: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub skill_id: String,
    pub kind: String,
    pub version: i64,
    pub artifact: String,
    pub checksum: String,
    pub status: String,
    pub source: String,
    pub license: String,
}

/// The event is durable, but its observer notification failed.
#[derive(Debug)]
pub(crate) struct PersistedEventError(anyhow::Error);
impl std::fmt::Display for PersistedEventError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Persisted event notification failed: {}", self.0)
    }
}
impl std::error::Error for PersistedEventError {}

pub struct Store {
    pub(crate) events: crate::events::Hub,
    pub(crate) runtime_lock: Option<std::path::PathBuf>,
    db: Mutex<Connection>,
}
impl Store {
    pub(crate) fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| anyhow!("Database mutex poisoned"))
    }
    /// Open an existing current-schema database without migrations or bootstrap writes.
    pub fn open_existing_read_only(path: &Path) -> Result<Self> {
        if path.is_symlink() {
            bail!("Database path cannot be a symlink")
        }
        let db = crate::storage::wal_vfs::open(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version != CURRENT_SCHEMA_VERSION {
            bail!("Read-only inspection requires the current database schema")
        }
        let mut store = Self::from_frozen_connection(db)?;
        let mut lease = path.as_os_str().to_os_string();
        lease.push(".runtime.lock");
        store.runtime_lock = Some(lease.into());
        Ok(store)
    }
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_limit(path, crate::storage::DEFAULT_DATABASE_BYTES)
    }
    /// Open a current runtime database for mutation without creating or migrating it.
    pub fn open_existing_with_limit(path: &Path, maximum_bytes: u64) -> Result<Self> {
        let db = crate::storage::wal_vfs::open(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version != CURRENT_SCHEMA_VERSION {
            bail!("Runtime continuation requires an existing current-schema database")
        }
        crate::storage::configure(&db, maximum_bytes)?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        let mut lease = path.as_os_str().to_os_string();
        lease.push(".runtime.lock");
        Ok(Self {
            db: Mutex::new(db),
            runtime_lock: Some(lease.into()),
            events: crate::events::Hub::default(),
        })
    }
    pub fn open_with_limit(path: &Path, maximum_bytes: u64) -> Result<Self> {
        if path.is_symlink() {
            bail!("Database path cannot be a symlink")
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut store = Self::initialize(
            crate::storage::wal_vfs::open(path, rusqlite::OpenFlags::default())?,
            maximum_bytes,
        )?;
        let mut lease = path.as_os_str().to_os_string();
        lease.push(".runtime.lock");
        store.runtime_lock = Some(lease.into());
        Ok(store)
    }
    pub fn in_memory() -> Result<Self> {
        Self::initialize(
            Connection::open_in_memory()?,
            crate::storage::DEFAULT_DATABASE_BYTES,
        )
    }
    pub(crate) fn migrate_snapshot(
        path: &Path,
        maximum: u64,
        started: std::time::Instant,
    ) -> Result<()> {
        let db = crate::storage::wal_vfs::open(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.pragma_update(None, "trusted_schema", false)?;
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > std::time::Duration::from_secs(10)),
        );
        let store = Self::initialize(db, maximum)?;
        let db = store.lock()?;
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")?;
        let check: String = db.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
        if check != "ok" {
            bail!("Migrated snapshot failed consistency checking")
        }
        Ok(())
    }
    fn initialize(mut db: Connection, maximum_bytes: u64) -> Result<Self> {
        crate::storage::configure(&db, maximum_bytes)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > CURRENT_SCHEMA_VERSION {
            bail!("Database schema is newer than this runtime")
        }
        if version == 0 {
            tx.execute_batch(SCHEMA)?;
            tx.execute_batch(
                r#"
                CREATE TABLE candidate_documents (
                    candidate_id TEXT PRIMARY KEY REFERENCES candidates(id),
                    document_id TEXT NOT NULL REFERENCES documents(id)
                );
                CREATE INDEX candidate_documents_document ON candidate_documents(document_id);
                CREATE TABLE lifecycle_audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    skill_id TEXT NOT NULL REFERENCES skills(id),
                    candidate_id TEXT NOT NULL REFERENCES candidates(id),
                    action TEXT NOT NULL, previous_version INTEGER NOT NULL,
                    active_version INTEGER NOT NULL, created REAL NOT NULL
                );
                CREATE TRIGGER IF NOT EXISTS documents_delete AFTER DELETE ON documents BEGIN
                    INSERT INTO document_index(document_index,rowid,title,body)
                    VALUES ('delete',old.rowid,old.title,old.body);
                END;
                CREATE TRIGGER IF NOT EXISTS documents_update AFTER UPDATE ON documents BEGIN
                    INSERT INTO document_index(document_index,rowid,title,body)
                    VALUES ('delete',old.rowid,old.title,old.body);
                    INSERT INTO document_index(rowid,title,body) VALUES(new.rowid,new.title,new.body);
                END;
                INSERT INTO candidate_documents(candidate_id,document_id)
                    SELECT c.id,d.id FROM candidates c JOIN documents d
                    ON c.checksum=d.sha256 AND c.skill_id=d.skill_id
                    WHERE c.status='approved' AND c.kind='rag';
                PRAGMA user_version=1;
            "#,
            )?;
        }
        if version < 2 {
            tx.execute_batch(r#"
                ALTER TABLE skills ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
                CREATE TABLE expert_manifests (
                    skill_id TEXT NOT NULL REFERENCES skills(id),
                    version TEXT NOT NULL, manifest TEXT NOT NULL, checksum TEXT NOT NULL,
                    active INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(skill_id,version)
                );
                CREATE UNIQUE INDEX expert_active_version ON expert_manifests(skill_id) WHERE active=1;
                CREATE TABLE expert_audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, skill_id TEXT NOT NULL,
                    version TEXT, action TEXT NOT NULL, created REAL NOT NULL
                );
                PRAGMA user_version=2;
            "#)?;
        }
        if version < 3 {
            tx.execute_batch(
                r#"
                CREATE TABLE evaluation_datasets (
                    checksum TEXT PRIMARY KEY, content TEXT NOT NULL,
                    source TEXT NOT NULL, license TEXT NOT NULL,
                    split TEXT NOT NULL CHECK(split='held_out'), created REAL NOT NULL
                );
                PRAGMA user_version=3;
            "#,
            )?;
        }
        if version < 4 {
            tx.execute_batch(crate::memory::SCHEMA)?;
        }
        if version < 5 {
            tx.execute_batch(crate::models::SCHEMA)?;
        }
        if version < 6 {
            tx.execute_batch(crate::storage::SCHEMA)?;
        }
        if version < 7 {
            tx.execute_batch(MANAGED_RESIDENCY_SCHEMA)?;
        }
        if version < 8 {
            tx.execute_batch(MANAGED_RESIDENCY_SCHEMA_V8)?;
        }
        if version < 9 {
            tx.execute_batch(PROJECT_PROFILE_SCHEMA_V9)?;
        }
        if version < 10 {
            tx.execute_batch(AGENT_SESSIONS_SCHEMA_V10)?;
        }
        if version < 11 {
            tx.execute_batch(PROJECT_MEMORY_SCHEMA_V11)?;
        }
        tx.commit()?;
        Ok(Self {
            db: Mutex::new(db),
            runtime_lock: None,
            events: crate::events::Hub::default(),
        })
    }
    pub(crate) fn from_frozen_connection(db: Connection) -> Result<Self> {
        db.pragma_update(None, "query_only", true)?;
        Ok(Self {
            db: Mutex::new(db),
            runtime_lock: None,
            events: crate::events::Hub::default(),
        })
    }
    /// Publish a new immutable version of an operator-managed project profile.
    /// The untrusted project goal never changes task permissions or authority.
    pub fn publish_project(
        &self,
        draft: crate::projects::ProjectDraft,
    ) -> Result<crate::projects::ProjectProfile> {
        crate::security::authorize("human-cli", "project.publish")?;
        draft.validate()?;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        crate::storage::admit(&tx)?;
        let (total_profiles, registered_projects): (u64, u64) = tx.query_row(
            "SELECT (SELECT count(*) FROM (SELECT 1 FROM project_profiles LIMIT ?1)),(SELECT count(*) FROM (SELECT DISTINCT id FROM project_profiles ORDER BY id LIMIT ?2))",
            params![crate::projects::MAX_PROJECTS as u64 * crate::projects::MAX_VERSIONS + 1, crate::projects::MAX_PROJECTS as u64 + 1],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let total_limit = crate::projects::MAX_PROJECTS as u64 * crate::projects::MAX_VERSIONS;
        if total_profiles > total_limit
            || registered_projects > crate::projects::MAX_PROJECTS as u64
        {
            bail!("Stored project profile registry exceeds its bounds")
        }
        if total_profiles >= total_limit {
            bail!("Project profile registry is full")
        }
        let (count, maximum): (u64, Option<u64>) = tx.query_row(
            "SELECT count(*),max(version) FROM (SELECT version FROM project_profiles WHERE id=?1 ORDER BY version DESC LIMIT ?2)",
            params![draft.id, crate::projects::MAX_VERSIONS + 1],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let version = count
            .checked_add(1)
            .ok_or_else(|| anyhow!("Project profile version overflow"))?;
        if count >= crate::projects::MAX_VERSIONS || maximum.is_some_and(|current| current != count)
        {
            bail!("Project profile version history is full or inconsistent")
        }
        if count == 0 && registered_projects >= crate::projects::MAX_PROJECTS as u64 {
            bail!("Project profile registry is full")
        }
        let profile = crate::projects::ProjectProfile::from_draft(draft, version)?;
        let preferences = serde_json::to_string(&profile.preferences)?;
        tx.execute(
            "INSERT INTO project_profiles(id,version,format_version,name,goal,preferences_json,checksum,created) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![profile.id,profile.version,profile.format_version,profile.name,profile.goal,preferences,profile.checksum,now()],
        )?;
        tx.commit()?;
        Ok(profile)
    }

    /// Read one explicit immutable project profile version (`id@version`).
    pub fn project_profile(&self, reference: &str) -> Result<crate::projects::ProjectProfile> {
        crate::security::authorize("human-cli", "project.read")?;
        self.project_profile_for_runtime(reference)
    }

    /// Resolve an explicitly selected immutable profile for task construction.
    /// This trusted runtime read grants no actor capability; management reads use
    /// the human-only `project_profile` API above.
    pub(crate) fn project_profile_for_runtime(
        &self,
        reference: &str,
    ) -> Result<crate::projects::ProjectProfile> {
        let db = self.lock()?;
        Self::project_profile_from(&db, reference)
    }

    /// Resolve an immutable project version from an existing transaction so a
    /// client session can pin profile context with its opening retrieval view.
    pub(crate) fn project_profile_from(
        db: &Connection,
        reference: &str,
    ) -> Result<crate::projects::ProjectProfile> {
        let selection = crate::projects::Selection::parse(reference)?;
        let profile = db.query_row(
                "SELECT CASE WHEN length(CAST(id AS BLOB))<=100 THEN id END,version,format_version,CASE WHEN length(CAST(name AS BLOB))<=120 THEN name END,CASE WHEN length(CAST(goal AS BLOB))<=2000 THEN goal END,CASE WHEN length(CAST(preferences_json AS BLOB))<=128 THEN preferences_json END,CASE WHEN length(CAST(checksum AS BLOB))=64 THEN checksum END FROM project_profiles WHERE id=?1 AND version=?2",
                params![selection.id(), selection.version()],
                project_profile_from_row,
            )
            .optional()?
            .ok_or_else(|| anyhow!("Project profile version not found"))?;
        profile.validate()?;
        Ok(profile)
    }

    /// Return the bounded, fully validated project profile inventory.
    pub fn projects(&self) -> Result<Vec<crate::projects::ProjectProfile>> {
        crate::security::authorize("human-cli", "project.read")?;
        let db = self.lock()?;
        let project_count: u64 = db.query_row(
            "SELECT count(*) FROM (SELECT DISTINCT id FROM project_profiles ORDER BY id LIMIT ?1)",
            [crate::projects::MAX_PROJECTS as u64 + 1],
            |row| row.get(0),
        )?;
        if project_count > crate::projects::MAX_PROJECTS as u64 {
            bail!("Stored project profile registry exceeds its bound")
        }
        let maximum = crate::projects::MAX_PROJECTS * crate::projects::MAX_VERSIONS as usize;
        let mut statement = db.prepare(
            "SELECT CASE WHEN length(CAST(id AS BLOB))<=100 THEN id END,version,format_version,CASE WHEN length(CAST(name AS BLOB))<=120 THEN name END,CASE WHEN length(CAST(goal AS BLOB))<=2000 THEN goal END,CASE WHEN length(CAST(preferences_json AS BLOB))<=128 THEN preferences_json END,CASE WHEN length(CAST(checksum AS BLOB))=64 THEN checksum END FROM project_profiles ORDER BY id,version LIMIT ?1",
        )?;
        let mut rows = statement.query([(maximum + 1) as i64])?;
        let mut profiles = Vec::new();
        while let Some(row) = rows.next()? {
            if profiles.len() >= maximum {
                bail!("Stored project profile inventory exceeds its bound")
            }
            let profile = project_profile_from_row(row)?;
            profile.validate()?;
            profiles.push(profile);
        }
        Ok(profiles)
    }

    /// Publish one immutable, schema-validated project-memory revision.
    pub fn publish_project_memory(
        &self,
        record: &crate::project_memory::ProjectMemoryRecord,
    ) -> Result<crate::project_memory::StoredProjectMemory> {
        crate::security::authorize("human-cli", "project.publish")?;
        record.validate()?;
        if record.revision > crate::project_memory::MAX_REVISIONS_PER_PROJECT {
            bail!("Project memory revision history is full")
        }
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let profile = Self::project_profile_from(&tx, &record.project_ref)?;
        let selection = crate::projects::Selection::parse(&record.project_ref)?;
        let stored =
            crate::project_memory::StoredProjectMemory::new(record.clone(), profile.checksum)?;
        let payload = stored.record.encode()?;
        let existing: Option<(String, u64, u32, u16, String, String, String)> = tx
            .query_row(
                "SELECT project_id,profile_version,revision,format_version,record_json,record_sha256,profile_sha256 \
                 FROM project_memory_records WHERE id=?1",
                [&record.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        if let Some((id, version, revision, format, bytes, digest, profile_digest)) = existing {
            if id != selection.id()
                || version != selection.version()
                || revision != record.revision
                || format != record.format_version
                || bytes != payload
                || digest != sha256(payload.as_bytes())
                || profile_digest != stored.provenance.project_profile_sha256
            {
                bail!("Project memory identity already exists with different canonical bytes")
            }
            return Ok(stored);
        }
        let total: u64 =
            tx.query_row("SELECT count(*) FROM project_memory_records", [], |row| {
                row.get(0)
            })?;
        let prior_revisions: u64 = tx.query_row(
            "SELECT count(*) FROM project_memory_records WHERE project_id=?1 AND profile_version=?2",
            rusqlite::params![selection.id(), selection.version()],
            |row| row.get(0),
        )?;
        if total >= crate::project_memory::MAX_TOTAL_RECORDS
            || prior_revisions >= crate::project_memory::MAX_REVISIONS_PER_PROJECT as u64
            || record.revision != prior_revisions as u32 + 1
        {
            bail!("Project memory requires the next bounded revision")
        }
        tx.execute(
            "INSERT INTO project_memory_records(id,project_id,profile_version,revision,format_version,record_json,record_sha256,profile_sha256,publisher,created) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'human-cli',?9)",
            rusqlite::params![
                record.id,
                selection.id(),
                selection.version(),
                record.revision,
                record.format_version,
                payload,
                sha256(payload.as_bytes()),
                stored.provenance.project_profile_sha256,
                now(),
            ],
        )?;
        tx.commit()?;
        Ok(stored)
    }

    /// Read bounded, canonical memory revisions for one exact project profile.
    pub fn project_memories(
        &self,
        project_ref: &str,
    ) -> Result<Vec<crate::project_memory::StoredProjectMemory>> {
        crate::security::authorize("human-cli", "project.read")?;
        let db = self.lock()?;
        Self::project_memories_from(&db, project_ref)
    }

    /// Resolve exact project-memory revisions from a caller's opening transaction.
    /// The caller owns the snapshot; this helper never opens another connection.
    pub(crate) fn project_memories_from(
        db: &Connection,
        project_ref: &str,
    ) -> Result<Vec<crate::project_memory::StoredProjectMemory>> {
        let profile = Self::project_profile_from(db, project_ref)?;
        let selection = crate::projects::Selection::parse(project_ref)?;
        let mut statement = db.prepare(
            "SELECT id,CASE WHEN length(CAST(record_json AS BLOB))<=?3 THEN record_json END, \
                    CASE WHEN length(CAST(record_sha256 AS BLOB))=64 THEN record_sha256 END, \
                    CASE WHEN length(CAST(profile_sha256 AS BLOB))=64 THEN profile_sha256 END, \
                    revision,format_version,publisher \
             FROM project_memory_records WHERE project_id=?1 AND profile_version=?2 \
             ORDER BY revision LIMIT ?4",
        )?;
        let mut rows = statement.query_map(
            rusqlite::params![
                selection.id(),
                selection.version(),
                crate::project_memory::MAX_RECORD_BYTES,
                crate::project_memory::MAX_REVISIONS_PER_PROJECT as i64 + 1,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u16>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )?;
        let mut records = Vec::new();
        for row in rows.by_ref() {
            let (
                stored_id,
                payload,
                record_hash,
                profile_hash,
                revision,
                format_version,
                publisher,
            ) = row?;
            let payload =
                payload.ok_or_else(|| anyhow!("Stored project memory exceeds its byte bound"))?;
            let record_hash =
                record_hash.ok_or_else(|| anyhow!("Stored project memory digest is malformed"))?;
            let profile_hash = profile_hash
                .ok_or_else(|| anyhow!("Stored project profile digest is malformed"))?;
            if sha256(payload.as_bytes()) != record_hash
                || profile_hash != profile.checksum
                || publisher != "human-cli"
                || format_version != crate::project_memory::FORMAT_VERSION
                || revision != records.len() as u32 + 1
            {
                bail!("Stored project memory provenance, digest, or revision chain is inconsistent")
            }
            let record = crate::project_memory::ProjectMemoryRecord::decode(&payload)?;
            if record.id != stored_id
                || record.project_ref != project_ref
                || record.revision != revision
            {
                bail!("Stored project memory record identity does not match its storage key")
            }
            records.push(crate::project_memory::StoredProjectMemory::new(
                record,
                profile_hash,
            )?);
            if records.len() > crate::project_memory::MAX_REVISIONS_PER_PROJECT as usize {
                bail!("Stored project memory revision history exceeds its bound")
            }
        }
        Ok(records)
    }

    /// Read one exact latest project-memory revision from a caller's transaction.
    pub(crate) fn latest_project_memory_from(
        db: &Connection,
        project_ref: &str,
    ) -> Result<Option<crate::project_memory::StoredProjectMemory>> {
        Ok(Self::project_memories_from(db, project_ref)?.pop())
    }

    pub fn create_task(&self, query: &str, mode: &str) -> Result<String> {
        self.create_named_task(&Uuid::new_v4().to_string(), query, mode)
    }
    pub(crate) fn create_named_task(&self, id: &str, query: &str, mode: &str) -> Result<String> {
        Uuid::parse_str(id)?;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        crate::storage::admit(&tx)?;
        tx.execute(
            "INSERT INTO tasks VALUES (?1,?2,?3,'running','',?4,?4)",
            params![id, query, mode, now()],
        )?;
        tx.commit()?;
        Ok(id.to_string())
    }
    pub fn finish_task(&self, id: &str, status: &str, answer: &str) -> Result<()> {
        self.lock()?.execute(
            "UPDATE tasks SET status=?2,answer=?3,updated=?4 WHERE id=?1",
            params![id, status, answer, now()],
        )?;
        Ok(())
    }
    pub fn event(&self, id: &str, kind: &str, source: &str, payload: Value) -> Result<()> {
        let raw = payload.to_string();
        if raw.len() > 262144 || kind.len() > 64 || source.len() > 64 {
            bail!("Audit event exceeds bounds")
        }
        let mut progress = crate::events::Progress::from_audit(1, id, kind, source, &payload)?;
        let db = self.lock()?;
        db.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,?2,?3,?4,?5)",
            params![id, kind, source, raw, now()],
        )?;
        if let Some(event) = &mut progress {
            event.sequence = db.last_insert_rowid() as u64;
            self.events.publish(event).map_err(PersistedEventError)?;
        }
        Ok(())
    }
    pub(crate) fn worker_audit(&self, anchor: &str, payload: Value) -> Result<()> {
        let raw = payload.to_string();
        if raw.len() > 4096 {
            bail!("Worker audit exceeds metadata bounds")
        }
        // The existence check and insert are one statement, preventing retention
        // from deleting the anchor between validation and recording.
        let inserted = self.lock()?.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) SELECT id,'worker.audit','fabric',?2,?3 FROM tasks WHERE id=?1",
            params![anchor, raw, now()],
        )?;
        if inserted != 1 {
            bail!("Worker audit anchor no longer exists")
        }
        Ok(())
    }
    pub fn task(&self, id: &str) -> Result<Value> {
        let db = self.lock()?;
        let task:Option<Value>=db.query_row("SELECT CASE WHEN length(CAST(id AS BLOB))<=64 THEN id END,CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END,CASE WHEN length(CAST(mode AS BLOB))<=32 THEN mode END,CASE WHEN length(CAST(status AS BLOB))<=64 THEN status END,CASE WHEN length(CAST(answer AS BLOB))<=262144 THEN answer END FROM tasks WHERE id=?1",[id],|r| {
            Ok(json!({"id":r.get::<_,String>(0)?,"query":r.get::<_,String>(1)?,"mode":r.get::<_,String>(2)?,
                "status":r.get::<_,String>(3)?,"answer":r.get::<_,String>(4)?}))
        }).optional()?;
        let mut stmt = db.prepare("SELECT id,CASE WHEN length(CAST(kind AS BLOB))<=64 THEN kind END,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 ORDER BY id LIMIT 4097")?;
        let mut rows = stmt.query([id])?;
        let mut events = Vec::new();
        let mut bytes = 0usize;
        while let Some(row) = rows.next()? {
            if events.len() >= 4096 {
                bail!("Task trace exceeds event limit; use paged task-events")
            }
            let payload: String = row.get(3)?;
            bytes += payload.len();
            if bytes > 8_000_000 {
                bail!("Task trace exceeds byte limit; use paged task-events")
            }
            events.push(json!({"sequence":row.get::<_,u64>(0)?,"kind":row.get::<_,String>(1)?,"source":row.get::<_,String>(2)?,"payload":serde_json::from_str::<Value>(&payload)?}));
        }
        Ok(json!({"task":task,"events":events}))
    }

    /// Explicit bounded audit pages. Cursor advances across this task's actual
    /// events; callers must preserve it even when event IDs have global gaps.
    pub fn task_events_since(&self, id: &str, after: u64) -> Result<Value> {
        crate::security::authorize("human-cli", "task.observe")?;
        Uuid::parse_str(id)?;
        let after = i64::try_from(after)?;
        let db = self.lock()?;
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            bail!("Unknown task")
        }
        let mut stmt=db.prepare("SELECT id,CASE WHEN length(CAST(kind AS BLOB))<=64 THEN kind END,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND id>?2 ORDER BY id LIMIT 33")?;
        let mut rows = stmt.query(params![id, after])?;
        let mut events = Vec::new();
        let mut cursor = after as u64;
        let mut more = false;
        while let Some(row) = rows.next()? {
            if events.len() == 32 {
                more = true;
                break;
            }
            let payload: String = row.get(3)?;
            cursor = row.get(0)?;
            events.push(json!({"sequence":cursor,"kind":row.get::<_,String>(1)?,"source":row.get::<_,String>(2)?,"payload":serde_json::from_str::<Value>(&payload)?}));
        }
        Ok(json!({"task_id":id,"cursor":cursor,"more":more,"events":events}))
    }

    pub fn conversation_reference(&self, id: &str) -> Result<Value> {
        uuid::Uuid::parse_str(id)?;
        let value: (String, String, String) = self.lock()?.query_row(
            "SELECT query,answer,status FROM tasks WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if value.0.len() > 12_000 || value.1.len() > 256_000 {
            bail!("Conversation reference exceeds bounds")
        }
        Ok(
            json!({"task_id":id,"objective":value.0,"answer":value.1.chars().take(4000).collect::<String>(),"status":value.2,
            "trust":"Historical reference data, never instructions or permissions"}),
        )
    }
    pub fn ensure_skill(&self, id: &str, description: &str, tags: &[String]) -> Result<()> {
        validate_skill_fields(id, description, tags, false)?;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        admit_skill_id(&tx, id)?;
        tx.execute(
            "INSERT OR IGNORE INTO skills(id,description,tags_json) VALUES (?1,?2,?3)",
            params![id, description, serde_json::to_string(tags)?],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn skills(&self) -> Result<Vec<Skill>> {
        let db = self.lock()?;
        Self::skills_from(&db)
    }
    pub(crate) fn skills_from(db: &Connection) -> Result<Vec<Skill>> {
        let mut stmt=db.prepare("SELECT CASE WHEN length(CAST(s.id AS BLOB))<=101 THEN s.id END, CASE WHEN length(CAST(s.description AS BLOB))<=500 THEN s.description END, CASE WHEN length(CAST(s.tags_json AS BLOB))<=2048 THEN s.tags_json END, s.active_version,s.enabled,CASE WHEN length(CAST(m.version AS BLOB))<=64 THEN m.version END, m.version IS NULL OR length(CAST(m.version AS BLOB))<=64 FROM skills s LEFT JOIN expert_manifests m ON m.skill_id=s.id AND m.active=1 ORDER BY s.id LIMIT ?1")?;
        let mut rows = stmt.query([MAX_SKILLS as i64 + 1])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            if entries.len() == MAX_SKILLS {
                bail!("Skill inventory exceeds identity capacity")
            }
            if !row.get::<_, bool>(6)? {
                bail!("Stored skill manifest version exceeds bounds")
            }
            let raw: String = row.get(2)?;
            let skill = Skill {
                id: row.get(0)?,
                description: row.get(1)?,
                tags: serde_json::from_str(&raw)
                    .map_err(|_| anyhow!("Invalid stored skill tag JSON"))?,
                active_version: row.get(3)?,
                enabled: match row.get::<_, i64>(4)? {
                    0 => false,
                    1 => true,
                    _ => bail!("Invalid stored skill enable state"),
                },
                manifest_version: row.get(5)?,
            };
            validate_skill_fields(&skill.id, &skill.description, &skill.tags, true)?;
            if skill.active_version < 0 {
                bail!("Invalid stored skill active version")
            }
            if let Some(version) = &skill.manifest_version {
                if semver::Version::parse(version)?.to_string() != *version {
                    bail!("Invalid stored skill manifest version")
                }
            }
            entries.push(skill);
        }
        Ok(entries)
    }
    pub fn add_document(&self, doc: &Document) -> Result<bool> {
        crate::security::validate_provenance(&doc.source, &doc.license)?;
        if doc.body.trim().is_empty()
            || doc.body.len() > 256_000
            || doc.title.len() > 500
            || doc.source.len() > 1000
            || doc.license.len() > 128
        {
            bail!("Document exceeds metadata or content bounds")
        }
        let result = self.lock()?.execute(
            "INSERT OR IGNORE INTO documents VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                doc.id,
                doc.skill_id,
                doc.title,
                doc.body,
                doc.source,
                doc.license,
                sha256(doc.body.as_bytes())
            ],
        )?;
        Ok(result != 0)
    }
    pub fn search(&self, query: &str, skill: Option<&str>, limit: usize) -> Result<Vec<Document>> {
        let db = self.lock()?;
        Self::search_from(&db, query, skill, limit)
    }

    /// Pin version metadata and retrieved content to one consistent SQLite snapshot.
    pub fn task_context(&self, query: &str) -> Result<TaskContext> {
        self.task_context_scoped(query, true, true)
    }

    pub(crate) fn task_context_scoped(
        &self,
        query: &str,
        retrieve: bool,
        experts: bool,
    ) -> Result<TaskContext> {
        Ok(self
            .task_context_with_references(query, retrieve, experts, false)?
            .0)
    }

    pub(crate) fn task_context_with_references(
        &self,
        query: &str,
        retrieve: bool,
        experts: bool,
        snapshot: bool,
    ) -> Result<(TaskContext, Option<crate::corpus::FrozenCorpus>)> {
        self.task_context_with_memory_policy(query, retrieve, experts, snapshot, true)
    }

    pub(crate) fn task_context_with_memory_policy(
        &self,
        query: &str,
        retrieve: bool,
        experts: bool,
        snapshot: bool,
        legacy_memories: bool,
    ) -> Result<(TaskContext, Option<crate::corpus::FrozenCorpus>)> {
        let mut db = self.lock()?;
        let tx = db.transaction()?;
        let manifests = crate::registry::snapshot_from(&tx)?;
        let mut expert_models = std::collections::BTreeMap::new();
        if experts {
            for manifest in &manifests {
                if let Some(selection) = &manifest.model {
                    if let Some(model) = crate::models::registered_from(&tx, selection)? {
                        if model.enabled && model.manifest.max_output_tokens >= 192 {
                            expert_models.insert(manifest.id.clone(), model);
                        }
                    }
                }
            }
        }
        let context = TaskContext {
            memories: if retrieve && legacy_memories {
                crate::memory::search_from(&tx, query)?
            } else {
                Vec::new()
            },
            skills: Self::skills_from(&tx)?,
            documents: if retrieve {
                Self::search_scoped(&tx, query, None, 3, experts)?
            } else {
                Vec::new()
            },
            manifests,
            expert_models,
        };
        let snapshot = if snapshot {
            crate::corpus::snapshot_from(&tx)?
        } else {
            None
        };
        tx.commit()?;
        Ok((context, snapshot))
    }

    /// Resolve client collective references against the caller's existing
    /// transaction so session creation and its pinned context share one
    /// consistent SQLite snapshot. Expert model registrations are deliberately
    /// excluded: this endpoint continues to use only its configured backend.
    pub(crate) fn client_context_from(db: &Connection, query: &str) -> Result<TaskContext> {
        Self::client_context_with_memory_policy(db, query, true)
    }

    pub(crate) fn client_context_with_memory_policy(
        db: &Connection,
        query: &str,
        legacy_memories: bool,
    ) -> Result<TaskContext> {
        Ok(TaskContext {
            memories: if legacy_memories {
                crate::memory::search_from(db, query)?
            } else {
                Vec::new()
            },
            skills: Self::skills_from(db)?,
            documents: Self::search_scoped(db, query, None, 3, true)?,
            manifests: crate::registry::snapshot_from(db)?,
            expert_models: std::collections::BTreeMap::new(),
        })
    }

    fn search_from(
        db: &Connection,
        query: &str,
        skill: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        Self::search_scoped(db, query, skill, limit, true)
    }

    fn search_scoped(
        db: &Connection,
        query: &str,
        skill: Option<&str>,
        limit: usize,
        experts: bool,
    ) -> Result<Vec<Document>> {
        let mut tokens = Vec::new();
        for token in query.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
            if token.len() >= 2 && token.len() <= 40 && tokens.len() < 8 {
                tokens.push(format!("\"{}\"", token))
            }
        }
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let match_query = tokens.join(" OR ");
        let statement = "SELECT d.id,d.skill_id,d.title,d.body,d.source,d.license,d.sha256 \
           FROM document_index JOIN documents d ON d.rowid=document_index.rowid \
           WHERE document_index MATCH ?1 AND (?2 IS NULL OR d.skill_id=?2) \
           AND EXISTS(SELECT 1 FROM skills enabled_skill WHERE enabled_skill.id=d.skill_id AND enabled_skill.enabled=1) \
           AND (?4=1 OR NOT EXISTS(SELECT 1 FROM candidate_documents memory_cd WHERE memory_cd.document_id=d.id)) \
           AND (NOT EXISTS(SELECT 1 FROM candidate_documents cd WHERE cd.document_id=d.id) \
                OR EXISTS(SELECT 1 FROM candidate_documents cd JOIN candidates c ON c.id=cd.candidate_id \
                    JOIN skills s ON s.id=c.skill_id WHERE cd.document_id=d.id \
                    AND c.status='approved' AND c.version=s.active_version)) \
           ORDER BY bm25(document_index) LIMIT ?3";
        let mut stmt = db.prepare(statement)?;
        let rows = stmt
            .query_map(
                params![match_query, skill, limit.clamp(1, 20) as i64, experts],
                |r| {
                    Ok((
                        Document {
                            id: r.get(0)?,
                            skill_id: r.get(1)?,
                            title: r.get(2)?,
                            body: r.get(3)?,
                            source: r.get(4)?,
                            license: r.get(5)?,
                        },
                        r.get::<_, String>(6)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(document, hash)| {
                if document.body.len() > 256_000 || sha256(document.body.as_bytes()) != hash {
                    bail!("Retrieved document integrity or size check failed")
                }
                Ok(document)
            })
            .collect()
    }
    pub(crate) fn create_candidate(
        &self,
        skill: &str,
        kind: &str,
        artifact: &str,
        checksum: &str,
        source: &str,
        license: &str,
    ) -> Result<Candidate> {
        self.create_candidate_record((skill, kind, artifact, checksum, source, license), None)
    }

    pub(crate) fn create_candidate_record(
        &self,
        metadata: (&str, &str, &str, &str, &str, &str),
        origin: Option<&crate::gaps::Preparation>,
    ) -> Result<Candidate> {
        self.create_candidate_with_lineage(metadata, origin, None)
    }

    pub(crate) fn create_prepared_candidate_record(
        &self,
        metadata: (&str, &str, &str, &str, &str, &str),
        report: &str,
    ) -> Result<Candidate> {
        self.create_candidate_with_lineage(metadata, None, Some(report))
    }

    fn create_candidate_with_lineage(
        &self,
        metadata: (&str, &str, &str, &str, &str, &str),
        origin: Option<&crate::gaps::Preparation>,
        preparation: Option<&str>,
    ) -> Result<Candidate> {
        let (skill, kind, artifact, checksum, source, license) = metadata;
        if kind != "rag" && kind != "lora" {
            bail!("Unsupported artifact kind")
        }
        crate::security::validate_provenance(source, license)?;
        if artifact.len() > 4096
            || checksum.len() != 64
            || !checksum.bytes().all(|b| b.is_ascii_hexdigit())
        {
            bail!("Invalid candidate artifact metadata")
        }
        let mut connection = self.lock()?;
        let db = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(origin) = origin {
            if origin.capability != skill || kind != "rag" {
                bail!("Gap preparation is limited to its declared RAG capability")
            }
            let raw: String = db.query_row("SELECT payload FROM events WHERE id=?1 AND task_id=?2 AND kind='capability_gap' AND source='jev-plus'", params![origin.gap_event_id,origin.task_id], |r| r.get(0))?;
            if sha256(raw.as_bytes()) != origin.gap_checksum {
                bail!("Gap evidence changed; prepare again")
            }
        }
        let count: i64 = db.query_row("SELECT COUNT(*) FROM candidates", [], |row| row.get(0))?;
        if count >= 10_000 {
            bail!("Candidate registry capacity exhausted")
        }
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM skills WHERE id=?1)",
            [skill],
            |r| r.get(0),
        )?;
        if !exists {
            bail!("Skill does not exist")
        }
        let version: i64 = db.query_row(
            "SELECT COALESCE(MAX(version),0)+1 FROM candidates WHERE skill_id=?1",
            [skill],
            |r| r.get(0),
        )?;
        let candidate = Candidate {
            id: Uuid::new_v4().to_string(),
            skill_id: skill.into(),
            kind: kind.into(),
            version,
            artifact: artifact.into(),
            checksum: checksum.into(),
            status: "candidate".into(),
            source: source.into(),
            license: license.into(),
        };
        db.execute(
            "INSERT INTO candidates VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                candidate.id,
                candidate.skill_id,
                candidate.kind,
                candidate.version,
                candidate.artifact,
                candidate.checksum,
                candidate.status,
                candidate.source,
                candidate.license,
                now()
            ],
        )?;
        if let Some(origin) = origin {
            let payload = json!({"candidate_id":candidate.id,"candidate_checksum":candidate.checksum,"capability":skill,"gap_event_id":origin.gap_event_id,"gap_checksum":origin.gap_checksum,"automatic_promotion":false});
            db.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'gap_candidate','human-cli',?2,?3)",params![origin.task_id,payload.to_string(),now()])?;
        }
        if let Some(report) = preparation {
            if kind != "rag" || report.len() > 65_536 {
                bail!("Prepared lineage is limited to bounded RAG reports")
            }
            crate::evolution::prepared::validate_report_snapshot(
                report.as_bytes(),
                &candidate.checksum,
            )?;
            crate::storage::admit(&db)?;
            let task = Uuid::new_v4().to_string();
            let payload = json!({"format_version":1,"candidate_id":candidate.id,"candidate_checksum":candidate.checksum,
                "report_sha256":sha256(report.as_bytes()),"report_json":report,
                "permissions_granted":false,"evaluation_authority":false,"automatic_promotion":false});
            let raw = payload.to_string();
            if raw.len() > 262144 {
                bail!("Prepared lineage audit exceeds bounds")
            }
            db.execute("INSERT INTO tasks(id,query,mode,status,answer,created,updated) VALUES(?1,'Prepared source candidate import','fast','completed','Provenance recorded; no evaluation or promotion',?2,?2)",params![task,now()])?;
            db.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'candidate_preparation','human-cli',?2,?3)",params![task,raw,now()])?;
        }
        db.commit()?;
        Ok(candidate)
    }
    pub fn candidate(&self, id: &str) -> Result<Candidate> {
        self.lock()?.query_row("SELECT id,skill_id,kind,version,artifact,checksum,status,source,license FROM candidates WHERE id=?1",
            [id],|r| Ok(Candidate {id:r.get(0)?,skill_id:r.get(1)?,kind:r.get(2)?,version:r.get(3)?,artifact:r.get(4)?,
                checksum:r.get(5)?,status:r.get(6)?,source:r.get(7)?,license:r.get(8)?}))
            .map_err(Into::into)
    }
    pub fn candidates(&self) -> Result<Vec<Candidate>> {
        let db = self.lock()?;
        let mut stmt=db.prepare("SELECT id,skill_id,kind,version,artifact,checksum,status,source,license FROM candidates ORDER BY created DESC")?;
        let entries = stmt
            .query_map([], |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    skill_id: r.get(1)?,
                    kind: r.get(2)?,
                    version: r.get(3)?,
                    artifact: r.get(4)?,
                    checksum: r.get(5)?,
                    status: r.get(6)?,
                    source: r.get(7)?,
                    license: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(entries)
    }
    pub(crate) fn record_dataset(
        &self,
        content: &str,
        source: &str,
        license: &str,
    ) -> Result<String> {
        crate::security::validate_provenance(source, license)?;
        if content.len() > 2_000_000 {
            bail!("Evaluation dataset exceeds bounds")
        }
        let checksum = sha256(content.as_bytes());
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let existing: Option<(String, String, String)> = tx
            .query_row(
                "SELECT content,source,license FROM evaluation_datasets WHERE checksum=?1",
                [&checksum],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((previous, previous_source, previous_license)) = existing {
            if previous != content || previous_source != source || previous_license != license {
                bail!("Evaluation dataset manifest is immutable")
            }
            return Ok(checksum);
        }
        let (count,bytes):(i64,i64)=tx.query_row("SELECT COUNT(*),COALESCE(SUM(length(CAST(content AS BLOB))),0) FROM evaluation_datasets",[],|row|Ok((row.get(0)?,row.get(1)?)))?;
        if count >= 1024 || bytes.saturating_add(content.len() as i64) > 64_000_000 {
            bail!("Evaluation dataset store capacity exhausted")
        }
        tx.execute(
            "INSERT INTO evaluation_datasets VALUES (?1,?2,?3,?4,'held_out',?5)",
            params![checksum, content, source, license, now()],
        )?;
        tx.commit()?;
        Ok(checksum)
    }

    fn dataset_from(db: &Connection, checksum: &str) -> Result<String> {
        let content: String = db.query_row(
            "SELECT content FROM evaluation_datasets WHERE checksum=?1 AND split='held_out'",
            [checksum],
            |row| row.get(0),
        )?;
        if content.len() > 2_000_000 || sha256(content.as_bytes()) != checksum {
            bail!("Evaluation dataset integrity check failed")
        }
        Ok(content)
    }

    pub(crate) fn rag_baselines(&self, skill: &str) -> Result<Vec<RagBaseline>> {
        let db = self.lock()?;
        let versions = db.prepare("SELECT version FROM candidates WHERE skill_id=?1 AND kind='rag' AND status='approved' ORDER BY version")?
            .query_map([skill], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if versions.len() > 128 {
            bail!("Historical RAG evaluation exceeds 128 approved versions")
        }
        let mut baselines = Vec::new();
        for version in versions {
            let (id,body,checksum,source,license):(String,String,String,String,String)=db.query_row(
            "SELECT c.id,d.body,c.checksum,c.source,c.license FROM candidates c JOIN candidate_documents cd ON cd.candidate_id=c.id \
             JOIN documents d ON d.id=cd.document_id WHERE c.skill_id=?1 AND c.version=?2 AND c.status='approved' AND c.kind='rag'",
            params![skill,version],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
            if body.len() > 256_000 || sha256(body.as_bytes()) != checksum {
                bail!("Active RAG baseline integrity check failed")
            }
            let (passed,details):(bool,String)=db.query_row("SELECT success,details FROM evaluations WHERE candidate_id=?1 AND suite='quality' ORDER BY id DESC LIMIT 1",[&id],|row|Ok((row.get(0)?,row.get(1)?)))?;
            let details: Value = serde_json::from_str(&details)?;
            if !passed {
                bail!("Active baseline lacks a passing held-out suite")
            }
            let hash = details["dataset_checksum"].as_str().ok_or_else(|| {
                anyhow!("Re-evaluate the active baseline with reviewed held-out dataset provenance")
            })?;
            let dataset_content = Self::dataset_from(&db, hash)?;
            baselines.push(RagBaseline {
                candidate_id: id,
                version,
                body,
                dataset_content,
                source,
                license,
            });
        }
        Ok(baselines)
    }

    pub(crate) fn evaluation(
        &self,
        id: &str,
        suite: &str,
        pass: bool,
        score: f64,
        details: Value,
    ) -> Result<()> {
        self.evaluations(id, &[(suite, pass, score, details)])
    }

    pub(crate) fn evaluations(&self, id: &str, records: &[(&str, bool, f64, Value)]) -> Result<()> {
        if records.is_empty() || records.len() > 3 {
            bail!("Invalid evaluation batch size")
        }
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let review_id = Uuid::new_v4().to_string();
        let mut suites = std::collections::BTreeSet::new();
        for (suite, pass, score, details) in records {
            if !score.is_finite()
                || !(0.0..=1.0).contains(score)
                || !matches!(*suite, "integrity" | "quality" | "regression")
                || !suites.insert(*suite)
            {
                bail!("Invalid evaluation record")
            }
            if *suite == "quality" {
                let sealed: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM candidates c JOIN evaluations e ON e.candidate_id=c.id WHERE c.id=?1 AND c.status='approved' AND e.suite='quality' AND e.success=1 AND json_extract(e.details,'$.held_out')=1 AND json_extract(e.details,'$.dataset_checksum') IS NOT NULL)",
                    [id], |row| row.get(0))?;
                if sealed {
                    bail!("Approved RAG acceptance suite is immutable; create a new candidate")
                }
            }
            let mut details = details.clone();
            details
                .as_object_mut()
                .ok_or_else(|| anyhow!("Evaluation details must be an object"))?
                .insert("review_id".into(), Value::String(review_id.clone()));
            tx.execute("INSERT INTO evaluations(candidate_id,suite,success,score,details,created) VALUES (?1,?2,?3,?4,?5,?6)",
                params![id,suite,*pass as i32,score,details.to_string(),now()])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn promote_rag(&self, id: &str, body: &str) -> Result<Candidate> {
        // Revalidate gates and candidate while holding the same DB lock.
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let candidate:Candidate=tx.query_row("SELECT id,skill_id,kind,version,artifact,checksum,status,source,license FROM candidates WHERE id=?1",[id],
            |r|Ok(Candidate{id:r.get(0)?,skill_id:r.get(1)?,kind:r.get(2)?,version:r.get(3)?,artifact:r.get(4)?,
                checksum:r.get(5)?,status:r.get(6)?,source:r.get(7)?,license:r.get(8)?}))?;
        if candidate.status != "candidate" || candidate.kind != "rag" {
            bail!("Only new RAG candidates may be promoted")
        }
        if sha256(body.as_bytes()) != candidate.checksum {
            bail!("Artifact changed since evaluation")
        }
        let mut quality_dataset = None;
        let mut quality_review = None;
        for suite in ["integrity", "quality", "regression"] {
            let approved:Option<(i64,f64,String)>=tx.query_row("SELECT success,score,details FROM evaluations WHERE candidate_id=?1 AND suite=?2 ORDER BY id DESC LIMIT 1",
                params![id,suite],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            let Some((1, score, details)) = approved else {
                bail!("Missing passing {suite} evaluation")
            };
            if score < if suite == "regression" { 1.0 } else { 0.9 } {
                bail!("Missing passing {suite} evaluation")
            }
            let details: Value = serde_json::from_str(&details)?;
            if details["candidate_checksum"].as_str() != Some(candidate.checksum.as_str()) {
                bail!("Evaluation artifact identity mismatch")
            }
            if suite == "quality" {
                let hash = details["dataset_checksum"].as_str().ok_or_else(|| {
                    anyhow!("Quality evaluation lacks reviewed dataset provenance")
                })?;
                if details["held_out"] != true || hash == candidate.checksum {
                    bail!("Invalid held-out evaluation manifest")
                }
                Self::dataset_from(&tx, hash)?;
                quality_review = Some(
                    details["review_id"]
                        .as_str()
                        .ok_or_else(|| {
                            anyhow!("Quality requires a paired review; re-evaluate candidate")
                        })?
                        .to_string(),
                );
                quality_dataset = Some(hash.to_string());
            }
            if suite == "regression" {
                if details["review_id"].as_str() != quality_review.as_deref() {
                    bail!("Quality and regression records belong to different reviews")
                }
                if details["candidate_dataset_checksum"].as_str() != quality_dataset.as_deref() {
                    bail!("Quality and regression evaluations use different dataset manifests")
                }
                let historical_ids = tx.prepare("SELECT id,version FROM candidates WHERE skill_id=?1 AND kind='rag' AND status='approved' ORDER BY version")?
                    .query_map([&candidate.skill_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                if details["baseline_ids"] != serde_json::to_value(historical_ids)? {
                    bail!("Approved history changed; re-evaluate candidate regressions")
                }
                let current: i64 = tx.query_row(
                    "SELECT active_version FROM skills WHERE id=?1",
                    [&candidate.skill_id],
                    |row| row.get(0),
                )?;
                if details["baseline_version"].as_i64() != Some(current) {
                    bail!("Active baseline changed; re-evaluate candidate regressions")
                }
            }
        }
        tx.execute(
            "INSERT OR IGNORE INTO documents VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                Uuid::new_v4().to_string(),
                candidate.skill_id,
                Path::new(&candidate.artifact)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("artifact"),
                body,
                candidate.source,
                candidate.license,
                candidate.checksum
            ],
        )?;
        let (document_id, document_skill, stored_body): (String, String, String) = tx.query_row(
            "SELECT id,skill_id,body FROM documents WHERE sha256=?1",
            [&candidate.checksum],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if document_skill != candidate.skill_id {
            bail!("Document hash belongs to another skill")
        }
        if sha256(stored_body.as_bytes()) != candidate.checksum {
            bail!("Stored document failed integrity verification")
        }
        tx.execute(
            "INSERT INTO candidate_documents VALUES (?1,?2)",
            params![id, document_id],
        )?;
        let previous: i64 = tx.query_row(
            "SELECT active_version FROM skills WHERE id=?1",
            [&candidate.skill_id],
            |r| r.get(0),
        )?;
        tx.execute("UPDATE candidates SET status='approved' WHERE id=?1", [id])?;
        tx.execute(
            "UPDATE skills SET active_version=?2 WHERE id=?1",
            params![candidate.skill_id, candidate.version],
        )?;
        tx.execute("INSERT INTO lifecycle_audit(skill_id,candidate_id,action,previous_version,active_version,created) VALUES (?1,?2,'promote',?3,?4,?5)",
            params![candidate.skill_id,id,previous,candidate.version,now()])?;
        tx.commit()?;
        drop(db);
        self.candidate(id)
    }

    /// Restore a previously approved RAG version using its durable document snapshot.
    /// Caller must enforce operator authorization before entering this storage boundary.
    pub(crate) fn rollback_rag(&self, id: &str) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (skill, version, checksum, body): (String, i64, String, String) = tx.query_row(
            "SELECT c.skill_id,c.version,c.checksum,d.body FROM candidates c \
             JOIN candidate_documents cd ON cd.candidate_id=c.id JOIN documents d ON d.id=cd.document_id \
             WHERE c.id=?1 AND c.kind='rag' AND c.status='approved'", [id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
        if sha256(body.as_bytes()) != checksum {
            bail!("Stored rollback artifact failed integrity verification")
        }
        let previous: i64 = tx.query_row(
            "SELECT active_version FROM skills WHERE id=?1",
            [&skill],
            |r| r.get(0),
        )?;
        if version >= previous {
            bail!("Rollback requires a previously approved older version")
        }
        tx.execute(
            "UPDATE skills SET active_version=?2 WHERE id=?1",
            params![skill, version],
        )?;
        tx.execute("INSERT INTO lifecycle_audit(skill_id,candidate_id,action,previous_version,active_version,created) VALUES (?1,?2,'rollback',?3,?4,?5)",
            params![skill,id,previous,version,now()])?;
        tx.commit()?;
        Ok(())
    }
}

pub fn bootstrap(store: &Store) -> Result<()> {
    let skills: &[(&str, &str, &[&str])] = &[
        (
            "rust.s1",
            "Rust language and tooling",
            &["rust", "cargo", "tokio", "borrow", "lifetime"],
        ),
        (
            "linux.s1",
            "Linux administration",
            &["linux", "arch", "nixos", "bash", "kernel"],
        ),
        (
            "security.s1",
            "Defensive software security",
            &["security", "audit", "vulnerability", "secure"],
        ),
        (
            "reverse.s1",
            "Binary format analysis",
            &["binary", "reverse", "disassembly", "metadata"],
        ),
        (
            "data.sql.s1",
            "SQL and databases",
            &["sql", "sqlite", "database", "query"],
        ),
        (
            "git.s1",
            "Git version control",
            &["git", "commit", "merge", "repository"],
        ),
        (
            "trainer.s1",
            "Offline training manifests and research documentation",
            &["trainer", "training", "lora", "dataset"],
        ),
        (
            "unity.s1",
            "Unity and C#",
            &["unity", "shader", "gameobject", "csharp", "urp"],
        ),
    ];
    for (id, description, tags) in skills {
        store.ensure_skill(
            id,
            description,
            &tags.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}

pub fn route(skills: &[Skill], query: &str) -> Vec<String> {
    let words = query
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>();
    let mut scores: Vec<(i64, String)> = skills
        .iter()
        .filter(|skill| skill.enabled)
        .filter_map(|skill| {
            let value = skill.tags.iter().filter(|tag| words.contains(*tag)).count() as i64 * 3;
            if value > 0 {
                Some((value, skill.id.clone()))
            } else {
                None
            }
        })
        .collect();
    scores.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scores.into_iter().take(2).map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_pages_bound_reads_preserve_order_and_isolate_tasks() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("paged", "fast").unwrap();
        let other = store.create_task("other", "fast").unwrap();
        for index in 0..35 {
            store
                .event(&task, "audit", "runtime", json!({"index":index}))
                .unwrap();
            store
                .event(&other, "audit", "runtime", json!({"private":true}))
                .unwrap();
        }
        let first = store.task_events_since(&task, 0).unwrap();
        assert_eq!(first["events"].as_array().unwrap().len(), 32);
        assert_eq!(first["more"], true);
        let second = store
            .task_events_since(&task, first["cursor"].as_u64().unwrap())
            .unwrap();
        assert_eq!(second["events"].as_array().unwrap().len(), 3);
        assert_eq!(second["more"], false);
        assert_eq!(second["events"][0]["payload"]["index"], 32);
        assert!(!first.to_string().contains("private"));
        assert!(!second.to_string().contains("private"));
        let empty = store
            .task_events_since(&task, second["cursor"].as_u64().unwrap())
            .unwrap();
        assert_eq!(empty["cursor"], second["cursor"]);
        assert!(empty["events"].as_array().unwrap().is_empty());
        assert!(store.task_events_since(&task, u64::MAX).is_err());
        assert!(store
            .task_events_since(&Uuid::new_v4().to_string(), 0)
            .is_err());
    }
    #[test]
    fn full_trace_refuses_excessive_or_malformed_audit_without_truncating_success() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("bounded", "fast").unwrap();
        {
            let mut db = store.lock().unwrap();
            let tx = db.transaction().unwrap();
            for _ in 0..4097 {
                tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'audit','runtime','{}',0)",[&task]).unwrap();
            }
            tx.commit().unwrap();
        }
        assert!(store
            .task(&task)
            .unwrap_err()
            .to_string()
            .contains("event limit"));
        assert_eq!(
            store.task_events_since(&task, 0).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            32
        );
        let db = store.lock().unwrap();
        db.execute("DELETE FROM events WHERE task_id=?1", [&task])
            .unwrap();
        db.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'audit','runtime',?2,0)",params![task,"x".repeat(262145)]).unwrap();
        drop(db);
        assert!(store.task(&task).is_err());
        assert!(store.task_events_since(&task, 0).is_err());
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE events SET payload='invalid json' WHERE task_id=?1",
                [&task],
            )
            .unwrap();
        assert!(store.task(&task).is_err());
        assert!(store.task_events_since(&task, 0).is_err());
    }
    #[test]
    fn full_trace_byte_ceiling_preserves_paged_history() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("large audit", "fast").unwrap();
        for _ in 0..34 {
            store
                .event(
                    &task,
                    "audit",
                    "runtime",
                    json!({"body":"x".repeat(240000)}),
                )
                .unwrap();
        }
        assert!(store
            .task(&task)
            .unwrap_err()
            .to_string()
            .contains("byte limit"));
        assert_eq!(
            store.task_events_since(&task, 0).unwrap()["events"]
                .as_array()
                .unwrap()
                .len(),
            32
        );
    }
    fn database_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("fabric-migration-{}.db", Uuid::new_v4()))
    }

    #[test]
    fn evaluation_pairs_roll_back_on_second_write_failure() {
        let store = Store::in_memory().unwrap();
        bootstrap(&store).unwrap();
        let candidate = store
            .create_candidate(
                "rust.s1",
                "rag",
                "fixture.md",
                &sha256(b"fixture"),
                "test",
                "CC0",
            )
            .unwrap();
        store.lock().unwrap().execute_batch("CREATE TRIGGER fail_regression BEFORE INSERT ON evaluations WHEN NEW.suite='regression' BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;").unwrap();
        let records = [
            ("quality", true, 1.0, json!({})),
            ("regression", true, 1.0, json!({})),
        ];
        assert!(store.evaluations(&candidate.id, &records).is_err());
        let count: i64 = store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM evaluations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_regression")
            .unwrap();
        store.evaluations(&candidate.id, &records).unwrap();
        let reviews = store
            .lock()
            .unwrap()
            .prepare("SELECT json_extract(details,'$.review_id') FROM evaluations ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(reviews.len(), 2);
        assert_eq!(reviews[0], reviews[1]);
    }

    #[test]
    fn migrate_legacy_database_and_reject_future_schema() {
        let path = database_path();
        let legacy = Connection::open(&path).unwrap();
        legacy.execute_batch(SCHEMA).unwrap();
        legacy
            .execute("INSERT INTO skills VALUES ('rust.s1','legacy','[]',0)", [])
            .unwrap();
        drop(legacy);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.skills().unwrap()[0].description, "legacy");
        let version: i64 = store
            .lock()
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.skills().unwrap().len(), 1);
        store
            .lock()
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        drop(store);
        assert!(Store::open(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn schema_seven_migration_is_atomic_and_preserves_schema_six_data() {
        let path = database_path();
        let store = Store::open(&path).unwrap();
        let task = store
            .create_task("preserve legacy history", "fast")
            .unwrap();
        store
            .event(
                &task,
                "model_residency",
                "human-cli",
                json!({"state":"confirmed","adapter":"lmstudio-native-v1","action":{"operation":"load"}}),
            )
            .unwrap();
        store.finish_task(&task, "completed", "preserved").unwrap();
        {
            let db = store.lock().unwrap();
            db.execute_batch(
                "DROP TABLE agent_session_calls; DROP TABLE agent_sessions;
                 DROP TRIGGER project_profiles_no_update; DROP TRIGGER project_profiles_no_delete; DROP INDEX project_profiles_inventory; DROP TABLE project_profiles;
                 DROP INDEX native_task_lifecycle_history; DROP TABLE managed_residency_resolution_members; DROP TABLE managed_residency_resolutions; DROP TABLE managed_residency_receipt_archive; DROP TRIGGER managed_residency_hot_no_archive_duplicate; DROP TRIGGER managed_residency_identity_immutable;
                 DROP INDEX managed_residency_active_identity;
                 DROP INDEX managed_residency_state;
                 DROP TABLE managed_residency_receipts;
                 CREATE INDEX managed_residency_active_identity ON tasks(id);
                 PRAGMA user_version=6;",
            )
            .unwrap();
        }
        drop(store);

        assert!(Store::open(&path).is_err());
        let db = Connection::open(&path).unwrap();
        let version: i64 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(
            version, 6,
            "failed migration must leave the old version intact"
        );
        let ledger_exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='managed_residency_receipts')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(
            !ledger_exists,
            "DDL before the injected index conflict must roll back"
        );
        let answer: String = db
            .query_row("SELECT answer FROM tasks WHERE id=?1", [&task], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(answer, "preserved");
        let event_count: i64 = db
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='model_residency'",
                [&task],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(event_count, 1);
        drop(db);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn managed_receipt_schema_enforces_links_immutability_and_release_reuse() {
        let store = Store::in_memory().unwrap();
        let load_task = store.create_task("load", "standard").unwrap();
        let release_task = store.create_task("release", "standard").unwrap();
        let receipt_json = r#"{"format_version":1}"#;
        let receipt_hash = sha256(receipt_json.as_bytes());
        let inventory_hash = sha256(b"inventory");
        let db = store.lock().unwrap();

        let insert = |db: &Connection, id: &str, instance: &str| {
            db.execute(
                "INSERT INTO managed_residency_receipts(receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,current_inventory_sha256,state,load_task_id,release_task_id,created,updated) VALUES (?1,'http://127.0.0.1:1234/api/v1',?2,?3,?4,?5,'active',?6,NULL,1,1)",
                params![id, instance, receipt_json, receipt_hash, inventory_hash, load_task],
            )
        };
        insert(&db, "receipt-a", "provider-instance").unwrap();
        assert!(insert(&db, "receipt-b", "provider-instance").is_err());
        db.execute(
            "UPDATE managed_residency_receipts SET state='quarantined' WHERE receipt_id='receipt-a'",
            [],
        )
        .unwrap();
        assert!(insert(&db, "receipt-b", "provider-instance").is_err());
        db.execute(
            "UPDATE managed_residency_receipts SET state='active' WHERE receipt_id='receipt-a'",
            [],
        )
        .unwrap();
        assert!(db.execute(
            "INSERT INTO managed_residency_receipts(receipt_id,endpoint,instance_id,receipt_json,receipt_sha256,current_inventory_sha256,state,load_task_id,created,updated) VALUES ('receipt-invalid-fk','http://127.0.0.1:1234/api/v1','other-instance',?1,?2,?3,'active','missing-task',1,1)",
            params![receipt_json, receipt_hash, inventory_hash],
        ).is_err());

        assert!(db.execute(
            "UPDATE managed_residency_receipts SET receipt_json='{}' WHERE receipt_id='receipt-a'",
            [],
        ).is_err());
        assert!(db.execute(
            "UPDATE managed_residency_receipts SET receipt_sha256=?1 WHERE receipt_id='receipt-a'",
            [sha256(b"tampered")],
        ).is_err());
        assert!(db.execute(
            "UPDATE managed_residency_receipts SET load_task_id='missing-task' WHERE receipt_id='receipt-a'",
            [],
        ).is_err());
        assert!(db.execute(
            "UPDATE managed_residency_receipts SET state='release_requested',release_task_id='missing-task' WHERE receipt_id='receipt-a'",
            [],
        ).is_err());
        assert!(db.execute(
            "UPDATE managed_residency_receipts SET state='release_requested',release_task_id=?1 WHERE receipt_id='receipt-a'",
            [&release_task],
        ).is_ok());
        assert!(db
            .execute("DELETE FROM tasks WHERE id=?1", [&release_task])
            .is_err());
        assert!(db.execute(
            "UPDATE managed_residency_receipts SET state='released' WHERE receipt_id='receipt-a'",
            [],
        ).is_ok());
        insert(&db, "receipt-c", "provider-instance").unwrap();
        assert!(db
            .execute("DELETE FROM tasks WHERE id=?1", [&load_task])
            .is_err());
        drop(db);
    }

    #[test]
    fn existing_database_readers_require_the_current_schema_nine() {
        let path = database_path();
        drop(Store::open(&path).unwrap());
        drop(Store::open_existing_read_only(&path).unwrap());
        drop(
            Store::open_existing_with_limit(&path, crate::storage::DEFAULT_DATABASE_BYTES).unwrap(),
        );

        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "user_version", 6).unwrap();
        drop(db);
        assert!(Store::open_existing_read_only(&path).is_err());
        assert!(
            Store::open_existing_with_limit(&path, crate::storage::DEFAULT_DATABASE_BYTES).is_err()
        );

        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "user_version", 99).unwrap();
        drop(db);
        assert!(Store::open(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn managed_residency_event_history_scan_uses_the_bounded_migration_index() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("history index fixture", "fast").unwrap();
        let db = store.lock().unwrap();
        for sequence in 0..2048 {
            db.execute(
                "INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'unrelated_audit','fixture','{}',?2)",
                params![task, sequence as f64],
            ).unwrap();
        }
        let plan = db.prepare(
            "EXPLAIN QUERY PLAN SELECT CASE WHEN length(CAST(e.payload AS BLOB))<=4096 THEN e.payload END,CASE WHEN length(CAST(t.status AS BLOB))<=64 THEN t.status END,e.task_id FROM events e JOIN tasks t ON t.id=e.task_id WHERE e.kind='model_residency' AND e.source='human-cli' ORDER BY e.id LIMIT 4097",
        ).unwrap().query_map([], |row| row.get::<_, String>(3)).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert!(
            plan.iter()
                .any(|step| step.contains("managed_residency_event_scan")),
            "the bounded endpoint-history scan must use its schema index; plan={plan:?}"
        );
        assert!(
            plan.iter()
                .all(|step| !step.contains("USE TEMP B-TREE FOR ORDER BY")),
            "the ordered event scan must not sort the full audit history; plan={plan:?}"
        );
    }

    #[test]
    fn rollback_restores_only_approved_untampered_document_versions() {
        let path = database_path();
        let store = Store::open(&path).unwrap();
        bootstrap(&store).unwrap();
        let mut ids = Vec::new();
        let dataset="{\"query\":\"Rust\",\"expected_excerpt\":\"Rust snapshot\"}\n{\"query\":\"snapshot\",\"expected_excerpt\":\"Rust snapshot\"}\n{\"query\":\"Rust snapshot\",\"expected_excerpt\":\"Rust snapshot\"}";
        let dataset_hash = store
            .record_dataset(dataset, "storage-test", "CC0")
            .unwrap();
        for body in ["Rust snapshot first", "Rust snapshot second"] {
            let c = store
                .create_candidate(
                    "rust.s1",
                    "rag",
                    "fixture.md",
                    &sha256(body.as_bytes()),
                    "test",
                    "CC0",
                )
                .unwrap();
            // Storage transaction test; production evaluation is covered independently.
            let baseline_version = store
                .skills()
                .unwrap()
                .iter()
                .find(|skill| skill.id == "rust.s1")
                .unwrap()
                .active_version;
            let baseline_ids: Vec<_> = store
                .rag_baselines("rust.s1")
                .unwrap()
                .into_iter()
                .map(|baseline| (baseline.candidate_id, baseline.version))
                .collect();
            let records = ["integrity", "quality", "regression"].into_iter().map(|suite| (suite, true, 1.0, json!({"fixture":true,"baseline_ids":baseline_ids,"dataset_checksum":dataset_hash,"candidate_dataset_checksum":dataset_hash,"held_out":true,"baseline_version":baseline_version,"candidate_checksum":c.checksum}))).collect::<Vec<_>>();
            store.evaluations(&c.id, &records).unwrap();
            store.promote_rag(&c.id, body).unwrap();
            ids.push(c.id);
        }
        assert_eq!(
            store.search("snapshot", Some("rust.s1"), 10).unwrap()[0].body,
            "Rust snapshot second"
        );
        assert!(store
            .task_context_scoped("snapshot", true, false)
            .unwrap()
            .documents
            .is_empty());
        assert_eq!(
            store
                .task_context_scoped("snapshot", true, true)
                .unwrap()
                .documents
                .len(),
            1
        );
        assert!(store.rollback_rag(&ids[1]).is_err());
        assert!(crate::evaluation::rollback(&store, &ids[0], false).is_err());
        let unknown = store
            .create_candidate(
                "rust.s1",
                "rag",
                "fixture.md",
                &sha256(b"unknown"),
                "test",
                "CC0",
            )
            .unwrap();
        assert!(store.rollback_rag(&unknown.id).is_err());
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE documents SET body='tampered' WHERE sha256=?1",
                [sha256(b"Rust snapshot first")],
            )
            .unwrap();
        assert!(crate::evaluation::rollback(&store, &ids[0], true).is_err());
        assert_eq!(
            store
                .skills()
                .unwrap()
                .iter()
                .find(|s| s.id == "rust.s1")
                .unwrap()
                .active_version,
            2
        );
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE documents SET body='Rust snapshot first' WHERE sha256=?1",
                [sha256(b"Rust snapshot first")],
            )
            .unwrap();
        crate::evaluation::rollback(&store, &ids[0], true).unwrap();
        let hits = store.search("snapshot", Some("rust.s1"), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].body, "Rust snapshot first");
        assert!(crate::security::authorize("jev-r", "candidate.rollback").is_err());
        let audits: i64 = store
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM lifecycle_audit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(audits, 3);
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened.search("snapshot", None, 10).unwrap()[0].body,
            "Rust snapshot first"
        );
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn skill_routing() {
        let tmp = std::env::temp_dir().join(format!("cogfabric-test-{}.db", Uuid::new_v4()));
        let store = Store::open(&tmp).unwrap();
        bootstrap(&store).unwrap();
        assert_eq!(
            route(&store.skills().unwrap(), "Rust tokio example")[0],
            "rust.s1"
        );
        drop(store);
        let _ = std::fs::remove_file(tmp);
    }
}

#[cfg(test)]
mod managed_schema_eight_tests {
    use super::*;

    fn legacy() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db.execute_batch(SCHEMA).unwrap();
        db.execute_batch(MANAGED_RESIDENCY_SCHEMA).unwrap();
        db.execute("INSERT INTO tasks(id,query,mode,status,created,updated) VALUES('original','load','fast','completed',1,1)", []).unwrap();
        db.execute("INSERT INTO managed_residency_receipts VALUES('receipt','http://127.0.0.1:1234/api/v1/models','instance','{}',?1,?1,'active','original',NULL,1,2)", [sha256(b"fixture")]).unwrap();
        db
    }

    #[test]
    fn schema_eight_preserves_existing_receipts_without_adopting_inventory() {
        let mut db = legacy();
        let before: String = db
            .query_row(
                "SELECT receipt_json FROM managed_residency_receipts",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let tx = db.transaction().unwrap();
        tx.execute_batch(MANAGED_RESIDENCY_SCHEMA_V8).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            8
        );
        assert_eq!(
            db.query_row(
                "SELECT receipt_json FROM managed_residency_receipts",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            before
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM managed_residency_resolutions",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM managed_residency_receipt_archive",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        db.execute("UPDATE managed_residency_receipts SET state='retired'", [])
            .unwrap();
        assert!(db
            .execute(
                "UPDATE managed_residency_receipts SET receipt_json='[0]'",
                []
            )
            .is_err());
        assert!(db
            .execute("DELETE FROM tasks WHERE id='original'", [])
            .is_err());
    }

    #[test]
    fn failed_schema_eight_migration_rolls_back_original_ledger_and_version() {
        let mut db = legacy();
        db.execute_batch("CREATE TABLE managed_residency_resolutions(conflicting TEXT);")
            .unwrap();
        {
            let tx = db.transaction().unwrap();
            assert!(tx.execute_batch(MANAGED_RESIDENCY_SCHEMA_V8).is_err());
        }
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(
            db.query_row("SELECT state FROM managed_residency_receipts", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "active"
        );
        assert!(db
            .execute("UPDATE managed_residency_receipts SET state='retired'", [])
            .is_err());
        assert!(db
            .execute(
                "UPDATE managed_residency_receipts SET receipt_json='[0]'",
                []
            )
            .is_err());
    }
}
