//! Explicit restoration of a checkpointed, never-dispatched language call.
use super::{inference, ExecutionLease};
use crate::{
    backend::{InferenceBackend, LocalBackend},
    config::Config,
    language::PresentationCheckpoint,
    reasoning::{Node, Resources, Session, TerminalState},
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use rusqlite::params;
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct Admission {
    cfg: Config,
    mode: String,
    checkpoint: PresentationCheckpoint,
    state: TerminalState,
    prior_calls: u32,
}

fn admit(store: &Store, task: &str) -> Result<Admission> {
    let mut db = store.lock()?;
    let started = Instant::now();
    db.progress_handler(
        1000,
        Some(move || started.elapsed() > Duration::from_secs(5)),
    );
    let result = (|| -> Result<Admission> {
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::validate_verifier_recovery(&tx, task, None)?;
        let (query, status): (String, String) = tx.query_row(
            "SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END,status FROM tasks WHERE id=?1",
            [task], |row| Ok((row.get(0)?, row.get(1)?)))?;
        if !matches!(status.as_str(), "running" | "interrupted") {
            bail!("Terminal presentation restoration requires an abandoned or interrupted task")
        }
        let audit = inference::audit_from(&tx, task)?;
        if audit["language_boundary"]["phase"] != "presentation_ready"
            || audit["results_not_recorded"] != 0
            || audit["legacy_results_without_call_identity"] != 0
        {
            bail!("Only an exact never-dispatched presentation checkpoint can resume")
        }
        let raw: String = tx.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 1", [task], |row|row.get(0))?;
        let settings = crate::strict_json::value(raw.as_bytes(), 262144)?;
        let cfg: Config = serde_json::from_value(settings["configuration"].clone())?;
        cfg.validate()?;
        if settings["runtime_role_prompts_sha256"] != crate::backend::runtime_role_prompts_sha256()?
        {
            bail!("Original compiled language prompts are unavailable or changed")
        }
        // Preserve unsupported scopes rather than bypassing their restoration.
        if cfg.backend != "openai-local"
            || cfg.resource_admission.is_some()
            || cfg.input_token_admission.is_some()
            || cfg.model_registration.is_some()
        {
            bail!("Terminal presentation restoration requires the configured local adapter without registered/resource/token-admission scopes")
        }
        let raw: String = tx.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=65536 THEN payload END FROM events WHERE task_id=?1 AND kind='module_snapshot' AND source='fabric' ORDER BY id LIMIT 1",[task],|row|row.get(0))?;
        let modules = crate::strict_json::value(raw.as_bytes(), 65536)?;
        if modules["modules"].as_array().is_none_or(|entries| {
            entries.len() != 1
                || entries[0]["id"] != "inference.local"
                || entries[0]["version"] != "1.0.0"
        }) {
            bail!("Terminal restoration cannot replace the original named inference module")
        }
        let mut expected_lock = cfg.db().into_os_string();
        expected_lock.push(".runtime.lock");
        if store
            .runtime_lock
            .as_deref()
            .map(std::path::Path::as_os_str)
            != Some(expected_lock.as_os_str())
        {
            bail!("Original presentation workspace does not match the owned store")
        }
        let (raw, checkpoint_id): (String, i64) = tx.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=65536 THEN payload END,id FROM events WHERE task_id=?1 AND kind='language_presentation_ready' ORDER BY id LIMIT 1", [task], |row|Ok((row.get(0)?,row.get(1)?)))?;
        let checkpoint: PresentationCheckpoint =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 65536)?)?;
        checkpoint.validate(&cfg, &query)?;
        if checkpoint.call != audit["reserved_calls"].as_u64().unwrap_or(0) as usize + 1 {
            bail!("Pending language call identity changed")
        }
        if checkpoint.status == "completed" {
            let proof: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='verification' AND source='verifier-r' AND json_extract(payload,'$.verified')=1 AND json_extract(payload,'$.objective_hash')=?2 AND json_extract(payload,'$.answer_hash')=?3 AND id<?4)",
                params![task,checkpoint.objective_sha256,checkpoint.answer_sha256,checkpoint_id], |row| row.get(0))?;
            if !proof {
                bail!("Completed presentation checkpoint lacks an exact independent proof binding")
            }
        }
        let clock: crate::execution_clock::Snapshot =
            serde_json::from_value(settings["execution_clock"].clone())?;
        clock.validate(cfg.max_wall_seconds)?;
        let observed_at = Instant::now();
        let remaining = clock.remaining()?;
        let remaining_ns = remaining["remaining_ns"]
            .as_u64()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                anyhow::anyhow!("Original presentation clock expired or belongs to another boot")
            })?;
        if remaining["status"] != "same_boot" {
            bail!("Original same-boot clock is unavailable")
        }
        let deadline = observed_at + Duration::from_nanos(remaining_ns);
        let mut nodes = Vec::new();
        let mut statement = tx.prepare("SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning' ORDER BY id LIMIT 65")?;
        let raws = statement
            .query_map([task], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        for raw in raws {
            let value = crate::strict_json::value(raw.as_bytes(), 262144)?;
            let node: Node = serde_json::from_value(value["node"].clone())?;
            if node.id != nodes.len()
                || node.id >= 64
                || node.parent.is_some_and(|parent| parent >= node.id)
            {
                bail!("Original terminal reasoning-node identities are inconsistent")
            }
            nodes.push(node);
        }
        let calls = audit["reserved_calls"].as_u64().unwrap_or(0) as u32;
        if nodes.len() != calls as usize {
            bail!("Original terminal result history is incomplete")
        }
        let resources = Resources {
            calls,
            cycles: tx.query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='cycle_reservation'",
                [task],
                |row| row.get(0),
            )?,
            reserved_output_tokens: audit["reserved_output_tokens"].as_u64().unwrap_or(0) as usize,
            prompt_bytes: audit["charged_prompt_bytes"].as_u64().unwrap_or(0) as usize,
            input_tokens: audit["input_tokens"].as_u64(),
            output_tokens: audit["output_tokens"].as_u64(),
            tool_calls: tx.query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='tool_reservation'",
                [task],
                |row| row.get(0),
            )?,
            ..Resources::default()
        };
        let call_limit = audit["language_boundary"]["reservation"]["root_call_limit"]
            .as_u64()
            .unwrap_or(0) as usize;
        if calls as usize >= call_limit
            || resources
                .reserved_output_tokens
                .saturating_add(checkpoint.allocation)
                > cfg.max_tokens
            || resources
                .prompt_bytes
                .saturating_add(checkpoint.prompt.len())
                > cfg.max_input_bytes
        {
            bail!("Original held presentation capacity is exhausted")
        }
        security::TaskPolicy::reasoning(task)?.authorize(task, "interface", "llm.generate")?;
        if tx.execute(
            "UPDATE tasks SET status='running',updated=unixepoch() WHERE id=?1 AND status=?2",
            params![task, status],
        )? != 1
        {
            bail!("Task changed during terminal presentation admission")
        }
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'language_presentation_resume','human-cli',?2,unixepoch())",
            params![task,json!({"format_version":1,"call":checkpoint.call,"prompt_sha256":checkpoint.prompt_sha256,
                "original_clock_preserved":true,"fresh_budget_granted":false,"automatic_replay":false}).to_string()])?;
        if started.elapsed() > Duration::from_secs(5) || Instant::now() >= deadline {
            bail!("Terminal presentation admission deadline exhausted")
        }
        let mode = settings["mode"].as_str().unwrap_or("").to_owned();
        crate::reasoning::mode_limits(&mode)?;
        tx.commit()?;
        Ok(Admission {
            cfg,
            mode,
            checkpoint,
            state: TerminalState {
                deadline,
                resources,
                nodes,
                call_limit,
            },
            prior_calls: calls,
        })
    })();
    db.progress_handler(0, None::<fn() -> bool>);
    result
}

