//! Restore only an already published language result; never inference replay.
use super::{inference, ExecutionLease};
use crate::{
    security,
    store::{sha256, Store},
    verification::Criterion,
};
use anyhow::{bail, Result};
use rusqlite::params;
use serde_json::{json, Value};

impl Store {
    pub fn recover_language_result(&self, task: &str) -> Result<Value> {
        security::authorize("human-cli", "task.recover")?;
        uuid::Uuid::parse_str(task)?;
        let path = self.runtime_lock.as_deref().ok_or_else(|| {
            anyhow::anyhow!("Language result recovery requires a persistent store")
        })?;
        let _lease = ExecutionLease::acquire(path, true).map_err(|error| {
            error.context("Cannot acquire exclusive runtime lease for language result recovery")
        })?;
        let mut db = self.lock()?;
        let started = std::time::Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > std::time::Duration::from_secs(5)),
        );
        let result = (|| -> Result<(Value, crate::events::Progress)> {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            // Unknown callbacks and managed native ownership still require their
            // existing restoration path; terminal text does not bypass them.
            super::validate_verifier_recovery(&tx, task, None)?;
            let (query, prior_status): (String, String) = tx.query_row(
                "SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END,
                        CASE WHEN length(status)<=64 THEN status END FROM tasks WHERE id=?1",
                [task],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if !matches!(prior_status.as_str(), "running" | "interrupted") {
                bail!("Only abandoned or interrupted tasks permit language result recovery")
            }
            let audit = inference::audit_from(&tx, task)?;
            if audit["language_boundary"]["phase"] != "presented"
                || audit["results_not_recorded"] != 0
                || audit["legacy_results_without_call_identity"] != 0
            {
                bail!("Recovery requires a published final language response and no unresolved inference")
            }
            let raw: String = tx.query_row(
                "SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END
                 FROM events WHERE task_id=?1 AND kind='language_boundary_presentation'
                 ORDER BY id DESC LIMIT 1",
                [task],
                |row| row.get(0),
            )?;
            let publication = crate::strict_json::value(raw.as_bytes(), 4096)?;
            let raw: String = tx.query_row(
                "SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END
                 FROM events WHERE task_id=?1 AND kind='reasoning'
                 AND json_extract(payload,'$.node.role')='synthesizer'
                 ORDER BY id DESC LIMIT 1",
                [task],
                |row| row.get(0),
            )?;
            let saved = crate::strict_json::value(raw.as_bytes(), 262144)?;
            let answer = saved["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing final language response"))?;
            if answer.is_empty()
                || answer.len() > 96_000
                || publication["answer_sha256"] != sha256(answer.as_bytes())
            {
                bail!("Published language response identity mismatch")
            }
            let mut status = publication["status"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing published status"))?
                .to_owned();
            let mut verified = false;
            let mut criterion_name = None;
            if status == "completed" {
                let criterion = Criterion::from_query(&query)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Completed language result requires a fresh independent compiled checker"
                    )
                })?;
                security::TaskPolicy::reasoning(task)?.authorize(
                    task,
                    "jev-plus",
                    criterion.capability(),
                )?;
                if audit["remaining_inference_admission"]["tool_calls"]
                    .as_u64()
                    .unwrap_or(0)
                    == 0
                {
                    bail!("No original verification allowance remains for language result recovery")
                }
                let tools: u64 = tx.query_row(
                    "SELECT count(*) FROM events WHERE task_id=?1 AND kind='tool_reservation'",
                    [task],
                    |row| row.get(0),
                )?;
                tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'tool_reservation','security-kernel',?2,unixepoch())",
                    params![task,json!({"format_version":1,"invocation":tools+1,"actor":"jev-plus",
                        "capability":criterion.capability(),"origin":"operator_language_result_recovery"}).to_string()])?;
                let check = criterion.check(answer);
                verified = check.is_ok();
                if !verified {
                    status = "rejected".into();
                }
                criterion_name = Some(criterion.name());
                tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'verification','verifier-r',?2,unixepoch())",
                    params![task,json!({"criterion":criterion.name(),"verified":verified,
                        "origin":"operator_language_result_recovery","objective_hash":sha256(query.as_bytes()),
                        "answer_hash":sha256(answer.as_bytes()),"reason":check.err().map(|error|error.to_string())}).to_string()])?;
            } else if !matches!(status.as_str(), "unverified" | "rejected") {
                bail!("Unsupported published language result status")
            }
            if tx.execute("UPDATE tasks SET status=?2,answer=?3,updated=unixepoch() WHERE id=?1 AND status=?4",
                params![task,status,answer,prior_status])? != 1 {
                bail!("Task changed during exclusive language result recovery")
            }
            let report = json!({"format_version":1,"task_id":task,"status":status,
                "origin":"published_language_result","verified":verified,"criterion":criterion_name,
                "answer_hash":sha256(answer.as_bytes()),"inference_repeated":false,
                "automatic_replay":false,"fresh_budget_granted":false,"resource_usage_complete":false});
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'recovery','human-cli',?2,unixepoch())",
                params![task,report.to_string()])?;
            let progress = crate::events::Progress::from_audit(
                tx.last_insert_rowid() as u64,
                task,
                "recovery",
                "human-cli",
                &report,
            )?
            .ok_or_else(|| anyhow::anyhow!("Missing typed recovery event"))?;
            if started.elapsed() > std::time::Duration::from_secs(5) {
                bail!("Language result recovery deadline exhausted")
            }
            tx.commit()?;
            Ok((report, progress))
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        let (report, progress) = result?;
        self.events.publish(&progress)?;
        Ok(report)
    }
}
