//! Bounded durable storage for immutable typed route snapshots.
//!
//! Snapshot records contain route declarations and provenance only. They are
//! written with the session-opening transaction and never refreshed from the
//! active registry during continuation.

use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

// The inner 1 MiB catalog and 64 KiB module record are JSON strings in
// the envelope, so quote/backslash escaping can roughly double their size.
const MAX_SNAPSHOT_BYTES: usize = 2_500_000;
const CHUNK_BYTES: usize = 3_072;
const MAX_CHUNKS: usize = MAX_SNAPSHOT_BYTES.div_ceil(CHUNK_BYTES);
const MAX_EVENT_BYTES: usize = 8_192;
const KIND: &str = "agent_typed_route_snapshot_chunk";
const SOURCE: &str = "agent-session";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {
    format_version: u8,
    task_sha256: String,
    snapshot_sha256: String,
    total_bytes: usize,
    chunk_count: usize,
    index: usize,
    data_hex: String,
}

/// Store a single immutable snapshot in bounded event rows. The caller must
/// include this operation in the transaction that creates the agent session.
pub(super) fn persist(db: &Connection, task: &str, raw: &[u8]) -> Result<String> {
    validate_task(task)?;
    validate_snapshot(raw)?;
    if raw.len() > MAX_SNAPSHOT_BYTES {
        bail!("Typed route snapshot exceeds its storage bound")
    }
    let snapshot_sha256 = crate::store::sha256(raw);
    let task_sha256 = crate::store::sha256(task.as_bytes());
    let count = raw.len().div_ceil(CHUNK_BYTES);
    if count == 0 || count > MAX_CHUNKS {
        bail!("Typed route snapshot chunk count is outside its bound")
    }

    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind=?2 AND source=?3)",
        params![task, KIND, SOURCE],
        |row| row.get(0),
    )?;
    if exists {
        bail!("Typed route snapshot already exists for this task")
    }

    let deadline = Instant::now() + Duration::from_secs(1);
    db.progress_handler(1000, Some(move || Instant::now() >= deadline));
    let result = (|| -> Result<()> {
        for (index, bytes) in raw.chunks(CHUNK_BYTES).enumerate() {
            let chunk = Chunk {
                format_version: 1,
                task_sha256: task_sha256.clone(),
                snapshot_sha256: snapshot_sha256.clone(),
                total_bytes: raw.len(),
                chunk_count: count,
                index,
                data_hex: hex::encode(bytes),
            };
            let payload = serde_json::to_string(&serde_json::to_value(&chunk)?)?;
            if payload.len() > MAX_EVENT_BYTES {
                bail!("Typed route snapshot chunk exceeds its event bound")
            }
            db.execute(
                "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,?2,?3,?4,unixepoch())",
                params![task, KIND, SOURCE, payload],
            )?;
        }
        Ok(())
    })();
    db.progress_handler(0, None::<fn() -> bool>);
    result?;
    Ok(snapshot_sha256)
}

/// Restore and verify one complete snapshot by its task-bound content digest.
/// This function reads only the frozen snapshot rows, never the live registry.
pub(super) fn restore(db: &Connection, task: &str, expected_sha256: &str) -> Result<Vec<u8>> {
    validate_task(task)?;
    validate_digest(expected_sha256)?;
    let deadline = Instant::now() + Duration::from_secs(1);
    db.progress_handler(1000, Some(move || Instant::now() >= deadline));
    let result = restore_inner(db, task, expected_sha256);
    db.progress_handler(0, None::<fn() -> bool>);
    result
}

