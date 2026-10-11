//! Explicit continuation of a fully recorded pre-solver advisory graph.
use super::{graph, inference, ExecutionLease};
use crate::{
    backend::{InferenceBackend, LocalBackend},
    config::Config,
    planning::Subtask,
    reasoning::{Node, Resources, RestoredState, Session},
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

struct Checkpoint {
    cfg: Config,
    query: String,
    context: String,
    mode: String,
    subtasks: Vec<Subtask>,
    boundary: Option<Value>,
    state: RestoredState,
}

fn prepare(db: &rusqlite::Connection, task: &str) -> Result<Checkpoint> {
    let (status, graph, subtasks) = graph::graph_from(db, task)?;
    if !matches!(status.as_str(), "running" | "interrupted") {
        bail!("Task is not abandoned or interrupted")
    }
    let graph = graph.ok_or_else(|| anyhow::anyhow!("No saved advisory graph"))?;
    if subtasks
        .iter()
        .any(|n| !matches!(n.state.as_str(), "pending" | "unverified"))
    {
        bail!("Continuation requires pending or recorded advisory nodes; running outcomes need reconciliation")
    }
    let query: String = db.query_row(
        "SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END FROM tasks WHERE id=?1",
        [task],
        |r| r.get(0),
    )?;
    let audit = inference::audit_from(db, task)?;
    if audit["results_not_recorded"] != 0 || audit["legacy_results_without_call_identity"] != 0 {
        bail!("Unresolved inference prevents graph continuation")
    }
    let mut snapshots = BTreeMap::<String, Value>::new();
    let mut nodes = Vec::new();
    let mut solver_texts = BTreeMap::new();
    let mut planner_texts = BTreeMap::new();
    let mut bytes = 0usize;
    let mut stmt = db.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('execution_settings','execution_options','model_registration','module_snapshot','reasoning_context','reasoning') ORDER BY id LIMIT 70")?;
    let mut rows = stmt.query([task])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        let kind: String = row.get(0)?;
        let source: String = row.get(1)?;
        let raw: String = row.get(2)?;
        bytes += raw.len();
        if count > 69 || bytes > 8_000_000 {
            bail!("Checkpoint exceeds bounded history")
        }
        let payload: Value = serde_json::from_str(&raw)?;
        if kind == "reasoning" {
            if source != "jev-r" {
                bail!("Invalid saved reasoning source")
            }
            let node: Node = serde_json::from_value(payload["node"].clone())?;
            if node.id != nodes.len()
                || node.parent != nodes.last().map(|n: &Node| n.id)
                || node.cycle > 16
                || nodes.last().is_some_and(|n: &Node| n.cycle > node.cycle)
            {
                bail!("Saved reasoning nodes are not contiguous")
            }
            if node.role == "planner" {
                planner_texts.insert(
                    payload["call"].as_u64().unwrap_or(0),
                    payload["text"].as_str().unwrap_or("").to_string(),
                );
            }
            if node.role == "solver" {
                solver_texts.insert(
                    payload["call"].as_u64().unwrap_or(0),
                    payload["text"].as_str().unwrap_or("").to_string(),
                );
            }
            nodes.push(node);
        } else {
            let expected = match kind.as_str() {
                "module_snapshot" => "fabric",
                "reasoning_context" => "jev-r",
                _ => "runtime",
            };
            if source != expected || snapshots.insert(kind, payload).is_some() {
                bail!("Invalid or repeated checkpoint snapshot")
            }
        }
    }
    let settings = snapshots
        .get("execution_settings")
        .ok_or_else(|| anyhow::anyhow!("Missing original settings"))?;
    let cfg: Config = serde_json::from_value(settings["configuration"].clone())?;
    cfg.validate()?;
    if let Some(selection) = &cfg.model_registration {
        let registered = crate::models::registered_from(db, selection)?
            .ok_or_else(|| anyhow::anyhow!("Original registered model is missing"))?;
        let saved = snapshots
            .get("model_registration")
            .ok_or_else(|| anyhow::anyhow!("Missing original model registration snapshot"))?;
        if !registered.enabled
            || saved["registration"] != serde_json::to_value(&registered)?
            || saved["weight_identity_verified"] != false
            || cfg.backend != "openai-local"
            || cfg.model != registered.manifest.model
            || cfg.api_base != registered.manifest.api_base
            || !cfg.role_profiles.is_empty()
            || cfg.max_tokens > registered.manifest.max_output_tokens as usize
        {
            bail!("Original registered model is disabled or differs from its pinned configuration")
        }
    } else if snapshots
        .get("model_registration")
        .is_some_and(|saved| !saved["registration"].is_null())
    {
        bail!("Unexpected registered model snapshot")
    }
    let mode = settings["mode"].as_str().unwrap_or("").to_string();
    let options = snapshots
        .get("execution_options")
        .ok_or_else(|| anyhow::anyhow!("Missing execution options"))?;
    if options["format_version"] != 1
        || !matches!(options["profile"].as_str(), Some("full" | "collective"))
        || options["explain"] != false
        || !options["conversation_ref"].is_null()
    {
        bail!("Unsupported continuation execution options")
    }
    let modules = snapshots
        .get("module_snapshot")
        .ok_or_else(|| anyhow::anyhow!("Missing pinned modules"))?;
    let expected =
        json!([{"id":"inference.local","version":"1.0.0","roles":crate::fabric::protocol::ROLES}]);
    if modules["modules"] != expected {
        bail!("Unsupported pinned inference modules")
    }
    let context = snapshots
        .get("reasoning_context")
        .ok_or_else(|| anyhow::anyhow!("Missing immutable context"))?;
    let text = context["context"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing context bytes"))?;
    if context["format_version"] != 1
        || context["objective"] != query
        || text.len() > 96000
        || context["objective_hash"] != sha256(query.as_bytes())
        || context["context_hash"] != sha256(text.as_bytes())
        || context["prompt_base_hash"]
            != sha256(Session::prompt_base(&query, text, true)?.as_bytes())
        || context["untrusted_data"] != true
        || context["permissions_restorable"] != false
        || context["cycles_before"] != 0
    {
        bail!("Invalid original reasoning context")
    }
    let calls_before = context["calls_before"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing context charge anchor"))?;
    let recorded_before: u64 = db.query_row("SELECT count(*) FROM events WHERE task_id=?1 AND kind='model_dispatch' AND id < (SELECT id FROM events WHERE task_id=?1 AND kind='reasoning_context')", [task], |r| r.get(0))?;
    if calls_before != recorded_before {
        bail!("Context charge anchor differs from dispatch history")
    }
    if audit["remaining_inference_admission"]["cycles"]
        .as_u64()
        .is_none()
        || audit["remaining_inference_admission"]["tool_calls"]
            .as_u64()
            .is_none()
    {
        bail!("Missing durable cycle or tool allowance")
    }
    use rusqlite::OptionalExtension;
    let boundary_row: Option<(u64,String,String)> = db.query_row("SELECT id,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning_boundary' ORDER BY id DESC LIMIT 1",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let boundary = boundary_row.map(|(id,source,raw)| -> Result<Value> {
        if source != "jev-r" { bail!("Invalid reasoning boundary source") }
        let v: Value = serde_json::from_str(&raw)?;
        let later: u64 = db.query_row("SELECT count(*) FROM events WHERE task_id=?1 AND id>?2 AND kind IN ('model_dispatch','cycle_reservation','tool_reservation')",rusqlite::params![task,id],|r|r.get(0))?;
        let admission=&audit["remaining_inference_admission"];
        let cycles=settings["cycle_limit"].as_u64().unwrap()-admission["cycles"].as_u64().unwrap();
        let previous=v["previous_answer"].as_str().ok_or_else(||anyhow::anyhow!("Missing prior answer"))?;
        let strategy=v["strategy"].as_str().ok_or_else(||anyhow::anyhow!("Missing revision strategy"))?;
        if later != 0 || v["format_version"] != 1 || v["phase"] != "ready_solver"
            || v["mode"] != mode || v["objective_hash"] != context["objective_hash"]
            || v["context_hash"] != context["context_hash"] || v["prompt_base_hash"] != context["prompt_base_hash"]
            || v["calls_before"] != audit["reserved_calls"] || v["cycles_before"] != cycles
            || v["tool_calls_before"] != cfg.max_tool_calls as u64-admission["tool_calls"].as_u64().unwrap()
            || v["reserved_output_tokens"] != audit["reserved_output_tokens"]
            || v["charged_prompt_bytes"] != audit["charged_prompt_bytes"]
            || v["permissions_restorable"] != false || v["subtask_results_verified"] != false
            || previous.len()>96000 || strategy.chars().count()>4000
            || (cycles==0 && (!previous.is_empty() || !v["last_answer_hash"].is_null()))
            || (cycles>0 && (v["last_answer_hash"] != sha256(previous.as_bytes())
                || solver_texts.last_key_value().is_none_or(|(_,text)|text!=previous)))
            || subtasks.iter().any(|n|n.state!="unverified")
        { bail!("Reasoning boundary is stale, inconsistent or unsupported") }
        Ok(v)
    }).transpose()?;
    let tail = audit["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["call"].as_u64().unwrap() > calls_before)
        .collect::<Vec<_>>();
    if tail.first().is_none_or(|c| c["role"] != "planner")
        || (boundary.is_none() && tail.iter().skip(1).any(|c| c["role"] != "solver"))
    {
        bail!("Checkpoint is not in the pre-solver graph phase")
    }
    let planner = planner_texts
        .get(&tail[0]["call"].as_u64().unwrap())
        .ok_or_else(|| anyhow::anyhow!("Missing recorded planner result"))?;
    if serde_json::to_value(crate::planning::Plan::parse(planner)?)?
        != serde_json::to_value(&graph)?
    {
        bail!("Saved graph differs from charged planner result")
    }
    let completed = graph
        .order()?
        .into_iter()
        .filter(|i| subtasks[*i].state == "unverified")
        .collect::<Vec<_>>();
    if (boundary.is_none() && tail.len() != completed.len() + 1) || tail.len() < completed.len() + 1
    {
        bail!("Saved graph and inference outcomes do not match")
    }
    if graph
        .order()?
        .iter()
        .take(completed.len())
        .copied()
        .collect::<Vec<_>>()
        != completed
    {
        bail!("Completed nodes are not a scheduler prefix")
    }
    for (call, index) in tail.iter().skip(1).zip(completed) {
        if call["role"] != "solver" {
            bail!("Invalid advisory result role")
        }
        let answer = solver_texts
            .get(&call["call"].as_u64().unwrap())
            .ok_or_else(|| anyhow::anyhow!("Missing advisory solver result"))?;
        if subtasks[index].result.as_deref()
            != Some(answer.chars().take(1200).collect::<String>().as_str())
        {
            bail!("Saved advisory result differs from charged inference")
        }
    }
    let admission = &audit["remaining_inference_admission"];
    if (boundary.is_none() && admission["cycles"].as_u64() != settings["cycle_limit"].as_u64())
        || admission["cycles"].as_u64().unwrap_or(0) == 0
        || admission["tool_calls"].as_u64().is_none()
        || admission["calls"].as_u64().unwrap_or(0) == 0
    {
        bail!("No valid pre-solver remaining admission")
    }
    let clock: crate::execution_clock::Snapshot =
        serde_json::from_value(settings["execution_clock"].clone())?;
    clock.validate(cfg.max_wall_seconds)?;
    let observed = Instant::now();
    let remaining = clock.remaining()?["remaining_ns"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| anyhow::anyhow!("Original execution deadline is expired or unavailable"))?;
    security::TaskPolicy::reasoning(task)?.authorize(task, "jev-r", "llm.generate")?;
    let used_cycles =
        settings["cycle_limit"].as_u64().unwrap() - admission["cycles"].as_u64().unwrap();
    if nodes.iter().any(|n| n.cycle as u64 > used_cycles) {
        bail!("Reasoning node exceeds charged cycle history")
    }
    let resources = Resources {
        calls: audit["reserved_calls"].as_u64().unwrap() as u32,
        cycles: (settings["cycle_limit"].as_u64().unwrap() - admission["cycles"].as_u64().unwrap())
            as u32,
        tool_calls: cfg.max_tool_calls - admission["tool_calls"].as_u64().unwrap() as u32,
        reserved_output_tokens: audit["reserved_output_tokens"].as_u64().unwrap() as usize,
        prompt_bytes: audit["charged_prompt_bytes"].as_u64().unwrap() as usize,
        input_tokens: audit["input_tokens"].as_u64(),
        output_tokens: audit["output_tokens"].as_u64(),
        ..Default::default()
    };
    Ok(Checkpoint {
        cfg,
        query,
        context: text.into(),
        mode,
        subtasks,
        boundary,
        state: RestoredState {
            resources,
            nodes,
            deadline: observed + Duration::from_nanos(remaining),
        },
    })
}

pub async fn resume(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Task continuation requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    let path = store
        .runtime_lock
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Continuation requires a persistent store"))?;
    let mut lease = Some(
        ExecutionLease::acquire(path, true)
            .map_err(|_| anyhow::anyhow!("Cannot continue while runtime leases are held"))?,
    );
    let checkpoint = {
        let mut db = store.lock()?;
        let started = Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(5)),
        );
        let prepared = (|| -> Result<Checkpoint> {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let mut checkpoint = prepare(&tx, task)?;
            if checkpoint.cfg.inference_process {
                checkpoint.cfg.home = path
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("Missing worker workspace"))?
                    .into();
                use std::os::unix::fs::PermissionsExt;
                let metadata = std::fs::symlink_metadata(&checkpoint.cfg.home)?;
                if !metadata.is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
                    bail!("Managed continuation requires a private existing workspace")
                }
                let mut expected = checkpoint.cfg.db().into_os_string();
                expected.push(".runtime.lock");
                if path.as_os_str() != expected {
                    bail!("Managed continuation requires the canonical workspace database")
                }
                // No inference has begun. A competing exclusive owner during conversion
                // cannot commit recovery while this write transaction remains open.
                lease = Some(
                    lease
                        .take()
                        .expect("Admission owns its lease")
                        .into_shared()?,
                );
            }
            tx.execute(
                "UPDATE tasks SET status='running',updated=unixepoch() WHERE id=?1",
                [task],
            )?;
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'continuation','human-cli',?2,unixepoch())",rusqlite::params![task,json!({"state":"started","format_version":1,"original_budget_preserved":true,"prior_timing_complete":false,"subtask_results_verified":false}).to_string()])?;
            if started.elapsed() > Duration::from_secs(5) {
                bail!("Continuation admission deadline exhausted")
            }
            tx.commit()?;
            Ok(checkpoint)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        prepared?
    };
    let prior_calls = checkpoint.state.resources.calls;
    let deadline = checkpoint.state.deadline;
    let process = if checkpoint.cfg.inference_process {
        let mut cfg = checkpoint.cfg.clone();
        cfg.max_calls = cfg
            .max_calls
            .min(match checkpoint.mode.as_str() {
                "fast" => 1,
                "standard" => 4,
                "deep" => 10,
                "research" => 18,
                _ => 0,
            })
            .saturating_sub(checkpoint.state.resources.calls as usize);
        cfg.max_tokens = cfg
            .max_tokens
            .saturating_sub(checkpoint.state.resources.reserved_output_tokens);
        cfg.max_input_bytes = cfg
            .max_input_bytes
            .saturating_sub(checkpoint.state.resources.prompt_bytes);
        cfg.max_wall_seconds = deadline
            .saturating_duration_since(Instant::now())
            .as_secs()
            .saturating_add(1)
            .min(cfg.max_wall_seconds);
        Some(Arc::new(crate::fabric::unix::process::ProcessTask::new(
            cfg,
            store.clone(),
            task,
        )?))
    } else {
        None
    };
    let inference: Arc<dyn InferenceBackend> = match &process {
        Some(process) => process.clone(),
        None => Arc::new(LocalBackend(checkpoint.cfg.clone())),
    };
    let backend: Arc<dyn InferenceBackend> =
        Arc::new(crate::fabric::Broker::with_inference(inference));
    let mut session = Session::restored(
        checkpoint.cfg,
        store.clone(),
        backend,
        task.into(),
        &checkpoint.mode,
        checkpoint.state,
    )?;
    let execution = async {
        if let Some(boundary) = &checkpoint.boundary {
            session
                .continue_solver(
                    boundary,
                    &checkpoint.query,
                    &checkpoint.context,
                    &checkpoint.mode,
                )
                .await
        } else {
            session
                .continue_graph(
                    checkpoint.subtasks,
                    &checkpoint.query,
                    &checkpoint.context,
                    &checkpoint.mode,
                )
                .await
        }
    };
    let mut outcome = tokio::select! {
        biased;
        _ = cancel => Err(anyhow::anyhow!("Continuation cancelled")),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(anyhow::anyhow!("Original continuation deadline exhausted")),
        result = execution => result,
    };
    if let Some(process) = &process {
        if let Err(error) = process.shutdown().await {
            outcome = Err(error);
        }
    }
    session.finish_metrics();
    let (answer, status) = match outcome {
        Ok(answer) if session.verified => (answer, "completed"),
        Ok(answer) if session.has_solution => (answer, "unverified"),
        Ok(answer) => (answer, "exhausted"),
        Err(_) => (
            "Continuation interrupted; inspect durable call outcomes before another attempt".into(),
            "interrupted",
        ),
    };
    session.finish_subtasks(if status == "interrupted" {
        "failed"
    } else {
        status
    })?;
    let report = json!({"format_version":1,"task_id":task,"status":status,"answer":answer,
        "verified":session.verified,"criterion":session.verified_criterion,
        "new_calls":session.resources.calls-prior_calls,"charged_resources":session.resources,
        "timing_scope":"continuation_only","prior_timing_complete":false,"subtask_results_verified":false,"automatic_replay":false});
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let updated = tx.execute(
        "UPDATE tasks SET status=?2,answer=?3,updated=unixepoch() WHERE id=?1 AND status='running'",
        rusqlite::params![task, status, answer],
    )?;
    if updated != 1 {
        bail!("Continuation lost its running task anchor")
    }
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'continuation','human-cli',?2,unixepoch())",rusqlite::params![task,serde_json::to_string(&report)?])?;
    let outcome = json!({"status":status,"resources":session.resources,"verified":session.verified,"origin":"operator_continuation","timing_scope":"continuation_only"});
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'outcome','jev-plus',?2,unixepoch())",rusqlite::params![task,outcome.to_string()])?;
    let progress = crate::events::Progress::from_audit(
        tx.last_insert_rowid() as u64,
        task,
        "outcome",
        "jev-plus",
        &outcome,
    )?
    .expect("Outcome has a typed event");
    tx.commit()?;
    drop(db);
    store.events.publish(&progress)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Generation;
    use std::{collections::VecDeque, future::Future, pin::Pin, sync::Mutex};
    const QUERY: &str = r#"sort:{"values":[3,1,2]}"#;
    const PLAN: &str = r#"{"steps":[{"id":"a","objective":"Inspect input","depends_on":[]},{"id":"b","objective":"Combine advice","depends_on":["a"]}]}"#;
    struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
    impl InferenceBackend for Script {
        fn generate<'a>(
            &'a self,
            role: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            let (expected, text) = self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("Unexpected repeated inference");
            assert_eq!(role, expected);
            Box::pin(async move {
                Ok(Generation {
                    text: text.into(),
                    model: "fixture/v1".into(),
                    input_tokens: Some(20),
                    output_tokens: Some(10),
                    elapsed_ms: 1.0,
                    queue_wait_ms: None,
                })
            })
        }
    }
    async fn fixture() -> (Arc<Store>, String) {
        fixture_in(Arc::new(Store::in_memory().unwrap())).await
    }
    async fn fixture_in(store: Arc<Store>) -> (Arc<Store>, String) {
        let task = store.create_task(QUERY, "deep").unwrap();
        let cfg = Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            max_tokens: 3000,
            ..Config::default()
        };
        let backend = Arc::new(Script(Mutex::new(
            vec![("planner", PLAN), ("solver", "original advice")].into(),
        )));
        let mut session = Session::new(cfg, store.clone(), backend, task.clone(), "deep").unwrap();
        store.event(&task,"execution_options","runtime",json!({"format_version":1,"profile":"full","explain":false,"conversation_ref":null})).unwrap();
        store.event(&task,"module_snapshot","fabric",json!({"modules":[{"id":"inference.local","version":"1.0.0","roles":crate::fabric::protocol::ROLES}]})).unwrap();
        store.event(&task,"reasoning_context","jev-r",json!({"format_version":1,"objective":QUERY,"context":"","objective_hash":sha256(QUERY.as_bytes()),"context_hash":sha256(b""),"prompt_base_hash":sha256(Session::prompt_base(QUERY,"",true).unwrap().as_bytes()),"calls_before":0,"cycles_before":0,"untrusted_data":true,"permissions_restorable":false})).unwrap();
        session
            .call("planner", "planning", 256)
            .await
            .unwrap()
            .unwrap();
        let plan = crate::planning::Plan::parse(PLAN).unwrap();
        store
            .event(
                &task,
                "plan",
                "planner-r",
                json!({"graph":plan,"trusted_permissions":false}),
            )
            .unwrap();
        let mut nodes = plan
            .steps
            .into_iter()
            .map(|step| Subtask {
                step,
                state: "pending".into(),
                result: None,
            })
            .collect::<Vec<_>>();
        for node in &nodes {
            store
                .event(&task, "subtask", "jev-plus", json!({"node":node}))
                .unwrap();
        }
        nodes[0].state = "running".into();
        store
            .event(&task, "subtask", "jev-plus", json!({"node":nodes[0]}))
            .unwrap();
        session
            .call("solver", "advisory", 192)
            .await
            .unwrap()
            .unwrap();
        nodes[0].state = "unverified".into();
        nodes[0].result = Some("original advice".into());
        store
            .event(&task, "subtask", "jev-plus", json!({"node":nodes[0]}))
            .unwrap();
        store.finish_task(&task, "interrupted", "").unwrap();
        (store, task)
    }
    #[tokio::test]
    async fn continues_only_pending_nodes_and_preserves_original_charges() {
        let (store, task) = fixture().await;
        let saved = prepare(&store.lock().unwrap(), &task).unwrap();
        assert_eq!(saved.state.resources.calls, 2);
        let backend = Arc::new(Script(Mutex::new(
            vec![("solver", "remaining advice"), ("solver", "[1,2,3]")].into(),
        )));
        let mut session = Session::restored(
            saved.cfg,
            store.clone(),
            backend.clone(),
            task.clone(),
            &saved.mode,
            saved.state,
        )
        .unwrap();
        let answer = session
            .continue_graph(saved.subtasks, &saved.query, &saved.context, &saved.mode)
            .await
            .unwrap();
        assert_eq!(answer, "[1,2,3]");
        assert!(session.verified);
        assert_eq!(session.resources.calls, 4);
        assert_eq!(session.resources.reserved_output_tokens, 1024);
        assert!(backend.0.lock().unwrap().is_empty());
        assert_eq!(
            inference::audit_from(&store.lock().unwrap(), &task).unwrap()["results_not_recorded"],
            0
        );
    }
    #[tokio::test]
    async fn rejects_changed_graph_context_and_unresolved_dispatch() {
        for variant in 0..4 {
            let (store, task) = fixture().await;
            match variant {
                0 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.graph.steps[0].objective','Changed objective') WHERE task_id=?1 AND kind='plan'",[&task]).unwrap();
                }
                1 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.calls_before',1) WHERE task_id=?1 AND kind='reasoning_context'",[&task]).unwrap();
                }
                2 => {
                    store
                        .event(
                            &task,
                            "model_dispatch",
                            "jev-r",
                            json!({"call":3,"role":"solver","allocation":192,"prompt_bytes":10}),
                        )
                        .unwrap();
                }
                _ => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.context','Changed context') WHERE task_id=?1 AND kind='reasoning_context'",[&task]).unwrap();
                }
            }
            assert!(
                prepare(&store.lock().unwrap(), &task).is_err(),
                "Accepted variant {variant}"
            );
        }
    }
    fn register_saved_model(store: &Store, task: &str) -> crate::models::Selection {
        let raw: String = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM events WHERE task_id=?1 AND kind='execution_settings'",
                [task],
                |r| r.get(0),
            )
            .unwrap();
        let mut settings: Value = serde_json::from_str(&raw).unwrap();
        let cfg: Config = serde_json::from_value(settings["configuration"].clone()).unwrap();
        let manifest = crate::models::Manifest {
            id: "local.resume".into(),
            version: "1.0.0".into(),
            model: cfg.model,
            api_base: cfg.api_base,
            source: "operator-fixture".into(),
            license: "CC0".into(),
            context_tokens: 8192,
            max_output_tokens: 4096,
            weights_sha256: None,
        };
        let file =
            std::env::temp_dir().join(format!("fabric-resume-model-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let registered = store.register_model(&file).unwrap();
        std::fs::remove_file(file).unwrap();
        let selection = crate::models::Selection {
            id: manifest.id,
            version: manifest.version,
        };
        settings["configuration"]["model_registration"] = serde_json::to_value(&selection).unwrap();
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE events SET payload=?2 WHERE task_id=?1 AND kind='execution_settings'",
                rusqlite::params![task, settings.to_string()],
            )
            .unwrap();
        store
            .event(
                task,
                "model_registration",
                "runtime",
                json!({"registration":registered,"weight_identity_verified":false}),
            )
            .unwrap();
        selection
    }
    #[tokio::test]
    async fn registered_resume_requires_enabled_unchanged_pinned_configuration() {
        for variant in 0..6 {
            let (store, task) = fixture().await;
            let selection = register_saved_model(&store, &task);
            assert!(prepare(&store.lock().unwrap(), &task).is_ok());
            match variant {
                0 => store.set_model_enabled(&selection, false).unwrap(),
                1 => {
                    store
                        .lock()
                        .unwrap()
                        .execute("DELETE FROM model_manifests", [])
                        .unwrap();
                }
                2 => {
                    store
                        .lock()
                        .unwrap()
                        .execute("UPDATE model_manifests SET checksum=?1", ["0".repeat(64)])
                        .unwrap();
                }
                3 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.registration.manifest.model','changed') WHERE kind='model_registration'",[]).unwrap();
                }
                4 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.model','changed') WHERE kind='execution_settings'",[]).unwrap();
                }
                _ => {
                    store
                        .lock()
                        .unwrap()
                        .execute("DELETE FROM events WHERE kind='model_registration'", [])
                        .unwrap();
                }
            }
            assert!(
                prepare(&store.lock().unwrap(), &task).is_err(),
                "Accepted registration mutation {variant}"
            );
        }
    }
    #[tokio::test]
    async fn public_resume_uses_real_http_and_commits_a_durable_outcome() {
        http_continuation(false, false).await;
    }
    #[tokio::test]
    async fn registered_resume_uses_the_original_http_model() {
        http_continuation(true, false).await;
    }
    #[tokio::test]
    async fn public_resume_continues_a_saved_revision_through_http() {
        http_continuation(false, true).await;
    }
    async fn http_continuation(registered: bool, revision: bool) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let root =
            std::env::temp_dir().join(format!("fabric-continuation-{}", uuid::Uuid::new_v4()));
        let (store, task) =
            fixture_in(Arc::new(Store::open(&root.join("state.db")).unwrap())).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",rusqlite::params![task,endpoint]).unwrap();
        if registered {
            register_saved_model(&store, &task);
        }
        if revision {
            save_revision_boundary(&store, &task).await;
        }
        assert!(resume(store.clone(), &task, false, std::future::pending())
            .await
            .is_err());
        let lease = store.execution_lease().unwrap();
        assert!(resume(store.clone(), &task, true, std::future::pending())
            .await
            .is_err());
        drop(lease);
        let server = tokio::spawn(async move {
            for answer in if revision {
                vec!["[1,2,3]"]
            } else {
                vec!["remaining advice", "[1,2,3]"]
            } {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                    assert!(request.len() < 131072);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let length = headers
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|n| n.parse::<usize>().ok())
                            })
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            let body: Value =
                                serde_json::from_slice(&request[end + 4..end + 4 + length])
                                    .unwrap();
                            assert_eq!(body["model"], "local-model");
                            break;
                        }
                    }
                }
                let body=json!({"model":"fixture/v1","choices":[{"message":{"content":answer}}],"usage":{"prompt_tokens":20,"completion_tokens":10}}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
            }
        });
        let report = tokio::time::timeout(
            Duration::from_secs(10),
            resume(store.clone(), &task, true, std::future::pending()),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(report["status"], "completed");
        assert_eq!(report["new_calls"], if revision { 1 } else { 2 });
        server.await.unwrap();
        let db = store.lock().unwrap();
        let (status, answer): (String, String) = db
            .query_row(
                "SELECT status,answer FROM tasks WHERE id=?1",
                [&task],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((status.as_str(), answer.as_str()), ("completed", "[1,2,3]"));
        let count: u32 = db
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='execution_settings'",
                [&task],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            inference::audit_from(&db, &task).unwrap()["reserved_calls"],
            if revision { 5 } else { 4 }
        );
        assert!(prepare(&db, &task).is_err());
        drop(db);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn cancellation_and_terminal_audit_failure_preserve_durable_charges() {
        for fail in [false, true] {
            let root = std::env::temp_dir().join(format!(
                "fabric-cancel-continuation-{}",
                uuid::Uuid::new_v4()
            ));
            let (store, task) =
                fixture_in(Arc::new(Store::open(&root.join("state.db")).unwrap())).await;
            if fail {
                store.lock().unwrap().execute_batch("CREATE TRIGGER reject_finished BEFORE INSERT ON events WHEN NEW.kind='outcome' BEGIN SELECT RAISE(ABORT,'fixture outcome failure'); END;").unwrap();
            }
            let result = resume(store.clone(), &task, true, std::future::ready(())).await;
            if fail {
                assert!(result.is_err())
            } else {
                let report = result.unwrap();
                assert_eq!(report["status"], "interrupted");
                assert_eq!(report["new_calls"], 0);
            }
            let db = store.lock().unwrap();
            let status: String = db
                .query_row("SELECT status FROM tasks WHERE id=?1", [&task], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(status, if fail { "running" } else { "interrupted" });
            assert_eq!(
                inference::audit_from(&db, &task).unwrap()["reserved_calls"],
                2
            );
            drop(db);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn refuses_unknown_expired_clocks_and_unsupported_configuration() {
        for mutation in [
            "json_set(payload,'$.execution_clock',NULL)",
            "json_set(payload,'$.execution_clock.started_ns',0,'$.execution_clock.deadline_ns',json_extract(payload,'$.configuration.max_wall_seconds')*1000000000)",

        ] {
            let (store,task)=fixture().await;
            store.lock().unwrap().execute(&format!("UPDATE events SET payload={mutation} WHERE task_id=?1 AND kind='execution_settings'"),[&task]).unwrap();
            assert!(prepare(&store.lock().unwrap(),&task).is_err(),"Accepted {mutation}");
        }
    }
    async fn save_revision_boundary(store: &Arc<Store>, task: &str) {
        let saved = prepare(&store.lock().unwrap(), task).unwrap();
        let backend = Arc::new(Script(Mutex::new(
            vec![
                ("solver", "remaining advice"),
                ("solver", "[3,2,1]"),
                ("solver", "[1,2,3]"),
            ]
            .into(),
        )));
        let mut original = Session::restored(
            saved.cfg,
            store.clone(),
            backend,
            task.to_string(),
            &saved.mode,
            saved.state,
        )
        .unwrap();
        original
            .continue_graph(saved.subtasks, &saved.query, &saved.context, &saved.mode)
            .await
            .unwrap();
        // Retain exactly the committed boundary preceding the second solver.
        // This models process loss before its next cycle reservation/dispatch.
        let db = store.lock().unwrap();
        let id:u64=db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.cycles_before')=1",[task],|r|r.get(0)).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            rusqlite::params![task, id],
        )
        .unwrap();
        db.execute("DELETE FROM events WHERE task_id=?1 AND kind='reasoning_context' AND id!=(SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_context')",[task]).unwrap();
        drop(db);
    }
    #[tokio::test]
    async fn resumes_a_revision_boundary_and_retains_repeat_stopping_state() {
        for repeat in [false, true] {
            let (store, task) = fixture().await;
            save_revision_boundary(&store, &task).await;
            let saved = prepare(&store.lock().unwrap(), &task).unwrap();
            let original_boundary = saved.boundary.clone().unwrap();
            for (field, value) in [
                ("phase", json!("unknown")),
                ("calls_before", json!(0)),
                ("previous_answer", json!("changed")),
                ("last_answer_hash", json!("changed")),
                ("tool_calls_before", json!(0)),
            ] {
                let mut changed = original_boundary.clone();
                changed[field] = value;
                store.lock().unwrap().execute("UPDATE events SET payload=?2 WHERE task_id=?1 AND kind='reasoning_boundary' AND id=(SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary')",rusqlite::params![task,changed.to_string()]).unwrap();
                assert!(
                    prepare(&store.lock().unwrap(), &task).is_err(),
                    "Accepted altered {field}"
                );
            }
            store.lock().unwrap().execute("UPDATE events SET payload=?2 WHERE task_id=?1 AND kind='reasoning_boundary' AND id=(SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary')",rusqlite::params![task,original_boundary.to_string()]).unwrap();
            store
                .event(
                    &task,
                    "cycle_reservation",
                    "jev-r",
                    json!({"format_version":1,"cycle":2}),
                )
                .unwrap();
            assert!(
                prepare(&store.lock().unwrap(), &task).is_err(),
                "Consumed boundary was accepted"
            );
            store
                .lock()
                .unwrap()
                .execute(
                    "DELETE FROM events WHERE id=(SELECT max(id) FROM events WHERE task_id=?1)",
                    [&task],
                )
                .unwrap();
            assert_eq!(saved.state.resources.cycles, 1);
            assert_eq!(saved.state.resources.calls, 4);
            let backend = Arc::new(Script(Mutex::new(
                vec![("solver", if repeat { "[3,2,1]" } else { "[1,2,3]" })].into(),
            )));
            let mut resumed = Session::restored(
                saved.cfg,
                store.clone(),
                backend.clone(),
                task.clone(),
                &saved.mode,
                saved.state,
            )
            .unwrap();
            let answer = resumed
                .continue_solver(
                    saved.boundary.as_ref().unwrap(),
                    &saved.query,
                    &saved.context,
                    &saved.mode,
                )
                .await
                .unwrap();
            assert_eq!(resumed.resources.calls, 5);
            assert_eq!(resumed.resources.cycles, 2);
            assert_eq!(resumed.resources.reserved_output_tokens, 1408);
            assert!(backend.0.lock().unwrap().is_empty());
            assert_eq!(resumed.verified, !repeat);
            assert_eq!(answer, if repeat { "[3,2,1]" } else { "[1,2,3]" });
            if repeat {
                assert_eq!(resumed.stop_reason, "no_progress");
            }
        }
    }
}