pub async fn resume(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Pending language presentation requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    let path = store.runtime_lock.as_deref().ok_or_else(|| {
        anyhow::anyhow!("Terminal presentation restoration requires a persistent store")
    })?;
    let _lease = ExecutionLease::acquire(path, true).map_err(|error| {
        error.context("Cannot acquire exclusive runtime lease for terminal presentation")
    })?;
    let Admission {
        cfg,
        mode,
        checkpoint,
        state,
        prior_calls,
    } = admit(&store, task)?;
    let deadline = state.deadline;
    let process = if cfg.inference_process {
        let mut worker_cfg = cfg.clone();
        worker_cfg.max_calls = 1;
        worker_cfg.max_tokens = checkpoint.allocation;
        worker_cfg.max_input_bytes = checkpoint.prompt.len();
        worker_cfg.max_wall_seconds = deadline
            .saturating_duration_since(Instant::now())
            .as_secs()
            .saturating_add(1)
            .min(cfg.max_wall_seconds);
        Some(Arc::new(crate::fabric::unix::process::ProcessTask::new(
            worker_cfg,
            store.clone(),
            task,
        )?))
    } else {
        None
    };
    let backend: Arc<dyn InferenceBackend> = match &process {
        Some(process) => process.clone(),
        None => Arc::new(LocalBackend(cfg.clone())),
    };
    let backend: Arc<dyn InferenceBackend> =
        Arc::new(crate::fabric::Broker::with_inference(backend));
    let mut session =
        Session::restore_terminal(cfg, store.clone(), backend, task.to_owned(), &mode, state)?;
    let result = tokio::select! {
        biased;
        _=cancel => Err(anyhow::anyhow!("Terminal presentation cancelled")),
        _=tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(anyhow::anyhow!("Original terminal presentation deadline exhausted")),
        result=session.call("synthesizer", &checkpoint.prompt, checkpoint.allocation) => result.and_then(|answer| answer.ok_or_else(||anyhow::anyhow!("Original terminal presentation allowance exhausted"))),
    };
    let result = if let Some(process) = &process {
        match process.shutdown().await {
            Ok(()) => result,
            Err(error) => Err(error),
        }
    } else {
        result
    };
    let result = result.and_then(|answer| {
        let input = crate::strict_json::value(checkpoint.prompt.as_bytes(), 8192)?;
        if checkpoint.status == "completed" && input["answer"] != answer {
            bail!("Restored presentation changed independently verified answer bytes")
        }
        Ok(answer)
    });
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (answer, status) = match &result {
        Ok(answer) => (answer.as_str(),checkpoint.status.as_str()),
        Err(_) => ("Terminal presentation interrupted; inspect charged outcomes before any further operation","interrupted"),
    };
    if result.is_ok() {
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'language_boundary_presentation','interface',?2,unixepoch())",
            params![task,json!({"answer_sha256":sha256(answer.as_bytes()),"original_answer_sha256":checkpoint.answer_sha256,
                "status":status,"verified_bytes_preserved":status=="completed","new_claims_verified":false}).to_string()])?;
    }
    if tx.execute(
        "UPDATE tasks SET status=?2,answer=?3,updated=unixepoch() WHERE id=?1 AND status='running'",
        params![task, status, answer],
    )? != 1
    {
        bail!("Terminal presentation lost its task anchor")
    }
    let report = json!({"format_version":1,"task_id":task,"status":status,
        "answer_hash":sha256(answer.as_bytes()),"new_calls":session.resources.calls-prior_calls,
        "resources":session.resources,"timing_scope":"terminal_continuation_only",
        "verified":status=="completed","resource_usage_complete":false,
        "automatic_replay":false,"fresh_budget_granted":false,"provider_stop_proven":false});
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'recovery','human-cli',?2,unixepoch())",params![task,report.to_string()])?;
    let progress = crate::events::Progress::from_audit(
        tx.last_insert_rowid() as u64,
        task,
        "recovery",
        "human-cli",
        &report,
    )?
    .ok_or_else(|| anyhow::anyhow!("Missing typed terminal recovery event"))?;
    tx.commit()?;
    store.events.publish(&progress)?;
    result?;
    Ok(report)
}
