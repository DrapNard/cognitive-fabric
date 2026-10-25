//! Bounded expiry cleanup for abandoned client-waiting collective sessions.
//!
//! The caller must hold an immediate SQLite transaction. Only sessions with a
//! waiting client task are considered; active inference claims are never touched.
use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use serde_json::Value;

const MAX_WAITING_SESSIONS: usize = 8;
const MAX_SNAPSHOT_BYTES: usize = 32_768;

/// Mark expired, different-boot, or unobservable waiting sessions terminal.
///
/// Malformed or checksum-invalid checkpoints are deliberately left alone: they
/// cannot be safely interpreted as evidence that their deadline has elapsed.
/// Database and audit failures propagate so the caller's transaction rolls back.
pub(super) fn expire_waiting(db: &Connection) -> Result<()> {
    let rows = db
        .prepare(
            "SELECT s.id,s.task_id,CASE WHEN length(CAST(s.snapshot AS BLOB))<=32768 THEN s.snapshot END,s.snapshot_sha256 \
             FROM agent_sessions s \
             JOIN tasks t ON t.id=s.task_id \
             WHERE s.state='waiting' AND t.status='waiting_client' \
             ORDER BY s.updated,s.id LIMIT 9",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if rows.len() > MAX_WAITING_SESSIONS {
        bail!("Waiting collective-session cleanup exceeds active-task capacity")
    }

    for (session_id, task_id, raw, checksum) in rows {
        let Some(raw) = raw else {
            continue;
        };
        if raw.len() > MAX_SNAPSHOT_BYTES || crate::store::sha256(raw.as_bytes()) != checksum {
            continue;
        }

        let Ok(value) = crate::strict_json::value(raw.as_bytes(), MAX_SNAPSHOT_BYTES) else {
            continue;
        };
        // Deserialize the complete checkpoint as well as the clock so an
        // unknown or malformed state shape cannot authorize destructive cleanup.
        if serde_json::from_value::<super::sessions::State>(value.clone()).is_err() {
            continue;
        }
        let Some(wall_seconds) = value
            .get("wall_seconds")
            .and_then(Value::as_u64)
            .filter(|seconds| (1..=3600).contains(seconds))
        else {
            continue;
        };
        let Some(clock_value) = value.get("clock") else {
            continue;
        };
        let Ok(clock) =
            serde_json::from_value::<crate::execution_clock::Snapshot>(clock_value.clone())
        else {
            continue;
        };
        if clock.validate(wall_seconds).is_err() {
            continue;
        }

        let Ok(observation) = clock.remaining() else {
            // A backwards or otherwise invalid local clock is not expiry proof.
            continue;
        };
        let clock_status = observation
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let remaining_ns = observation.get("remaining_ns");
        let expired = clock_status == "different_boot"
            || remaining_ns.is_none_or(Value::is_null)
            || remaining_ns.and_then(Value::as_u64).is_some_and(|n| n == 0)
            || observation
                .get("expired")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if !expired {
            continue;
        }

        let mut sanitized = value;
        if let Some(object) = sanitized.as_object_mut() {
            if let Some(advice) = object.get_mut("advice") {
                *advice = Value::String(String::new());
            }
            if let Some(references) = object.get_mut("references") {
                *references = Value::String(String::new());
            }
            if let Some(pending) = object.get_mut("pending") {
                *pending = Value::Array(Vec::new());
            }
            if let Some(history_hash) = object.get_mut("history_sha256") {
                *history_hash = Value::String(String::new());
            }
        }
        let sanitized_raw = serde_json::to_string(&sanitized)?;
        if sanitized_raw.len() > MAX_SNAPSHOT_BYTES {
            continue;
        }
        let sanitized_checksum = crate::store::sha256(sanitized_raw.as_bytes());

        if db.execute(
            "UPDATE agent_sessions SET state='failed',snapshot=?2,snapshot_sha256=?3,updated=unixepoch() \
             WHERE id=?1 AND state='waiting'",
            params![session_id, sanitized_raw, sanitized_checksum],
        )? != 1
        {
            bail!("Waiting collective session changed during expiry cleanup")
        }
        if db.execute(
            "UPDATE tasks SET status='exhausted',answer='Collective session expired; no automatic replay or refund',updated=unixepoch() \
             WHERE id=?1 AND status='waiting_client'",
            [&task_id],
        )? != 1
        {
            bail!("Waiting collective task changed during expiry cleanup")
        }
        db.execute(
            "DELETE FROM agent_session_calls WHERE session_id=?1",
            [&session_id],
        )?;
        let audit = serde_json::json!({
            "session_id": session_id,
            "state": "failed",
            "reason": "deadline_expired_or_unavailable",
            "clock_status": clock_status,
            "automatic_replay": false,
            "refund": false,
            "history_stored": false
        });
        let audit_raw = serde_json::to_string(&audit)?;
        if audit_raw.len() > 8192 {
            bail!("Collective expiry audit exceeds metadata bounds")
        }
        db.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,'agent_session_expired','jev-r',?2,unixepoch())",
            params![task_id, audit_raw],
        )?;
    }
    Ok(())
}
