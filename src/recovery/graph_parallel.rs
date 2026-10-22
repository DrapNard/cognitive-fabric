//! Validate declared parallel waves for observation, never continuation authority.
use super::*;

pub(super) fn parallel_waves_from(
    db: &rusqlite::Connection,
    task: &str,
    plan: &Plan,
    order: &[usize],
) -> Result<Option<Vec<Vec<usize>>>> {
    let mut statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind='subtask_parallel_schedule' ORDER BY id LIMIT 2",
    )?;
    let records = statement
        .query_map([task], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if records.is_empty() {
        return Ok(None);
    }
    if records.len() != 1 {
        bail!("Ambiguous parallel subtask schedule")
    }
    let (event_id, source, raw) = &records[0];
    let payload = crate::strict_json::value(raw.as_bytes(), 4096)?;
    let fields = [
        "format_version",
        "max_parallelism",
        "graph_sha256",
        "order",
        "waves",
        "permissions_granted",
        "results_verified",
    ];
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Invalid parallel schedule object"))?;
    let maximum = payload["max_parallelism"]
        .as_u64()
        .filter(|maximum| (2..=4).contains(maximum))
        .ok_or_else(|| anyhow::anyhow!("Invalid parallel schedule ceiling"))?
        as usize;
    if source != "jev-plus"
        || object.len() != fields.len()
        || fields.iter().any(|field| !object.contains_key(*field))
        || payload["format_version"] != 1
        || payload["permissions_granted"] != false
        || payload["results_verified"] != false
        || payload["graph_sha256"] != sha256(&serde_json::to_vec(plan)?)
        || payload["order"] != serde_json::to_value(order)?
    {
        bail!("Invalid parallel subtask schedule identity")
    }
    let mut settings_statement = db.prepare(
        "SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 2",
    )?;
    let settings = settings_statement
        .query_map([task], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if settings.len() != 1 || settings[0].0 >= *event_id || settings[0].1 != "runtime" {
        bail!("Parallel schedule has no unique preceding runtime settings")
    }
    let settings_payload = crate::strict_json::value(settings[0].2.as_bytes(), 262144)?;
    if settings_payload["format_version"] != 1 {
        bail!("Unsupported parallel schedule runtime settings")
    }
    let config: crate::config::Config =
        serde_json::from_value(settings_payload["configuration"].clone())?;
    config.validate()?;
    if config.subtask_parallelism != Some(maximum) {
        bail!("Parallel schedule differs from the original configured ceiling")
    }
    let waves: Vec<Vec<usize>> = serde_json::from_value(payload["waves"].clone())?;
    let mut expected = Vec::new();
    let mut completed = std::collections::BTreeSet::new();
    while completed.len() < plan.steps.len() {
        let wave = order
            .iter()
            .copied()
            .filter(|index| {
                !completed.contains(index)
                    && plan.steps[*index].depends_on.iter().all(|dependency| {
                        plan.steps.iter().enumerate().any(|(prior, step)| {
                            &step.id == dependency && completed.contains(&prior)
                        })
                    })
            })
            .take(maximum)
            .collect::<Vec<_>>();
        if wave.is_empty() {
            bail!("Parallel schedule cannot advance the pinned dependency graph")
        }
        completed.extend(wave.iter().copied());
        expected.push(wave);
    }
    if waves != expected {
        bail!("Parallel waves differ from the bounded ready-node schedule")
    }
    let (schedule_id, first_running): (Option<i64>, Option<i64>) = db.query_row(
        "SELECT (SELECT min(id) FROM events WHERE task_id=?1 AND kind='subtask_schedule'),(SELECT min(id) FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.state')='running')",
        [task], |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if schedule_id.is_none_or(|id| id >= *event_id)
        || first_running.is_some_and(|id| id <= *event_id)
    {
        bail!("Parallel schedule does not precede subtask dispatch")
    }
    Ok(Some(waves))
}

#[cfg(test)]
#[path = "graph_parallel/tests.rs"]
mod tests;
