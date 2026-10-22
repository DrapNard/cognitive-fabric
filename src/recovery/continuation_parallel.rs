//! Validate completed parallel advisory waves before operator continuation.
use super::{graph, inference};
use crate::{planning::Subtask, reasoning::Node, store::sha256};
use anyhow::{bail, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(super) struct ParallelContinuation {
    pub(super) marker_id: i64,
    pub(super) max_parallelism: usize,
    pub(super) waves: Vec<Vec<usize>>,
    pub(super) completed_waves: usize,
}

pub(super) struct HistoryAnchor<'a> {
    pub(super) tail: &'a [&'a Value],
    pub(super) calls_before: u64,
    pub(super) boundary_present: bool,
}

/// Validate the immutable wave marker, complete-wave prefix, and exact
/// dispatch/result lineage for every completed advisory node. The model outputs
/// remain advice; this only permits resuming pending later waves.
pub(super) fn validate(
    db: &rusqlite::Connection,
    task: &str,
    plan: &crate::planning::Plan,
    order: &[usize],
    subtasks: &[Subtask],
    anchor: HistoryAnchor<'_>,
) -> Result<Option<ParallelContinuation>> {
    let HistoryAnchor {
        tail,
        calls_before,
        boundary_present,
    } = anchor;
    let Some(schedule) = graph::parallel_schedule_from(db, task, plan, order)? else {
        return Ok(None);
    };
    if schedule.order != order {
        bail!("Parallel checkpoint order differs from its pinned schedule")
    }
    let mut completed_waves = 0usize;
    let mut saw_pending = false;
    let mut completed_indices = Vec::new();
    for wave in &schedule.waves {
        let completed = wave.iter().all(|index| {
            subtasks
                .get(*index)
                .is_some_and(|node| node.state == "unverified" && node.result.is_some())
        });
        let pending = wave.iter().all(|index| {
            subtasks
                .get(*index)
                .is_some_and(|node| node.state == "pending" && node.result.is_none())
        });
        if completed && !saw_pending {
            completed_waves += 1;
            completed_indices.extend(wave.iter().copied());
        } else if pending {
            saw_pending = true;
        } else {
            bail!("Parallel continuation requires a complete-wave prefix and pending suffix")
        }
    }
    if boundary_present && completed_waves != schedule.waves.len() {
        bail!("A final-solver boundary requires every pinned advisory wave to be complete")
    }
    if subtasks.iter().any(|node| {
        !matches!(node.state.as_str(), "pending" | "unverified")
            || (node.state == "unverified") != node.result.is_some()
    }) {
        bail!("Parallel continuation contains an unresolved or terminal advisory node")
    }

    if tail.first().is_none_or(|call| call["role"] != "planner")
        || tail.len() < completed_indices.len() + 1
    {
        bail!("Parallel checkpoint is missing its charged planner result")
    }
    let audit = inference::audit_from(db, task)?;
    let audited_calls = audit["calls"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing bounded inference ledger"))?;
    let by_call = audited_calls
        .iter()
        .filter_map(|call| Some((call["call"].as_u64()?, call)))
        .collect::<BTreeMap<_, _>>();

    let mut correlated_calls = BTreeSet::<u64>::new();
    let mut previous_wave_result = None::<i64>;
    for wave_index in 0..completed_waves {
        let wave = &schedule.waves[wave_index];
        let mut dispatch_ids = Vec::with_capacity(wave.len());
        let mut running_ids = Vec::with_capacity(wave.len());
        let mut result_ids = Vec::with_capacity(wave.len());
        let mut completed_event_ids = Vec::with_capacity(wave.len());
        for index in wave {
            let scheduled = completed_indices
                .iter()
                .position(|completed| completed == index)
                .ok_or_else(|| anyhow::anyhow!("Completed node is absent from the wave prefix"))?;
            let call = tail
                .get(1 + scheduled)
                .ok_or_else(|| anyhow::anyhow!("Missing completed-wave inference reservation"))?;
            if call["role"] != "solver" {
                bail!("Completed parallel node is not bound to a solver reservation")
            }
            let call_id = call["call"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Missing parallel solver call identity"))?;
            if call_id <= calls_before || call["state"] != "result_recorded" {
                bail!("Completed parallel node has no unique recorded inference result")
            }
            if by_call.get(&call_id).is_none_or(|record| {
                record["role"] != "solver" || record["state"] != "result_recorded"
            }) {
                bail!("Parallel reasoning result has no matching inference reservation")
            }
            let dispatches = events_by_call(db, task, "model_dispatch", call_id)?;
            if dispatches.len() != 1
                || dispatches[0].1 != "jev-r"
                || dispatches[0].2["role"] != "solver"
            {
                bail!("Parallel solver reservation is ambiguous or malformed")
            }
            let reasonings = events_by_call(db, task, "reasoning", call_id)?;
            if reasonings.len() != 1 || reasonings[0].1 != "jev-r" {
                bail!("Parallel solver call has no unique Jev-R reasoning result")
            }
            let (reasoning_id, _, payload) = &reasonings[0];
            let node: Node = serde_json::from_value(payload["node"].clone())?;
            let text = payload["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing parallel solver result text"))?
                .to_string();
            let subtask = subtasks
                .get(*index)
                .ok_or_else(|| anyhow::anyhow!("Parallel wave references an unknown node"))?;
            let running_events = subtask_state_events(db, task, &subtask.step.id, "running")?;
            if running_events.len() != 1 || running_events[0].1 != "jev-plus" {
                bail!("Completed parallel node has no unique durable admission")
            }
            let truncated: String = text.chars().take(1200).collect();
            let completed_events = subtask_result_events(db, task, &subtask.step.id)?;
            if completed_events.len() != 1 || completed_events[0].1 != "jev-plus" {
                bail!("Completed parallel node has no unique durable result transition")
            }
            let completed_payload =
                crate::strict_json::value(completed_events[0].2.as_bytes(), 262144)?;
            let completed_object = completed_payload
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Invalid completed advisory event"))?;
            let normal_event = completed_object.len() == 1 && completed_object.contains_key("node");
            let recovered_event = completed_object.len() == 2
                && completed_object.contains_key("node")
                && completed_object.get("origin").and_then(Value::as_str)
                    == Some("operator_advisory_recovery");
            if (!normal_event && !recovered_event)
                || completed_payload["node"] != serde_json::to_value(subtask)?
                || completed_payload["node"]["state"] != "unverified"
                || completed_payload["node"]["result"] != truncated
            {
                bail!("Completed parallel transition contains altered advisory data")
            }
            if dispatches[0].0 >= *reasoning_id
                || dispatches[0].0 >= running_events[0].0
                || *reasoning_id >= completed_events[0].0
                || dispatches[0].0 <= schedule.marker_id
                || node.role != "solver"
                || node.cycle != 0
                || node.content_hash != sha256(text.as_bytes())
                || subtask.result.as_deref() != Some(truncated.as_str())
                || !correlated_calls.insert(call_id)
            {
                bail!("Parallel subtask result differs from its cycle-zero charged inference")
            }
            dispatch_ids.push(dispatches[0].0);
            running_ids.push(running_events[0].0);
            result_ids.push(*reasoning_id);
            completed_event_ids.push(completed_events[0].0);
        }
        let first_result = *result_ids
            .iter()
            .min()
            .ok_or_else(|| anyhow::anyhow!("Completed wave has no result"))?;
        if dispatch_ids.iter().any(|id| *id >= first_result)
            || dispatch_ids.iter().max().is_some_and(|last_dispatch| {
                running_ids
                    .iter()
                    .min()
                    .is_some_and(|first_running| last_dispatch >= first_running)
            })
            || running_ids.iter().any(|id| *id >= first_result)
            || result_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || completed_event_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || previous_wave_result
                .is_some_and(|id| dispatch_ids.iter().any(|dispatch| *dispatch <= id))
        {
            bail!("Parallel wave reservation/result chronology is inconsistent")
        }
        previous_wave_result = completed_event_ids.iter().max().copied();
    }

    // No additional cycle-zero solver result may masquerade as an unrecorded
    // subtask. Later ready-solver boundary work uses its own charged cycle.
    let mut statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')>?2 AND json_extract(payload,'$.node.role')='solver' AND json_extract(payload,'$.node.cycle')=0 ORDER BY id LIMIT 34",
    )?;
    let rows = statement
        .query_map(rusqlite::params![task, calls_before], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != completed_indices.len()
        || rows.len() > 32
        || rows
            .iter()
            .any(|(_, source, raw)| source != "jev-r" || raw.len() > 262144)
    {
        bail!("Cycle-zero solver history does not match completed parallel waves")
    }
    let observed_calls = rows
        .iter()
        .map(|(_, _, raw)| -> Result<u64> {
            let payload = crate::strict_json::value(raw.as_bytes(), 262144)?;
            payload["call"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Missing cycle-zero solver call identity"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if correlated_calls != observed_calls {
        bail!("Cycle-zero solver events differ from completed graph nodes")
    }

    Ok(Some(ParallelContinuation {
        marker_id: schedule.marker_id,
        max_parallelism: schedule.max_parallelism,
        waves: schedule.waves,
        completed_waves,
    }))
}

fn events_by_call(
    db: &rusqlite::Connection,
    task: &str,
    kind: &str,
    call: u64,
) -> Result<Vec<(i64, String, Value)>> {
    let mut statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind=?2 AND json_extract(payload,'$.call')=?3 ORDER BY id LIMIT 2",
    )?;
    let rows = statement
        .query_map(rusqlite::params![task, kind, call], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, source, raw)| {
            let payload = crate::strict_json::value(raw.as_bytes(), 262144)?;
            Ok((id, source, payload))
        })
        .collect()
}

fn subtask_state_events(
    db: &rusqlite::Connection,
    task: &str,
    subtask_id: &str,
    state: &str,
) -> Result<Vec<(i64, String)>> {
    let mut statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END FROM events WHERE task_id=?1 AND kind='subtask' AND length(CAST(payload AS BLOB))<=262144 AND json_extract(payload,'$.node.step.id')=?2 AND json_extract(payload,'$.node.state')=?3 ORDER BY id LIMIT 2",
    )?;
    let rows = statement
        .query_map(rusqlite::params![task, subtask_id, state], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    Ok(rows)
}

fn subtask_result_events(
    db: &rusqlite::Connection,
    task: &str,
    subtask_id: &str,
) -> Result<Vec<(i64, String, String)>> {
    let mut statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='subtask' AND length(CAST(payload AS BLOB))<=262144 AND json_extract(payload,'$.node.step.id')=?2 AND json_extract(payload,'$.node.state')='unverified' ORDER BY id LIMIT 2",
    )?;
    let rows = statement
        .query_map(rusqlite::params![task, subtask_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    Ok(rows)
}