fn restore_inner(db: &Connection, task: &str, expected_sha256: &str) -> Result<Vec<u8>> {
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(payload AS BLOB))<=8192 THEN payload END
         FROM events WHERE task_id=?1 AND kind=?2 AND source=?3 ORDER BY id LIMIT ?4",
    )?;
    let mut rows = statement.query(params![task, KIND, SOURCE, MAX_CHUNKS + 1])?;
    let task_sha256 = crate::store::sha256(task.as_bytes());
    let mut chunks = Vec::new();
    let mut total_bytes = None;
    let mut expected_count = None;
    let mut declared_digest = None;
    let mut next_index = 0usize;
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        if raw.len() > MAX_EVENT_BYTES {
            bail!("Typed route snapshot event exceeds its byte bound")
        }
        let value = crate::strict_json::value(raw.as_bytes(), MAX_EVENT_BYTES)?;
        let canonical = serde_json::to_string(&value)?;
        if canonical != raw {
            bail!("Typed route snapshot chunk is not canonical JSON")
        }
        let chunk: Chunk = serde_json::from_value(value)?;
        if chunk.format_version != 1
            || chunk.task_sha256 != task_sha256
            || chunk.snapshot_sha256 != expected_sha256
            || chunk.index != next_index
            || chunk.chunk_count == 0
            || chunk.chunk_count > MAX_CHUNKS
            || chunk.total_bytes == 0
            || chunk.total_bytes > MAX_SNAPSHOT_BYTES
            || chunk.data_hex.len() > CHUNK_BYTES * 2
            || !chunk.data_hex.len().is_multiple_of(2)
        {
            bail!("Typed route snapshot chunk metadata is invalid")
        }
        if total_bytes
            .replace(chunk.total_bytes)
            .is_some_and(|n| n != chunk.total_bytes)
            || expected_count
                .replace(chunk.chunk_count)
                .is_some_and(|n| n != chunk.chunk_count)
            || declared_digest
                .replace(chunk.snapshot_sha256.clone())
                .is_some_and(|digest| digest != chunk.snapshot_sha256)
        {
            bail!("Typed route snapshot chunks disagree")
        }
        if chunk.chunk_count != chunk.total_bytes.div_ceil(CHUNK_BYTES) {
            bail!("Typed route snapshot chunk count does not match its size")
        }
        let bytes = hex::decode(&chunk.data_hex)?;
        if bytes.is_empty()
            || (chunk.index + 1 < chunk.chunk_count && bytes.len() != CHUNK_BYTES)
            || (chunk.index + 1 == chunk.chunk_count
                && bytes.len() != chunk.total_bytes - CHUNK_BYTES * (chunk.chunk_count - 1))
        {
            bail!("Typed route snapshot chunk length is invalid")
        }
        chunks.push(bytes);
        next_index += 1;
        if next_index > MAX_CHUNKS {
            bail!("Typed route snapshot has too many chunks")
        }
    }
    let count = expected_count.ok_or_else(|| anyhow!("Typed route snapshot is missing"))?;
    let total = total_bytes.ok_or_else(|| anyhow!("Typed route snapshot size is missing"))?;
    if count != next_index || count != total.div_ceil(CHUNK_BYTES) {
        bail!("Typed route snapshot is incomplete")
    }
    let mut raw = Vec::with_capacity(total);
    for chunk in chunks {
        raw.extend_from_slice(&chunk);
    }
    if raw.len() != total || crate::store::sha256(&raw) != expected_sha256 {
        bail!("Typed route snapshot digest or length mismatch")
    }
    validate_snapshot(&raw)?;
    Ok(raw)
}

fn validate_snapshot(raw: &[u8]) -> Result<()> {
    if raw.is_empty() || raw.len() > MAX_SNAPSHOT_BYTES {
        bail!("Typed route snapshot size is outside its bound")
    }
    let value = crate::strict_json::value(raw, MAX_SNAPSHOT_BYTES)?;
    if serde_json::to_vec(&value)? != raw {
        bail!("Typed route snapshot is not canonical JSON")
    }
    Ok(())
}

fn validate_task(task: &str) -> Result<()> {
    if task.len() != 36 || uuid::Uuid::parse_str(task).is_err() {
        bail!("Invalid typed route snapshot task identity")
    }
    Ok(())
}

fn validate_digest(digest: &str) -> Result<()> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("Invalid typed route snapshot digest")
    }
    Ok(())
}
