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
    pub(super) excluded_solver_calls: &'a BTreeSet<u64>,
}

/// Read-only, evidence-backed reconstruction of a trailing unpublished part
/// of one already admitted wave. The caller persists `reconciled` only inside
/// the approved continuation transaction.
pub(super) struct Reconciliation {
    pub(super) subtasks: Vec<Subtask>,
    pub(super) reconciled: Vec<Subtask>,
    pub(super) reconciled_indices: BTreeSet<usize>,
}

pub(super) fn reconcile_unpublished(
    db: &rusqlite::Connection,
    task: &str,
    plan: &crate::planning::Plan,
    order: &[usize],
    original: &[Subtask],
) -> Result<Reconciliation> {
    let mut unchanged = Reconciliation {
        subtasks: original.to_vec(),
        reconciled: Vec::new(),
        reconciled_indices: BTreeSet::new(),
    };
    if !original.iter().any(|node| node.state == "running") {
        return Ok(unchanged);
    }
    let schedule = graph::parallel_schedule_from(db, task, plan, order)?
        .ok_or_else(|| anyhow::anyhow!("Running parallel nodes have no immutable wave schedule"))?;
    let audit = inference::audit_from(db, task)?;
    if audit["results_not_recorded"] != 0 || audit["legacy_results_without_call_identity"] != 0 {
        bail!("An unrecorded or legacy inference result prevents parallel reconciliation")
    }

    let mut completed_prefix = Vec::new();
    let mut active_wave = None;
    for wave in &schedule.waves {
        let complete = wave.iter().all(|index| {
            original
                .get(*index)
                .is_some_and(|node| node.state == "unverified" && node.result.is_some())
        });
        let pending = wave.iter().all(|index| {
            original
                .get(*index)
                .is_some_and(|node| node.state == "pending" && node.result.is_none())
        });
        let active = wave.iter().any(|index| {
            original
                .get(*index)
                .is_some_and(|node| node.state == "running")
        });
        if active_wave.is_none() && complete {
            completed_prefix.extend(wave.iter().copied());
            continue;
        }
        if active_wave.is_none() && active {
            if wave.iter().any(|index| {
                original.get(*index).is_none_or(|node| {
                    !matches!(node.state.as_str(), "running" | "unverified")
                        || (node.state == "unverified") != node.result.is_some()
                })
            }) {
                bail!("Parallel wave has a mixed or terminal state that cannot be reconciled")
            }
            let mut saw_unpublished = false;
            for index in wave {
                let state = &original[*index].state;
                if state == "running" {
                    saw_unpublished = true;
                } else if saw_unpublished {
                    bail!("Unpublished parallel results are not a trailing wave suffix")
                }
            }
            active_wave = Some(wave.clone());
            continue;
        }
        if pending {
            continue;
        }
        bail!("Parallel history advanced beyond an incomplete wave")
    }
    let Some(active_nodes) = active_wave else {
        bail!("Running node is not part of the pinned parallel schedule")
    };
    if original
        .iter()
        .enumerate()
        .any(|(index, node)| node.state == "running" && !active_nodes.contains(&index))
    {
        bail!("Multiple unfinished parallel waves prevent reconciliation")
    }

    // A live wave reserves every member before any backend call. Require exactly
    // the recorded prefix through the active wave; an absent result means the
    // backend outcome is unknown and cannot be filled in by retry.
    let dispatched_nodes = completed_prefix
        .iter()
        .copied()
        .chain(active_nodes.iter().copied())
        .collect::<Vec<_>>();
    let mut dispatch_statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,
                CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END
         FROM events WHERE task_id=?1 AND kind='model_dispatch' AND id>?2
         ORDER BY id LIMIT 34",
    )?;
    let dispatch_rows = dispatch_statement
        .query_map(rusqlite::params![task, schedule.marker_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if dispatch_rows.len() != dispatched_nodes.len() || dispatch_rows.len() > 32 {
        bail!("Parallel dispatch history is incomplete or contains later work")
    }
    let audited = audit["calls"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing bounded inference call ledger"))?;
    let audited_by_call = audited
        .iter()
        .filter_map(|entry| Some((entry["call"].as_u64()?, entry)))
        .collect::<BTreeMap<_, _>>();
    let mut running_ids = Vec::new();
    let mut dispatch_ids = Vec::new();
    let mut response_ids = Vec::new();
    let mut completion_ids = Vec::new();
    let mut resource_ids = Vec::new();
    for (node_index, (dispatch_id, dispatch_source, raw_dispatch)) in
        dispatched_nodes.iter().zip(&dispatch_rows)
    {
        if dispatch_source != "jev-r" || raw_dispatch.len() > 16384 {
            bail!("Parallel dispatch has an invalid source or exceeds bounds")
        }
        let dispatch: Value = crate::strict_json::value(raw_dispatch.as_bytes(), 16384)?;
        let call = dispatch["call"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Parallel dispatch has no call identity"))?;
        let prompt_bytes = dispatch["prompt_bytes"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Parallel dispatch has no prompt charge"))?;
        if dispatch.as_object().is_none_or(|object| object.len() != 4)
            || dispatch["role"] != "solver"
            || dispatch["allocation"] != 192
            || prompt_bytes > 32000
            || audited_by_call.get(&call).is_none_or(|entry| {
                entry["role"] != "solver"
                    || entry["state"] != "result_recorded"
                    || entry["reserved_output_tokens"] != 192
                    || entry["prompt_bytes"] != prompt_bytes
            })
        {
            bail!("Parallel dispatch differs from its original solver charge")
        }
        let reasoning = events_by_call(db, task, "reasoning", call)?;
        if reasoning.len() != 1 || reasoning[0].1 != "jev-r" {
            bail!("Every dispatched parallel solver must have one recorded response")
        }
        let (reasoning_id, _, response) = &reasoning[0];
        let node: Node = serde_json::from_value(response["node"].clone())?;
        let text = response["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Recorded parallel response has no text"))?;
        let subtask = &original[*node_index];
        let running = subtask_state_events(db, task, &subtask.step.id, "running")?;
        if running.len() != 1 || running[0].1 != "jev-plus" {
            bail!("Parallel result has no unique prior subtask admission")
        }
        let completion = subtask_result_events(db, task, &subtask.step.id)?;
        let resource_events = subtask_resource_events(db, task, &subtask.step.id)?;
        let completed_origin = if subtask.state == "unverified" {
            if completion.len() != 1 || completion[0].1 != "jev-plus" {
                bail!("Published parallel result has no unique state transition")
            }
            let event = crate::strict_json::value(completion[0].2.as_bytes(), 262144)?;
            if event["node"] != serde_json::to_value(subtask)? {
                bail!("Published parallel transition differs from the graph state")
            }
            let object = event
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Invalid completed advisory event"))?;
            if object.len() == 1 && object.contains_key("node") {
                None
            } else if object.len() == 2 && object.contains_key("node") {
                Some(
                    object
                        .get("origin")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow::anyhow!("Missing advisory recovery provenance"))?
                        .to_owned(),
                )
            } else {
                bail!("Unsupported completed advisory event shape")
            }
        } else {
            if !completion.is_empty() {
                bail!("Unpublished result already has a completed state transition")
            }
            None
        };
        let truncated: String = text.chars().take(1200).collect();
        if text.is_empty()
            || text.len() > 96000
            || text.trim().is_empty()
            || response.as_object().is_none_or(|object| object.len() != 7)
            || node.role != "solver"
            || node.cycle != 0
            || node.content_hash != sha256(text.as_bytes())
            || node.model.is_empty()
            || node.model.len() > 256
            || response["output_tokens"]
                .as_u64()
                .is_some_and(|usage| usage > 192)
            || response
                .get("input_tokens")
                .is_none_or(|value| !value.is_null() && value.as_u64().is_none())
            || response
                .get("output_tokens")
                .is_none_or(|value| !value.is_null() && value.as_u64().is_none())
            || !response["inference_ms"]
                .as_f64()
                .is_some_and(|ms| ms.is_finite() && ms >= 0.0)
            || response.get("queue_wait_ms").is_none_or(|value| {
                !value.is_null()
                    && !value
                        .as_f64()
                        .is_some_and(|ms| ms.is_finite() && (0.0..=300000.0).contains(&ms))
            })
            || (*dispatch_id >= running[0].0)
            || (*dispatch_id >= *reasoning_id)
            || (subtask.state == "unverified"
                && completed_origin.is_none()
                && (resource_events.len() != 1 || resource_events[0].1 != "jev-plus"))
            || (subtask.state == "unverified"
                && completed_origin.is_some()
                && !resource_events.is_empty())
        {
            bail!("Recorded parallel response does not prove its exact bounded result")
        }
        if subtask.state == "unverified" {
            let event: Value = crate::strict_json::value(completion[0].2.as_bytes(), 262144)?;
            if event["node"] != serde_json::to_value(subtask)?
                || event["node"]["result"] != truncated
                || *reasoning_id >= completion[0].0
            {
                bail!("Published parallel result differs from its charged response")
            }
            if let Some(origin) = &completed_origin {
                if !matches!(
                    origin.as_str(),
                    "operator_advisory_recovery" | "operator_parallel_result_reconciliation"
                ) {
                    bail!("Unsupported advisory recovery provenance")
                }
            } else {
                let resource: Value =
                    crate::strict_json::value(resource_events[0].2.as_bytes(), 16384)?;
                if resource.as_object().is_none_or(|object| object.len() != 13)
                    || resource["format_version"] != 1
                    || resource["node_id"] != subtask.step.id
                    || resource["state"] != "unverified"
                    || resource["calls_reserved"] != 1
                    || resource["reserved_output_tokens"] != 192
                    || resource["prompt_bytes"] != prompt_bytes
                    || resource["input_tokens"] != response["input_tokens"]
                    || resource["output_tokens"] != response["output_tokens"]
                    || resource["queue_wait_ms"] != response["queue_wait_ms"]
                    || resource["result_verified"] != false
                    || resource["inference_completed"] != true
                    || !resource["elapsed_ms"]
                        .as_f64()
                        .is_some_and(|ms| ms.is_finite() && ms >= 0.0)
                    || !resource["inference_ms"]
                        .as_f64()
                        .is_some_and(|ms| ms.is_finite() && ms >= 0.0)
                    || completion[0].0 >= resource_events[0].0
                {
                    bail!("Parallel result resource audit differs from its original reservation")
                }
                resource_ids.push(resource_events[0].0);
            }
            completion_ids.push(completion[0].0);
        } else {
            if !resource_events.is_empty() {
                bail!("Unpublished subtask has a later resource-completion audit")
            }
            let mut recovered = subtask.clone();
            recovered.state = "unverified".into();
            recovered.result = Some(truncated);
            unchanged.subtasks[*node_index] = recovered.clone();
            unchanged.reconciled_indices.insert(*node_index);
            unchanged.reconciled.push(recovered);
        }
        dispatch_ids.push(*dispatch_id);
        running_ids.push(running[0].0);
        response_ids.push(*reasoning_id);
    }
    if dispatch_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || running_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || response_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || dispatch_ids
            .iter()
            .max()
            .is_some_and(|last| running_ids.iter().min().is_some_and(|first| last >= first))
        || running_ids
            .iter()
            .max()
            .is_some_and(|last| response_ids.iter().min().is_some_and(|first| last >= first))
        || completion_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || resource_ids.windows(2).any(|pair| pair[0] >= pair[1])
    {
        bail!("Parallel response chronology or suffix boundary is inconsistent")
    }
    if unchanged.reconciled.is_empty() {
        bail!("Running parallel work has no safely reconstructable result publication")
    }
    Ok(unchanged)
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
    reconciled_indices: &BTreeSet<usize>,
    anchor: HistoryAnchor<'_>,
) -> Result<Option<ParallelContinuation>> {
    let HistoryAnchor {
        tail,
        calls_before,
        boundary_present,
        excluded_solver_calls,
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
            let reconciled = reconciled_indices.contains(index);
            if reconciled {
                if !completed_events.is_empty() {
                    bail!("Reconciled parallel result already has a published transition")
                }
            } else {
                if completed_events.len() != 1 || completed_events[0].1 != "jev-plus" {
                    bail!("Completed parallel node has no unique durable result transition")
                }
                let completed_payload =
                    crate::strict_json::value(completed_events[0].2.as_bytes(), 262144)?;
                let completed_object = completed_payload
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("Invalid completed advisory event"))?;
                let normal_event =
                    completed_object.len() == 1 && completed_object.contains_key("node");
                let recovered_event = completed_object.len() == 2
                    && completed_object.contains_key("node")
                    && completed_object
                        .get("origin")
                        .and_then(Value::as_str)
                        .is_some_and(|origin| {
                            matches!(
                                origin,
                                "operator_advisory_recovery"
                                    | "operator_parallel_result_reconciliation"
                            )
                        });
                if (!normal_event && !recovered_event)
                    || completed_payload["node"] != serde_json::to_value(subtask)?
                    || completed_payload["node"]["state"] != "unverified"
                    || completed_payload["node"]["result"] != truncated
                {
                    bail!("Completed parallel transition contains altered advisory data")
                }
                completed_event_ids.push(completed_events[0].0);
            }
            if dispatches[0].0 >= *reasoning_id
                || dispatches[0].0 >= running_events[0].0
                || (!reconciled && *reasoning_id >= completed_events[0].0)
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
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')>?2 AND json_extract(payload,'$.node.role')='solver' AND json_extract(payload,'$.node.cycle')=0 ORDER BY id LIMIT 129",
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
    let mut retained_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let payload = crate::strict_json::value(row.2.as_bytes(), 262144)?;
        let call = payload["call"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Cycle-zero solver result lacks a call identity"))?;
        if !excluded_solver_calls.contains(&call) {
            retained_rows.push(row);
        }
    }
    let rows = retained_rows;
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

fn subtask_resource_events(
    db: &rusqlite::Connection,
    task: &str,
    subtask_id: &str,
) -> Result<Vec<(i64, String, String)>> {
    let mut statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,
                CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END
         FROM events WHERE task_id=?1 AND kind='subtask_resources'
           AND json_extract(payload,'$.node_id')=?2
         ORDER BY id LIMIT 2",
    )?;
    let rows = statement
        .query_map(rusqlite::params![task, subtask_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    Ok(rows)
}

#[cfg(test)]
#[path = "continuation_parallel_tests.rs"]
mod tests;
