//! Link an already recorded advisory result after a crash, without inference replay.
use super::{continuation, graph, inference, ExecutionLease};
use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use rusqlite::params;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

impl Store {
    /// Operator-approved reconciliation only. Advice remains unverified and all
    /// original reservations/deadlines remain charged. No backend is invoked.
    pub fn recover_advisory_result(&self, task: &str, human_approve: bool) -> Result<Value> {
        security::authorize("human-cli", "task.recover")?;
        if !human_approve {
            bail!("Advisory recovery requires explicit operator approval")
        }
        uuid::Uuid::parse_str(task)?;
        let path = self
            .runtime_lock
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Advisory recovery requires a persistent store"))?;
        let _lease = ExecutionLease::acquire(path, true)
            .map_err(|_| anyhow::anyhow!("Cannot reconcile while runtime leases are held"))?;
        let mut db = self.lock()?;
        let started = Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(5)),
        );
        let result = (|| -> Result<Value> {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let (status, plan, states) = graph::graph_from(&tx, task)?;
            if !matches!(status.as_str(), "running" | "interrupted") || plan.is_none() {
                bail!("No abandoned advisory graph")
            }
            let mut running = states.iter().filter(|n| n.state == "running");
            let mut node = running
                .next()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("No running advisory node to reconcile"))?;
            if running.next().is_some() {
                bail!("Ambiguous running advisory nodes")
            }
            let audit = inference::audit_from(&tx, task)?;
            if audit["results_not_recorded"] != 0
                || audit["legacy_results_without_call_identity"] != 0
            {
                bail!("Unresolved inference cannot be reconciled")
            }
            let (result_event,source,raw):(u64,String,String)=tx.query_row("SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' ORDER BY id DESC LIMIT 1",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let saved = crate::strict_json::value(raw.as_bytes(), 262144)?;
            let running_event:u64=tx.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')=?2 ORDER BY id DESC LIMIT 1",params![task,node.step.id],|r|r.get(0))?;
            if source != "jev-r"
                || saved["node"]["role"] != "solver"
                || saved["node"]["cycle"] != 0
                || saved["call"] != audit["reserved_calls"]
                || result_event <= running_event
            {
                bail!("Latest inference is not the running advisory result")
            }
            let text = saved["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing advisory result text"))?;
            node.state = "unverified".into();
            node.result = Some(text.chars().take(1200).collect());
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'subtask','jev-plus',?2,unixepoch())",params![task,json!({"node":node,"origin":"operator_advisory_recovery"}).to_string()])?;
            // Validate the complete prospective checkpoint before either audit row
            // commits. Failure rolls back the synthetic normal graph transition.
            continuation::validate_checkpoint(&tx, task)?;
            let report = json!({"format_version":1,"task_id":task,"subtask_id":node.step.id,"call":saved["call"],"result_sha256":sha256(node.result.as_deref().unwrap().as_bytes()),"state":"unverified","result_verified":false,"inference_replayed":false,"original_budget_preserved":true,"resume_authorized":false});
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'advisory_recovery','human-cli',?2,unixepoch())",params![task,report.to_string()])?;
            if started.elapsed() > Duration::from_secs(5) {
                bail!("Advisory recovery admission deadline exhausted")
            }
            tx.commit()?;
            Ok(report)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        result
    }
}
