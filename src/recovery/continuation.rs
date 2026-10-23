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
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "continuation_module_history.rs"]
mod module_dispatch_history;
#[path = "continuation_modules.rs"]
mod module_history;
#[cfg(test)]
#[path = "continuation_module_tests.rs"]
mod module_tests;
#[path = "continuation_parallel.rs"]
mod parallel_history;
#[path = "continuation_path_progress.rs"]
mod path_progress_history;
#[path = "continuation_policy.rs"]
mod policy;
#[path = "continuation_progress.rs"]
mod progress_history;
#[cfg(test)]
#[path = "continuation_progress_tests.rs"]
mod progress_tests;
#[path = "continuation_resources.rs"]
mod resource_history;
#[path = "resource_routes.rs"]
mod resource_routes;

struct Checkpoint {
    cfg: Config,
    query: String,
    context: String,
    mode: String,
    subtasks: Vec<Subtask>,
    boundary: Option<Value>,
    parallel_schedule: Option<parallel_history::ParallelContinuation>,
    state: RestoredState,
    admission: Value,
    resource_history: Value,
    resource_routes: Option<BTreeMap<String, crate::backend::InferenceTarget>>,
    resource_route_scope: Option<String>,
    finite_search_history: Option<super::finite_search::Receipt>,
    path_progress_enabled: bool,
    verifier_provider: Option<crate::verification::provider::Handle>,
    typed_modules: Option<module_history::Restored>,
    reconciled_parallel_subtasks: Vec<Subtask>,
}

fn prepare(db: &rusqlite::Connection, task: &str) -> Result<Checkpoint> {
    prepare_with_policy(db, task, &crate::escalation::Handle::default(), None)
}

fn prepare_with_policy(
    db: &rusqlite::Connection,
    task: &str,
    escalator: &crate::escalation::Handle,
    verifier_provider: Option<&crate::verification::provider::Handle>,
) -> Result<Checkpoint> {
    prepare_with_runtime(db, task, escalator, verifier_provider, None)
}

fn prepare_with_runtime(
    db: &rusqlite::Connection,
    task: &str,
    escalator: &crate::escalation::Handle,
    verifier_provider: Option<&crate::verification::provider::Handle>,
    modules: Option<Arc<crate::fabric::modules::Broker>>,
) -> Result<Checkpoint> {
    prepare_with_native(db, task, escalator, verifier_provider, modules, None)
}

fn prepare_with_native(
    db: &rusqlite::Connection,
    task: &str,
    escalator: &crate::escalation::Handle,
    verifier_provider: Option<&crate::verification::provider::Handle>,
    modules: Option<Arc<crate::fabric::modules::Broker>>,
    native: Option<&crate::residency::lifecycle::restoration::NativeRuntime>,
) -> Result<Checkpoint> {
    let typed_modules = module_history::restore(db, task, modules)?;
    super::validate_verifier_recovery_with_native(db, task, verifier_provider, native)?;
    let parallel_recorded: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='subtask_parallel_schedule')",
        [task],
        |row| row.get(0),
    )?;
    let (status, graph, mut subtasks) = graph::graph_from(db, task)?;
    if parallel_recorded && graph.is_none() {
        bail!("Parallel advisory schedule has no pinned graph")
    }
    if !matches!(status.as_str(), "running" | "interrupted") {
        bail!("Task is not abandoned or interrupted")
    }
    let mut reconciled_parallel_subtasks = Vec::new();
    let mut reconciled_parallel_indices = BTreeSet::new();
    if parallel_recorded && subtasks.iter().any(|node| node.state == "running") {
        let plan = graph
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Running parallel nodes have no saved graph"))?;
        let order = graph::order_from(db, task, plan)?;
        let reconciliation =
            parallel_history::reconcile_unpublished(db, task, plan, &order, &subtasks)?;
        subtasks = reconciliation.subtasks;
        reconciled_parallel_subtasks = reconciliation.reconciled;
        reconciled_parallel_indices = reconciliation.reconciled_indices;
    }
    if subtasks.iter().any(|n| {
        !matches!(
            n.state.as_str(),
            "not_recorded" | "pending" | "unverified" | "skipped"
        )
    }) {
        bail!("Continuation requires pending or recorded advisory nodes; running outcomes need reconciliation")
    }
    // Initial node writes precede all advisory execution. Missing state after
    // any execution transition is not an initialization crash and stays refused.
    if subtasks.iter().any(|n| n.state == "not_recorded")
        && subtasks
            .iter()
            .any(|n| !matches!(n.state.as_str(), "not_recorded" | "pending"))
    {
        bail!("Missing initial nodes after advisory execution advanced")
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
    let mut exploration = None;
    let mut explorer_seen = false;
    let mut bytes = 0usize;
    let mut contexts = 0usize;
    let mut policy_snapshots = 0usize;
    let mut stmt = db.prepare("SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END,id FROM events WHERE task_id=?1 AND kind IN ('execution_settings','execution_options','model_registration','module_snapshot','escalation_policy','reasoning_context','reasoning') ORDER BY id LIMIT 86")?;
    let mut rows = stmt.query([task])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        let kind: String = row.get(0)?;
        let source: String = row.get(1)?;
        let raw: String = row.get(2)?;
        bytes += raw.len();
        if count > 85 || bytes > 8_000_000 {
            bail!("Checkpoint exceeds bounded history")
        }
        let payload = crate::strict_json::value(raw.as_bytes(), 262144)?;
        if kind == "escalation_policy" {
            let expected = json!({"format_version":1,"policy":escalator.descriptor(),"permissions_granted":false,"implementation_attested":false});
            if source != "jev-plus" || payload != expected {
                bail!("Original escalation policy is unavailable or changed")
            }
            policy_snapshots += 1;
            continue;
        }
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
            if node.role == "explorer" {
                if explorer_seen || node.cycle != 0 {
                    bail!("Ambiguous exploration portfolio history")
                }
                explorer_seen = true;
                exploration = crate::reasoning::exploration::Portfolio::parse(
                    payload["text"].as_str().unwrap_or(""),
                )
                .ok();
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
            if kind == "reasoning_context" {
                if payload
                    .get("prompt_format")
                    .is_some_and(|format| format != crate::reasoning::PROMPT_FORMAT)
                {
                    bail!("Unsupported saved reasoning prompt format")
                }
                contexts += 1;
                if contexts > 17 || source != expected {
                    bail!("Invalid or excessive context history")
                }
                let event_id: u64 = row.get(3)?;
                let (calls,cycles):(u64,u64)=db.query_row("SELECT (SELECT count(*) FROM events WHERE task_id=?1 AND id<?2 AND kind='model_dispatch'),(SELECT count(*) FROM events WHERE task_id=?1 AND id<?2 AND kind='cycle_reservation')",rusqlite::params![task,event_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
                if payload["calls_before"] != calls || payload["cycles_before"] != cycles {
                    bail!("Context counters differ from chronological reservations")
                }
                if let Some(original) = snapshots.get(&kind) {
                    for field in [
                        "format_version",
                        "objective",
                        "context",
                        "objective_hash",
                        "context_hash",
                        "prompt_base_hash",
                        "untrusted_data",
                        "permissions_restorable",
                    ] {
                        if payload[field] != original[field] {
                            bail!("Continuation changed immutable reasoning context")
                        }
                    }
                    continue;
                }
            }
            if source != expected || snapshots.insert(kind, payload).is_some() {
                bail!("Invalid or repeated checkpoint snapshot")
            }
        }
    }
    if policy_snapshots == 0 && !escalator.is_builtin() {
        bail!("Legacy checkpoint has no original custom escalation policy")
    }
    let settings = snapshots
        .get("execution_settings")
        .ok_or_else(|| anyhow::anyhow!("Missing original settings"))?;
    if settings["runtime_role_prompts_sha256"] != crate::backend::runtime_role_prompts_sha256()? {
        bail!("Compiled role prompts changed or their original fingerprint is unavailable")
    }
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
    let empty_resource_route_snapshot = modules["modules"] == json!([])
        && modules["resource_routing_version"] == 1
        && modules["resource_routes"]["eligible"] == false
        && modules["resource_routes"]["routes"]
            .as_array()
            .is_some_and(Vec::is_empty);
    if modules["modules"] != expected && !empty_resource_route_snapshot {
        bail!("Unsupported pinned inference modules")
    }
    let validated_routes = resource_routes::validate(db, task, &cfg, settings, modules)?;
    let (resource_routes, resource_route_scope) = validated_routes
        .map(|(routes, scope)| (Some(routes), Some(scope)))
        .unwrap_or((None, None));
    let context = snapshots
        .get("reasoning_context")
        .ok_or_else(|| anyhow::anyhow!("Missing immutable context"))?;
    let text = context["context"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing context bytes"))?;
    if context["prompt_base_hash"] != sha256(Session::prompt_base(&query, text, true)?.as_bytes()) {
        bail!("Unsupported or altered saved reasoning prompt; recovery cannot substitute a different prompt format")
    }
    if context["format_version"] != 1
        || context["objective"] != query
        || text.len() > 96000
        || context["objective_hash"] != sha256(query.as_bytes())
        || context["context_hash"] != sha256(text.as_bytes())
        || context["untrusted_data"] != true
        || context["permissions_restorable"] != false
        || context["cycles_before"] != 0
    {
        bail!("Invalid original reasoning context")
    }
    let calls_before = context["calls_before"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing context charge anchor"))?;
    let recorded_before: u64 = db.query_row("SELECT count(*) FROM events WHERE task_id=?1 AND kind='model_dispatch' AND id < (SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_context')", [task], |r| r.get(0))?;
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
    let boundary_id = boundary_row.as_ref().map(|(id, _, _)| *id as i64);
    let boundary = boundary_row.map(|(id,source,raw)| -> Result<Value> {
        if source != "jev-r" { bail!("Invalid reasoning boundary source") }
        let v: Value = crate::strict_json::value(raw.as_bytes(), 262144)?;
        let later: u64 = db.query_row("SELECT count(*) FROM events WHERE task_id=?1 AND id>?2 AND kind IN ('model_dispatch','cycle_reservation','tool_reservation')",rusqlite::params![task,id],|r|r.get(0))?;
        let admission=&audit["remaining_inference_admission"];
        let cycles=settings["cycle_limit"].as_u64().unwrap()-admission["cycles"].as_u64().unwrap();
        let previous=v["previous_answer"].as_str().ok_or_else(||anyhow::anyhow!("Missing prior answer"))?;
        let strategy=v["strategy"].as_str().ok_or_else(||anyhow::anyhow!("Missing revision strategy"))?;
        if later != 0 || v["format_version"] != 1 || !matches!(v["phase"].as_str(),Some("ready_solver"|"ready_reflector"|"ready_critic"))
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
            || subtasks.iter().any(|n|!matches!(n.state.as_str(),"unverified"|"skipped"))
        { bail!("Reasoning boundary is stale, inconsistent or unsupported") }
        let history=crate::reasoning::convergence::History::from_nodes(&nodes,cycles as u32)?;
        if history.has_recurrence() {bail!("Recorded whole-goal candidates already recur; continuation cannot repeat a no-progress search")}
        if cycles > 0 && !escalator.is_builtin() {
            policy::validate_saved_decision(db, task, id, &v, escalator, &cfg, text)?;
        }
        if matches!(v["phase"].as_str(),Some("ready_reflector"|"ready_critic")) {
            super::revision::validate_revision_boundary(db,task,&v,&audit)?;
            if !matches!(mode.as_str(),"deep"|"research") || crate::verification::Criterion::from_query(&query)?.is_some() {bail!("Unsupported revision recovery objective or mode")}
            let base=Session::prompt_base(&query,text,true)?;
            let critic=v["phase"]=="ready_critic";
            let reason=if critic {"x".repeat(1000)} else {v["critique_reason"].as_str().unwrap().into()};
            let reflection=format!("{base}\nCandidate flaw (unverified advice): {reason}\nCandidate: {}",previous.chars().take(4000).collect::<String>());
            let solver=format!("{base}\nUnverified strategy: {}\nPrevious unverified answer: {}","x".repeat(16000),previous.chars().take(6000).collect::<String>());
            let critique=if critic {format!("{base}\nUnverified candidate: {}\nCritique your candidate. Return only JSON with revise (boolean) and reason (string). Never claim verification or permissions.",previous.chars().take(6000).collect::<String>())} else {String::new()};
            if admission["calls"].as_u64().unwrap_or(0)<if critic {3} else {2}
                || admission["reserved_output_tokens"].as_u64().unwrap_or(0)<if critic {640} else {512}
                || [reflection.len(),solver.len(),critique.len()].iter().any(|n|*n>32000)
                || reflection.len().saturating_add(solver.len()).saturating_add(critique.len())>cfg.max_input_bytes.saturating_sub(audit["charged_prompt_bytes"].as_u64().unwrap() as usize)
            {bail!("Insufficient original budget for critique, reflection and revised solver")}
        }
        Ok(v)
    }).transpose()?;
    let original_context_id: i64 = db.query_row(
        "SELECT min(id) FROM events WHERE task_id=?1 AND kind='reasoning_context'",
        [task],
        |row| row.get(0),
    )?;
    let finite_search_history =
        super::finite_search::validate(db, task, &query, &cfg, original_context_id, boundary_id)?;
    let path_progress = path_progress_history::validate_before(
        db,
        task,
        &query,
        &cfg,
        boundary.as_ref(),
        None,
        boundary_id,
    )?;
    let path_progress_enabled = path_progress.is_some();
    let progress = progress_history::validate_before(
        db,
        task,
        &query,
        &cfg,
        boundary.as_ref(),
        None,
        boundary_id,
    )?;
    let tail = audit["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["call"].as_u64().unwrap() > calls_before)
        .collect::<Vec<_>>();
    let mut parallel_schedule = None;
    if let Some(graph) = &graph {
        if tail.first().is_none_or(|c| c["role"] != "planner")
            || (boundary.is_none() && tail.iter().skip(1).any(|c| c["role"] != "solver"))
        {
            bail!("Checkpoint is not in the pre-solver graph phase")
        }
        let planner = planner_texts
            .get(&tail[0]["call"].as_u64().unwrap())
            .ok_or_else(|| anyhow::anyhow!("Missing recorded planner result"))?;
        if serde_json::to_value(crate::planning::Plan::parse(planner)?)?
            != serde_json::to_value(graph)?
        {
            bail!("Saved graph differs from charged planner result")
        }
        let order = super::graph::order_from(db, task, graph)?;
        parallel_schedule = parallel_history::validate(
            db,
            task,
            graph,
            &order,
            &subtasks,
            &reconciled_parallel_indices,
            parallel_history::HistoryAnchor {
                tail: &tail,
                calls_before,
                boundary_present: boundary.is_some(),
            },
        )?;
        if let Some(schedule) = &parallel_schedule {
            let completed = schedule
                .waves
                .iter()
                .take(schedule.completed_waves)
                .flatten()
                .copied()
                .collect::<Vec<_>>();
            if boundary.is_none() && tail.len() != completed.len() + 1 {
                bail!("Saved parallel graph and inference outcomes do not match")
            }
        } else {
            let parallel_configured = cfg.subtask_parallelism.is_some_and(|maximum| maximum > 1);
            let skipped_before_boundary = boundary
                .as_ref()
                .is_some_and(|saved| saved["phase"] == "ready_solver")
                && subtasks.iter().all(|node| node.state == "skipped");
            let pre_marker_initialization = boundary.is_none()
                && tail.len() == 1
                && subtasks
                    .iter()
                    .all(|node| matches!(node.state.as_str(), "not_recorded" | "pending"));
            if parallel_configured && !skipped_before_boundary && !pre_marker_initialization {
                bail!("Parallel graph has no committed full-wave schedule")
            }
            let completed = order
                .iter()
                .copied()
                .filter(|i| subtasks[*i].state == "unverified")
                .collect::<Vec<_>>();
            if (boundary.is_none() && tail.len() != completed.len() + 1)
                || tail.len() < completed.len() + 1
            {
                bail!("Saved graph and inference outcomes do not match")
            }
            if order
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
        }
    } else {
        if boundary.is_none() {
            bail!("No saved graph or unconsumed reasoning boundary")
        }
        if planner_texts
            .values()
            .any(|text| crate::planning::Plan::parse(text).is_ok())
        {
            bail!("A valid recorded planner result is missing its graph")
        }
    }
    if graph.is_some()
        && boundary.is_none()
        && cfg.subtask_parallelism.is_some_and(|maximum| maximum > 1)
    {
        validate_parallel_remaining_budget(
            &cfg,
            settings["call_limit"].as_u64().unwrap() as usize,
            &query,
            text,
            &subtasks,
            &audit,
        )?;
    }
    if boundary.is_none() && subtasks.iter().any(|n| n.state == "skipped") {
        bail!("Skipped graphs require a saved solver boundary")
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
    let resource_history = resource_history::validate(db, task, &cfg)?;
    Ok(Checkpoint {
        finite_search_history,
        path_progress_enabled,
        verifier_provider: verifier_provider.cloned(),
        typed_modules,
        resource_history,
        resource_routes,
        resource_route_scope,
        admission: admission.clone(),
        cfg,
        query,
        context: text.into(),
        mode,
        subtasks,
        boundary,
        parallel_schedule,
        reconciled_parallel_subtasks,
        state: RestoredState {
            resources,
            nodes,
            exploration,
            progress,
            path_progress: path_progress.unwrap_or_default(),
            deadline: observed + Duration::from_nanos(remaining),
        },
    })
}

fn validate_parallel_remaining_budget(
    cfg: &Config,
    call_limit: usize,
    query: &str,
    context: &str,
    subtasks: &[Subtask],
    audit: &Value,
) -> Result<()> {
    let base = Session::prompt_base(query, context, true)?;
    let (fits, _) = crate::reasoning::parallel::remaining_plan_admission(
        cfg,
        call_limit,
        audit["reserved_calls"].as_u64().unwrap() as usize,
        audit["reserved_output_tokens"].as_u64().unwrap() as usize,
        audit["charged_prompt_bytes"].as_u64().unwrap() as usize,
        subtasks,
        &base,
    );
    if !fits {
        bail!("Original remaining budget cannot admit every pending parallel wave and the final solver")
    }
    Ok(())
}

pub(super) fn validate_checkpoint(db: &rusqlite::Connection, task: &str) -> Result<()> {
    prepare(db, task).map(|_| ())
}

/// Construct a revision boundary only after a fresh failed independent check.
/// The caller's transaction owns both verification charges and this checkpoint.
pub(super) fn record_failed_revision(
    db: &rusqlite::Connection,
    task: &str,
    audit: &Value,
    saved: &Value,
    reason: &str,
) -> Result<bool> {
    use rusqlite::OptionalExtension;
    let admission = &audit["remaining_inference_admission"];
    let cycles: u64 = db.query_row(
        "SELECT count(*) FROM events WHERE task_id=?1 AND kind='cycle_reservation'",
        [task],
        |r| r.get(0),
    )?;
    if cycles == 0
        || saved["node"]["cycle"] != cycles
        || saved["call"] != audit["reserved_calls"]
        || admission["calls"].as_u64().unwrap_or(0) == 0
        || admission["cycles"].as_u64().unwrap_or(0) == 0
        || admission["tool_calls"].as_u64().unwrap_or(0) <= 1
        || admission["reserved_output_tokens"].as_u64().unwrap_or(0) < 384
    {
        return Ok(false);
    }
    let query: String = db.query_row(
        "SELECT CASE WHEN length(CAST(query AS BLOB))<=12000 THEN query END FROM tasks WHERE id=?1",
        [task],
        |row| row.get(0),
    )?;
    let _finite_search_history = super::finite_search::validate_saved_history(db, task, &query)?;
    let criterion = crate::verification::Criterion::from_query(&query)?;
    let settings_raw: Option<String> = db.query_row(
        "SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN json_extract(payload,'$.configuration') END FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 1",
        [task],
        |row| row.get(0),
    ).optional()?;
    let progress_active = if let (Some(criterion), Some(raw)) = (criterion, settings_raw) {
        let cfg: Config = serde_json::from_str(&raw)?;
        cfg.validate()?;
        cfg.progress_stopping.is_some() && criterion.name() == "finite_domain/v1"
    } else {
        false
    };
    let mut progress_state = None;
    let mut path_progress_state = None;
    if progress_active {
        let raw: String = db.query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 1",
            [task],
            |row| row.get(0),
        )?;
        let settings_payload: Value = crate::strict_json::value(raw.as_bytes(), 262144)?;
        let cfg: Config = serde_json::from_value(settings_payload["configuration"].clone())?;
        let settings = cfg.progress_stopping.as_ref().unwrap();
        let current_call = saved["call"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing recovered solver call"))?;
        let current_solver_id: i64 = db.query_row(
            "SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')=?2 AND json_extract(payload,'$.node.role')='solver' ORDER BY id DESC LIMIT 1",
            rusqlite::params![task, current_call],
            |row| row.get(0),
        )?;
        let answer = saved["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing recovered answer text"))?;
        if saved["node"]["content_hash"] != sha256(answer.as_bytes()) {
            bail!("Recovered solver answer hash is invalid")
        }
        let criterion = crate::verification::Criterion::from_query(&query)?.unwrap();
        if criterion.check(answer).is_ok() {
            bail!("Recovered result is not an independently rejected candidate")
        }
        let existing_current: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=?2)",
            rusqlite::params![task, cycles],
            |row| row.get(0),
        )?;
        let (next, assessment) = if existing_current {
            let history = progress_history::validate(db, task, &query, &cfg, None)?;
            let raw: String = db.query_row(
                "SELECT CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=?2",
                rusqlite::params![task, cycles],
                |row| row.get(0),
            )?;
            (history, crate::strict_json::value(raw.as_bytes(), 16384)?)
        } else {
            let history = progress_history::validate_until(
                db,
                task,
                &query,
                &cfg,
                None,
                Some(current_solver_id),
            )?;
            if history.observed_cycles().saturating_add(1) != cycles as u32 {
                bail!("Recovered finite-domain progress history is gapped or stale")
            }
            let mut next = history;
            let assessment = next.observe(&query, cycles as u32, answer, settings)?;
            db.execute(
                "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'reasoning_progress','verifier-r',?2,unixepoch())",
                rusqlite::params![task, assessment.to_string()],
            )?;
            (next, assessment)
        };
        progress_state = Some(serde_json::to_value(next)?);
        if assessment["stop"] == true {
            return Ok(false);
        }
    }
    let path_settings: Option<(String, String)> = db
        .query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN json_extract(payload,'$.configuration') END FROM events WHERE task_id=?1 AND kind='execution_settings' ORDER BY id LIMIT 1",
            [task],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((raw_settings, raw_configuration)) = path_settings {
        let settings_payload: Value = crate::strict_json::value(raw_settings.as_bytes(), 262144)?;
        let path_cfg: Config = serde_json::from_str(&raw_configuration)?;
        path_cfg.validate()?;
        if settings_payload["progress_policy"] == "shortest-path-progress/v1"
            && query.starts_with("path:")
        {
            let path_settings = path_cfg.progress_stopping.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Path progress marker lacks its original settings")
            })?;
            let path_criterion = crate::verification::Criterion::from_query(&query)?
                .ok_or_else(|| anyhow::anyhow!("Missing shortest-path criterion"))?;
            if path_criterion.name() != "shortest_path/v1" {
                bail!("Path progress marker differs from the original criterion")
            }
            let current_solver_id: i64 = db.query_row(
                "SELECT id FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')=?2 AND json_extract(payload,'$.node.role')='solver' ORDER BY id DESC LIMIT 1",
                rusqlite::params![task, saved["call"].as_u64()],
                |row| row.get(0),
            )?;
            let current_path_audit: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind IN ('reasoning_progress','reasoning_progress_provider_veto') AND json_extract(payload,'$.criterion')='shortest_path/v1' AND json_extract(payload,'$.cycle')=?2)",
                rusqlite::params![task, cycles],
                |row| row.get(0),
            )?;
            let history = if current_path_audit {
                let history = path_progress_history::validate_before(
                    db, task, &query, &path_cfg, None, None, None,
                )?
                .ok_or_else(|| anyhow::anyhow!("Path progress audit lacks its original marker"))?;
                let raw: String = db.query_row(
                    "SELECT CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('reasoning_progress','reasoning_progress_provider_veto') AND json_extract(payload,'$.criterion')='shortest_path/v1' AND json_extract(payload,'$.cycle')=?2 ORDER BY id DESC LIMIT 1",
                    rusqlite::params![task, cycles],
                    |row| row.get(0),
                )?;
                let assessment = crate::strict_json::value(raw.as_bytes(), 16384)?;
                if assessment["stop"] == true {
                    return Ok(false);
                }
                history
            } else {
                let mut history = path_progress_history::validate_before(
                    db,
                    task,
                    &query,
                    &path_cfg,
                    None,
                    Some(current_solver_id),
                    None,
                )?
                .ok_or_else(|| anyhow::anyhow!("Path progress is missing its original marker"))?;
                if history.observed_cycles().saturating_add(1) != cycles as u32 {
                    bail!("Recovered shortest-path progress history is gapped or stale")
                }
                let answer = saved["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing recovered shortest-path answer"))?;
                let assessment = history.observe(&query, cycles as u32, answer, path_settings)?;
                db.execute(
                    "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'reasoning_progress','verifier-r',?2,unixepoch())",
                    rusqlite::params![task, assessment.to_string()],
                )?;
                if assessment["stop"] == true {
                    return Ok(false);
                }
                history
            };
            path_progress_state = Some(serde_json::to_value(history)?);
        }
    }
    let previous:Option<String>=db.query_row("SELECT json_extract(payload,'$.node.content_hash') FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver' AND json_extract(payload,'$.node.cycle')>0 ORDER BY id DESC LIMIT 1 OFFSET 1",[task],|r|r.get(0)).optional()?;
    if previous
        .as_ref()
        .is_some_and(|hash| saved["node"]["content_hash"] == *hash)
    {
        return Ok(false);
    }
    let raw:Option<String>=db.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind='reasoning_context' ORDER BY id LIMIT 1",[task],|r|r.get(0)).optional()?;
    let Some(raw) = raw else { return Ok(false) };
    let context: Value = serde_json::from_str(&raw)?;
    let mode: String = db.query_row(
        "SELECT CASE WHEN length(mode)<=16 THEN mode END FROM tasks WHERE id=?1",
        [task],
        |r| r.get(0),
    )?;
    let tools: u64 = db.query_row(
        "SELECT count(*) FROM events WHERE task_id=?1 AND kind='tool_reservation'",
        [task],
        |r| r.get(0),
    )?;
    let mut boundary = json!({"format_version":1,"phase":"ready_solver","mode":mode,
        "objective_hash":context["objective_hash"],"context_hash":context["context_hash"],"prompt_base_hash":context["prompt_base_hash"],
        "strategy":format!("Independent checker rejected the candidate: {reason}"),"previous_answer":saved["text"],
        "last_answer_hash":saved["node"]["content_hash"],"calls_before":audit["reserved_calls"],"cycles_before":cycles,
        "tool_calls_before":tools,"reserved_output_tokens":audit["reserved_output_tokens"],"charged_prompt_bytes":audit["charged_prompt_bytes"],
        "permissions_restorable":false,"subtask_results_verified":false,"origin":"operator_failed_result_recovery"});
    if let Some(state) = progress_state {
        boundary["progress_state"] = state;
    }
    if let Some(state) = path_progress_state {
        boundary["path_progress_state"] = state;
    }
    let raw = serde_json::to_string(&boundary)?;
    if raw.len() > 262144 {
        bail!("Recovered revision checkpoint exceeds bounds")
    }
    db.execute_batch("SAVEPOINT recovered_revision")?;
    db.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'reasoning_boundary','jev-r',?2,unixepoch())",rusqlite::params![task,raw])?;
    if prepare(db, task).is_err() {
        db.execute_batch("ROLLBACK TO recovered_revision; RELEASE recovered_revision")?;
        return Ok(false);
    }
    db.execute_batch("RELEASE recovered_revision")?;
    Ok(true)
}

fn validate_worker_workspace(cfg: &mut Config, path: &std::path::Path) -> Result<()> {
    if cfg.inference_process {
        cfg.home = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Missing worker workspace"))?
            .into();
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::symlink_metadata(&cfg.home)?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
            bail!("Managed continuation requires a private existing workspace")
        }
        let mut expected = cfg.db().into_os_string();
        expected.push(".runtime.lock");
        if path.as_os_str() != expected {
            bail!("Managed continuation requires the canonical workspace database")
        }
    }
    Ok(())
}

/// Host-owned runtime objects retained from the original task. No deserializer:
/// database/model metadata cannot construct callbacks or registry authority.
#[derive(Clone, Default)]
pub struct RuntimeRestoration {
    pub escalation_policy: Option<Arc<dyn crate::escalation::EscalationPolicy>>,
    pub verifier: Option<Arc<dyn crate::verification::VerifierProvider>>,
    pub modules: Option<Arc<crate::fabric::modules::Broker>>,
}

#[derive(Default)]
struct ResumeBindings {
    modules: Option<Arc<crate::fabric::modules::Broker>>,
    native: Option<crate::residency::lifecycle::restoration::NativeRuntime>,
}

/// Continue a native task using its original operator policy and instance
/// receipts. No models are loaded or unloaded; unknown work is never replayed.
pub async fn resume_native_with_runtime(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    policy: &crate::residency::lifecycle::NativeLifecyclePolicy,
    runtime: RuntimeRestoration,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Native continuation requires explicit operator approval")
    }
    let escalator = runtime
        .escalation_policy
        .map(crate::escalation::Handle::new)
        .transpose()?
        .unwrap_or_default();
    let verifier = runtime
        .verifier
        .map(crate::verification::provider::Handle::new)
        .transpose()?;
    tokio::pin!(cancel);
    let native = tokio::select! {
        biased;
        _ = &mut cancel => bail!("Native continuation cancelled during setup"),
        result = crate::residency::lifecycle::restoration::prepare(store.clone(), task, policy, true) => result?,
    };
    let modules = if runtime.modules.is_none() && policy.typed_module_worker {
        Some(module_history::reconstruct_builtin_owned(
            store.clone(),
            task,
            human_approve,
            Some(native.ownership.clone()),
        )?)
    } else {
        runtime.modules
    };
    resume_with_runtime_handles(
        store,
        task,
        human_approve,
        escalator,
        verifier,
        ResumeBindings {
            modules,
            native: Some(native),
        },
        cancel,
    )
    .await
}

/// Restore the current compiled builtin worker under its recorded source
/// fingerprint. No worker is launched and no checkpoint is admitted here.
/// Custom callbacks and native lifecycle tasks require their original authority.
pub fn reconstruct_builtin_registry(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
) -> Result<Arc<crate::fabric::modules::Broker>> {
    module_history::reconstruct_builtin(store, task, human_approve)
}

/// Inspect using exact retained host objects. This never dispatches a module,
/// changes activation, recreates code, or grants permissions.
pub fn preview_with_runtime(
    store: &Store,
    task: &str,
    runtime: RuntimeRestoration,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    uuid::Uuid::parse_str(task)?;
    let escalator = runtime
        .escalation_policy
        .map(crate::escalation::Handle::new)
        .transpose()?
        .unwrap_or_default();
    let verifier = runtime
        .verifier
        .map(crate::verification::provider::Handle::new)
        .transpose()?;
    preview_with_runtime_handles(store, task, escalator, verifier, runtime.modules)
}

/// Resume under original budgets and exact retained module versions. Unknown
/// earlier dispatches remain refused; approval does not acknowledge their stop.
pub async fn resume_with_runtime(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    runtime: RuntimeRestoration,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Task continuation requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    let escalator = runtime
        .escalation_policy
        .map(crate::escalation::Handle::new)
        .transpose()?
        .unwrap_or_default();
    let verifier = runtime
        .verifier
        .map(crate::verification::provider::Handle::new)
        .transpose()?;
    resume_with_runtime_handles(
        store,
        task,
        human_approve,
        escalator,
        verifier,
        ResumeBindings {
            modules: runtime.modules,
            native: None,
        },
        cancel,
    )
    .await
}

/// Inspect current continuation admission without changing task state or launching workers.
/// Eligibility is sampled; an approved resume always validates again.
pub fn preview(store: &Store, task: &str) -> Result<Value> {
    preview_with_handle(store, task, crate::escalation::Handle::default(), None)
}

/// Inspect a checkpoint against an explicitly restored trusted-native policy.
/// The supplied descriptor must match every original snapshot; no decision callback runs.
pub fn preview_with_escalation_policy(
    store: &Store,
    task: &str,
    policy: Arc<dyn crate::escalation::EscalationPolicy>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    uuid::Uuid::parse_str(task)?;
    preview_with_handle(store, task, crate::escalation::Handle::new(policy)?, None)
}

/// Inspect continuation while restoring the exact task-pinned host verifier.
/// The callback is retained only as trusted host code; database metadata never
/// constructs or grants it.
pub fn preview_with_verifier_provider(
    store: &Store,
    task: &str,
    provider: Arc<dyn crate::verification::VerifierProvider>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    uuid::Uuid::parse_str(task)?;
    let provider = crate::verification::provider::Handle::new(provider)?;
    preview_with_handle(
        store,
        task,
        crate::escalation::Handle::default(),
        Some(provider),
    )
}

/// Restore both task-pinned native policy callbacks for checkpoint inspection.
pub fn preview_with_restored_policies(
    store: &Store,
    task: &str,
    escalator: Arc<dyn crate::escalation::EscalationPolicy>,
    provider: Arc<dyn crate::verification::VerifierProvider>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    uuid::Uuid::parse_str(task)?;
    preview_with_handle(
        store,
        task,
        crate::escalation::Handle::new(escalator)?,
        Some(crate::verification::provider::Handle::new(provider)?),
    )
}

fn preview_with_handle(
    store: &Store,
    task: &str,
    escalator: crate::escalation::Handle,
    verifier_provider: Option<crate::verification::provider::Handle>,
) -> Result<Value> {
    preview_with_runtime_handles(store, task, escalator, verifier_provider, None)
}

fn preview_with_runtime_handles(
    store: &Store,
    task: &str,
    escalator: crate::escalation::Handle,
    verifier_provider: Option<crate::verification::provider::Handle>,
    modules: Option<Arc<crate::fabric::modules::Broker>>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    uuid::Uuid::parse_str(task)?;
    if let Some(broker) = &modules {
        broker.require_store(store)?;
    }
    let path = store
        .runtime_lock
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Continuation requires a persistent store"))?;
    let _lease = ExecutionLease::acquire(path, true).map_err(|error| {
        error.context("Cannot acquire exclusive runtime lease for continuation preview")
    })?;
    let mut db = store.lock()?;
    let started = Instant::now();
    db.progress_handler(
        1000,
        Some(move || started.elapsed() > Duration::from_secs(5)),
    );
    let result = (|| -> Result<Checkpoint> {
        let tx = db.transaction()?;
        let mut checkpoint = prepare_with_runtime(
            &tx,
            task,
            &escalator,
            verifier_provider.as_ref(),
            modules.clone(),
        )?;
        validate_worker_workspace(&mut checkpoint.cfg, path)?;
        if started.elapsed() > Duration::from_secs(5) {
            bail!("Continuation admission deadline exhausted")
        }
        Ok(checkpoint)
    })();
    db.progress_handler(0, None::<fn() -> bool>);
    drop(db);
    let checkpoint = result?;
    let _resource_probe = checkpoint
        .cfg
        .resource_admission
        .as_ref()
        .map(|configuration| {
            crate::resource_scheduler::ResourcePool::probe_restoration(store, task, configuration)
        })
        .transpose()?;
    Ok(json!({"format_version":1,"task_id":task,
            "checkpoint_eligible":true,"resume_authorized":false,
            "human_approval_required":true,"resume_revalidates":true,
            "phase":checkpoint.boundary.as_ref().map(|b| b["phase"].clone()).unwrap_or(json!("pre_solver_graph")),
            "mode":checkpoint.mode,"model":checkpoint.cfg.model,
            "escalation_policy":escalator.descriptor(),"policy_implementation_attested":false,
            "verifier_provider":checkpoint.verifier_provider.as_ref().map(|provider|provider.descriptor()),
            "verifier_implementation_attested":false,
            "restored_typed_modules":checkpoint.typed_modules.as_ref().map(|modules|modules.evidence()),
            "managed_worker":checkpoint.cfg.inference_process,
            "resource_history":checkpoint.resource_history,
            "finite_search_history":checkpoint.finite_search_history,
            "parallel_history":checkpoint.parallel_schedule.as_ref().map(|schedule| json!({
                "marker_event_id":schedule.marker_id,"max_parallelism":schedule.max_parallelism,
                "completed_waves":schedule.completed_waves,"total_waves":schedule.waves.len(),
                "permissions_granted":false,"results_verified":false,"automatic_replay":false
            })),
            "remaining_admission":checkpoint.admission,
            "consumed":{"calls":checkpoint.state.resources.calls,
                "cycles":checkpoint.state.resources.cycles,
                "tool_calls":checkpoint.state.resources.tool_calls,
                "reserved_output_tokens":checkpoint.state.resources.reserved_output_tokens,
                "prompt_bytes":checkpoint.state.resources.prompt_bytes},
            "initial_nodes_to_record":checkpoint.subtasks.iter().filter(|n| n.state == "not_recorded").count(),
            "subtasks":checkpoint.subtasks.iter().map(|n|json!({"id":n.step.id,"state":n.state})).collect::<Vec<_>>(),
            "parallel_result_publications_to_reconcile":checkpoint.reconciled_parallel_subtasks.len(),
            "weights_attested":false}))
}

pub async fn resume(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    resume_with_handle(
        store,
        task,
        human_approve,
        crate::escalation::Handle::default(),
        None,
        cancel,
    )
    .await
}

/// Resume using an operator-restored native implementation of the pinned policy.
/// This never loads code or constructs authority from stored policy metadata.
pub async fn resume_with_escalation_policy(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    policy: Arc<dyn crate::escalation::EscalationPolicy>,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Task continuation requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    let escalator = crate::escalation::Handle::new(policy)?;
    resume_with_handle(store, task, human_approve, escalator, None, cancel).await
}

/// Resume with the exact operator-restored provider pinned to this task. The
/// restored callback is compared before admission and is used for future checks;
/// an intent without a result remains unresolved and is never retried.
pub async fn resume_with_verifier_provider(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    provider: Arc<dyn crate::verification::VerifierProvider>,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Task continuation requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    let provider = crate::verification::provider::Handle::new(provider)?;
    resume_with_handle(
        store,
        task,
        human_approve,
        crate::escalation::Handle::default(),
        Some(provider),
        cancel,
    )
    .await
}

/// Resume with both original host-installed native callbacks. Neither identity
/// is loaded from task metadata; each supplied implementation is compared to its
/// persisted descriptor before the task is admitted.
pub async fn resume_with_restored_policies(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    escalator: Arc<dyn crate::escalation::EscalationPolicy>,
    provider: Arc<dyn crate::verification::VerifierProvider>,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Task continuation requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    resume_with_handle(
        store,
        task,
        human_approve,
        crate::escalation::Handle::new(escalator)?,
        Some(crate::verification::provider::Handle::new(provider)?),
        cancel,
    )
    .await
}

async fn resume_with_handle(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    escalator: crate::escalation::Handle,
    verifier_provider: Option<crate::verification::provider::Handle>,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    resume_with_runtime_handles(
        store,
        task,
        human_approve,
        escalator,
        verifier_provider,
        ResumeBindings::default(),
        cancel,
    )
    .await
}

async fn resume_with_runtime_handles(
    store: Arc<Store>,
    task: &str,
    human_approve: bool,
    escalator: crate::escalation::Handle,
    verifier_provider: Option<crate::verification::provider::Handle>,
    bindings: ResumeBindings,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<Value> {
    let ResumeBindings { modules, native } = bindings;
    security::authorize("human-cli", "task.resume")?;
    if !human_approve {
        bail!("Task continuation requires explicit operator approval")
    }
    uuid::Uuid::parse_str(task)?;
    if let Some(broker) = &modules {
        broker.require_store(store.as_ref())?;
    }
    let path = store
        .runtime_lock
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Continuation requires a persistent store"))?;
    let mut lease = if let Some(runtime) = &native {
        runtime.ownership.validate(&store)?;
        None
    } else {
        Some(ExecutionLease::acquire(path, true).map_err(|error| {
            error.context("Cannot acquire exclusive runtime lease for task continuation")
        })?)
    };
    let admission_started = Instant::now();
    let original_configuration = {
        let mut db = store.lock()?;
        db.progress_handler(
            1000,
            Some(move || admission_started.elapsed() > Duration::from_secs(5)),
        );
        let prepared = (|| -> Result<Config> {
            let tx = db.transaction()?;
            Ok(prepare_with_native(
                &tx,
                task,
                &escalator,
                verifier_provider.as_ref(),
                modules.clone(),
                native.as_ref(),
            )?
            .cfg)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        prepared?
    };
    let resource_pool = original_configuration
        .resource_admission
        .as_ref()
        .map(|configuration| {
            crate::resource_scheduler::ResourcePool::configured_owned(
                store.clone(),
                configuration,
                native.as_ref().map(|runtime| runtime.ownership.clone()),
            )
        })
        .transpose()?;
    if let Some(pool) = &resource_pool {
        let snapshot = pool.snapshot()?;
        if snapshot.closed
            || snapshot
                .reservations
                .iter()
                .any(|entry| entry.task_id == task)
        {
            bail!("Original task resource pool remains closed or unresolved")
        }
    }
    if admission_started.elapsed() > Duration::from_secs(5) {
        bail!("Continuation admission deadline exhausted")
    }
    let restored_pool = resource_pool.as_ref().map(|pool| pool.snapshot().map(|s|
        json!({"pool_id":s.pool_id,"resident_catalog_sha256":s.resident_catalog_sha256,"resident_allocated":s.resident_allocated,"original_configuration_preserved":true,"physical_limits_enforced":false}))).transpose()?;
    let checkpoint = {
        let mut db = store.lock()?;
        let started = admission_started;
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(5)),
        );
        let prepared = (|| -> Result<Checkpoint> {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let mut checkpoint = prepare_with_native(
                &tx,
                task,
                &escalator,
                verifier_provider.as_ref(),
                modules.clone(),
                native.as_ref(),
            )?;
            if serde_json::to_value(&checkpoint.cfg)?
                != serde_json::to_value(&original_configuration)?
            {
                bail!("Original continuation configuration changed during resource admission")
            }
            if checkpoint.cfg.inference_process {
                validate_worker_workspace(&mut checkpoint.cfg, path)?;
            }
            if native.is_none()
                && (checkpoint.cfg.inference_process || checkpoint.typed_modules.is_some())
            {
                // No inference or module dispatch has begun. A competing exclusive owner during conversion
                // cannot commit recovery while this write transaction remains open.
                lease = Some(
                    lease
                        .take()
                        .expect("Admission owns its lease")
                        .into_shared()?,
                );
            }
            // Persist only missing initial transitions under the admission lease
            // and transaction, before workers or inference can be constructed.
            for node in &mut checkpoint.subtasks {
                if node.state == "not_recorded" {
                    node.state = "pending".into();
                    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'subtask','jev-plus',?2,unixepoch())",rusqlite::params![task,json!({"node":node,"origin":"operator_initial_graph_recovery"}).to_string()])?;
                }
            }
            for node in &checkpoint.reconciled_parallel_subtasks {
                if node.state != "unverified" || node.result.is_none() {
                    bail!("Parallel reconciliation produced an invalid advisory state")
                }
                tx.execute(
                    "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'subtask','jev-plus',?2,unixepoch())",
                    rusqlite::params![task,json!({"node":node,"origin":"operator_parallel_result_reconciliation"}).to_string()],
                )?;
            }
            let parallel_history = checkpoint.parallel_schedule.as_ref().map(|schedule| {
                json!({"marker_event_id":schedule.marker_id,"max_parallelism":schedule.max_parallelism,
                    "completed_waves":schedule.completed_waves,"total_waves":schedule.waves.len(),
                    "permissions_granted":false,"results_verified":false,"automatic_replay":false})
            });
            tx.execute(
                "UPDATE tasks SET status='running',updated=unixepoch() WHERE id=?1",
                [task],
            )?;
            tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'continuation','human-cli',?2,unixepoch())",rusqlite::params![task,json!({"state":"started","format_version":1,"original_budget_preserved":true,"prior_timing_complete":false,"subtask_results_verified":false,"reconciled_parallel_result_publications":checkpoint.reconciled_parallel_subtasks.len(),"escalation_policy":escalator.descriptor(),"verifier_provider":checkpoint.verifier_provider.as_ref().map(|provider|provider.descriptor()),"restored_native_runtime":native.as_ref().map(|runtime|runtime.evidence()),"restored_typed_modules":checkpoint.typed_modules.as_ref().map(|modules|modules.evidence()),"restored_resource_pool":restored_pool,"resource_history":checkpoint.resource_history,"finite_search_history":checkpoint.finite_search_history,"parallel_history":parallel_history}).to_string()])?;
            if started.elapsed() > Duration::from_secs(5) {
                bail!("Continuation admission deadline exhausted")
            }
            tx.commit()?;
            Ok(checkpoint)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        prepared?
    };
    let parallel_history = checkpoint.parallel_schedule.as_ref().map(|schedule| {
        json!({"marker_event_id":schedule.marker_id,"max_parallelism":schedule.max_parallelism,
            "completed_waves":schedule.completed_waves,"total_waves":schedule.waves.len(),
            "permissions_granted":false,"results_verified":false,"automatic_replay":false})
    });
    let prior_calls = checkpoint.state.resources.calls;
    let deadline = checkpoint.state.deadline;
    let path_progress_enabled = checkpoint.path_progress_enabled;
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
        None => match &native {
            Some(runtime) => runtime.backend()?,
            None => Arc::new(LocalBackend(checkpoint.cfg.clone())),
        },
    };
    let inference_concurrency = checkpoint.cfg.subtask_parallelism.unwrap_or(1);
    let backend: Arc<dyn InferenceBackend> = Arc::new(
        crate::fabric::Broker::with_inference_concurrency(inference, inference_concurrency),
    );
    let backend = resource_routes::restrict_backend(backend, checkpoint.resource_routes.clone())?;
    let mut session = Session::restored(
        checkpoint.cfg,
        store.clone(),
        backend,
        task.into(),
        &checkpoint.mode,
        checkpoint.state,
    )?;
    session.typed_modules = checkpoint
        .typed_modules
        .as_ref()
        .map(|modules| (modules.broker.clone(), modules.pins.clone()));
    session.set_resource_route_scope(checkpoint.resource_route_scope.clone())?;
    session.set_path_progress_enabled(path_progress_enabled);
    session.escalator = escalator;
    session.resource_pool = resource_pool;
    if let Some(provider) = checkpoint.verifier_provider.clone() {
        session.restore_verifier_provider(provider);
    }
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
                    checkpoint
                        .parallel_schedule
                        .as_ref()
                        .map(|schedule| schedule.waves.clone()),
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
        Ok(answer) if session.verification_attempted => (answer, "rejected"),
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
        "escalation_policy":session.escalator.descriptor(),"policy_implementation_attested":false,
        "verified":session.verified,"criterion":session.verified_criterion,
        "verification_attempted":session.verification_attempted,"stop_reason":session.stop_reason,
        "restored_resource_pool":restored_pool,"resource_history":checkpoint.resource_history,
        "parallel_history":parallel_history,
        "restored_native_runtime":native.as_ref().map(|runtime|runtime.evidence()),
        "restored_typed_modules":checkpoint.typed_modules.as_ref().map(|modules|modules.evidence()),
        "finite_search_history":checkpoint.finite_search_history,
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
#[path = "continuation_policy_tests.rs"]
mod policy_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Generation;
    use crate::scheduling::SubtaskScheduler;
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
        fixture_in_order(store, false).await
    }
    async fn fixture_in_order(store: Arc<Store>, custom_order: bool) -> (Arc<Store>, String) {
        fixture_in_with_policy(store, custom_order, None).await
    }
    pub(super) async fn fixture_in_with_policy(
        store: Arc<Store>,
        custom_order: bool,
        policy: Option<crate::escalation::Descriptor>,
    ) -> (Arc<Store>, String) {
        const INDEPENDENT_PLAN: &str = r#"{"steps":[{"id":"a","objective":"Inspect input","depends_on":[]},{"id":"b","objective":"Independent advice","depends_on":[]}]}"#;
        let plan_text = if custom_order { INDEPENDENT_PLAN } else { PLAN };
        let first = if custom_order { 1 } else { 0 };
        let task = store.create_task(QUERY, "deep").unwrap();
        let cfg = Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            max_tokens: 3000,
            ..Config::default()
        };
        let backend = Arc::new(Script(Mutex::new(
            vec![("planner", plan_text), ("solver", "original advice")].into(),
        )));
        let mut session = Session::new(cfg, store.clone(), backend, task.clone(), "deep").unwrap();
        if let Some(policy) = policy {
            store.event(&task, "escalation_policy", "jev-plus", json!({"format_version":1,"policy":policy,"permissions_granted":false,"implementation_attested":false})).unwrap();
        }
        store.event(&task,"execution_options","runtime",json!({"format_version":1,"profile":"full","explain":false,"conversation_ref":null})).unwrap();
        store.event(&task,"module_snapshot","fabric",json!({"modules":[{"id":"inference.local","version":"1.0.0","roles":crate::fabric::protocol::ROLES}]})).unwrap();
        store.event(&task,"reasoning_context","jev-r",json!({"format_version":1,"objective":QUERY,"context":"","objective_hash":sha256(QUERY.as_bytes()),"context_hash":sha256(b""),"prompt_base_hash":sha256(Session::prompt_base(QUERY,"",true).unwrap().as_bytes()),"calls_before":0,"cycles_before":0,"untrusted_data":true,"permissions_restorable":false})).unwrap();
        session
            .call("planner", "planning", 256)
            .await
            .unwrap()
            .unwrap();
        let plan = crate::planning::Plan::parse(plan_text).unwrap();
        store
            .event(
                &task,
                "plan",
                "planner-r",
                json!({"graph":plan,"trusted_permissions":false,"scheduler_required":custom_order}),
            )
            .unwrap();
        if custom_order {
            let descriptor = json!({"id":"fixture.reverse","version":"1.0.0"});
            store.event(&task,"subtask_schedule_intent","jev-plus",json!({"scheduler":descriptor,"permissions_granted":false,"implementation_attested":false})).unwrap();
            store.event(&task,"subtask_schedule","jev-plus",json!({"format_version":1,"scheduler":descriptor,"graph_sha256":sha256(&serde_json::to_vec(&plan).unwrap()),"order":[1,0],"permissions_granted":false})).unwrap();
        }
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
        nodes[first].state = "running".into();
        store
            .event(&task, "subtask", "jev-plus", json!({"node":nodes[first]}))
            .unwrap();
        session
            .call("solver", "advisory", 192)
            .await
            .unwrap()
            .unwrap();
        nodes[first].state = "unverified".into();
        nodes[first].result = Some("original advice".into());
        store
            .event(&task, "subtask", "jev-plus", json!({"node":nodes[first]}))
            .unwrap();
        store.finish_task(&task, "interrupted", "").unwrap();
        (store, task)
    }

    async fn unpublished_parallel_result_fixture(store: Arc<Store>) -> (Arc<Store>, String) {
        const PARALLEL_PLAN: &str = r#"{"steps":[{"id":"a","objective":"Inspect first input","depends_on":[]},{"id":"b","objective":"Inspect second input","depends_on":[]}]}"#;
        let task = store.create_task(QUERY, "deep").unwrap();
        let cfg = Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            max_tokens: 3000,
            subtask_parallelism: Some(2),
            ..Config::default()
        };
        let plan = crate::planning::Plan::parse(PARALLEL_PLAN).unwrap();
        let backend = Arc::new(Script(Mutex::new(
            vec![
                ("planner", PARALLEL_PLAN),
                ("solver", "first parallel advice"),
                ("solver", "second parallel advice"),
            ]
            .into(),
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
        store
            .event(
                &task,
                "plan",
                "planner-r",
                json!({"graph":plan,"trusted_permissions":false,"scheduler_required":true}),
            )
            .unwrap();
        let scheduler = crate::scheduling::StableScheduler.descriptor();
        let order = crate::scheduling::StableScheduler.order(&plan).unwrap();
        store.event(&task,"subtask_schedule_intent","jev-plus",json!({"scheduler":scheduler,"permissions_granted":false,"implementation_attested":false})).unwrap();
        store.event(&task,"subtask_schedule","jev-plus",json!({"format_version":1,"scheduler":scheduler,"graph_sha256":sha256(&serde_json::to_vec(&plan).unwrap()),"order":order,"permissions_granted":false})).unwrap();
        let nodes = plan
            .steps
            .iter()
            .cloned()
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
        store.lock().unwrap().execute_batch("CREATE TRIGGER reject_second_parallel_publication BEFORE INSERT ON events WHEN NEW.kind='subtask' AND json_extract(NEW.payload,'$.node.state')='unverified' AND json_extract(NEW.payload,'$.node.step.id')='b' BEGIN SELECT RAISE(ABORT,'fixture publication failure'); END;").unwrap();
        assert!(session
            .continue_graph(nodes, QUERY, "", "deep", None)
            .await
            .is_err());
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
                [&task],
            )
            .unwrap();
        drop(session);
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_second_parallel_publication")
            .unwrap();
        (store, task)
    }

    async fn advisory_crash_fixture(
        custom_order: bool,
    ) -> (Arc<Store>, String, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("fabric-advisory-recovery-{}", uuid::Uuid::new_v4()));
        let (store, task) = fixture_in_order(
            Arc::new(Store::open(&root.join("state.db")).unwrap()),
            custom_order,
        )
        .await;
        store.lock().unwrap().execute("DELETE FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.state')='unverified'",[&task]).unwrap();
        (store, task, root)
    }
    fn truncate_initial_graph(store: &Store, task: &str, pending: usize) {
        let db = store.lock().unwrap();
        let cutoff: u64 = db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='subtask' ORDER BY id LIMIT 1 OFFSET ?2",rusqlite::params![task,pending],|r|r.get(0)).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>=?2",
            rusqlite::params![task, cutoff],
        )
        .unwrap();
    }

    #[tokio::test]
    async fn initial_graph_preview_is_read_only_and_rejects_missing_state_after_execution() {
        for pending in 0..2 {
            let root =
                std::env::temp_dir().join(format!("fabric-initial-{}", uuid::Uuid::new_v4()));
            let (store, task) =
                fixture_in(Arc::new(Store::open(&root.join("state.db")).unwrap())).await;
            truncate_initial_graph(&store, &task, pending);
            let before = store.task(&task).unwrap();
            let report = preview(&store, &task).unwrap();
            assert_eq!(report["initial_nodes_to_record"], 2 - pending);
            assert_eq!(report["consumed"]["calls"], 1);
            assert_eq!(store.task(&task).unwrap(), before);
            assert!(resume(store.clone(), &task, false, std::future::pending())
                .await
                .is_err());
            assert_eq!(store.task(&task).unwrap(), before);
            store.lock().unwrap().execute_batch("CREATE TRIGGER reject_initial BEFORE INSERT ON events WHEN NEW.kind='subtask' AND json_extract(NEW.payload,'$.node.step.id')='b' BEGIN SELECT RAISE(ABORT,'fixture initial write failure'); END;").unwrap();
            assert!(resume(store.clone(), &task, true, std::future::pending())
                .await
                .is_err());
            assert_eq!(store.task(&task).unwrap(), before);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
        let (store, task) = fixture().await;
        store.lock().unwrap().execute("DELETE FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='b'",[&task]).unwrap();
        assert!(prepare(&store.lock().unwrap(), &task).is_err());
    }

    #[tokio::test]
    async fn initial_graph_resume_records_missing_nodes_before_http_without_replanning() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        for pending in 0..2 {
            let root =
                std::env::temp_dir().join(format!("fabric-initial-http-{}", uuid::Uuid::new_v4()));
            let (store, task) =
                fixture_in_order(Arc::new(Store::open(&root.join("state.db")).unwrap()), true)
                    .await;
            truncate_initial_graph(&store, &task, pending);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
            store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",rusqlite::params![task,endpoint]).unwrap();
            let observed_store = store.clone();
            let observed_task = task.clone();
            let server = tokio::spawn(async move {
                for (index, answer) in [
                    "first recovered advice",
                    "second recovered advice",
                    "[1,2,3]",
                ]
                .into_iter()
                .enumerate()
                {
                    let (socket, _) = listener.accept().await.unwrap();
                    let mut socket = BufReader::new(socket);
                    let mut length = None;
                    let mut total = 0;
                    loop {
                        let mut line = String::new();
                        assert!(socket.read_line(&mut line).await.unwrap() > 0);
                        total += line.len();
                        assert!(total < 8192);
                        if line == "\r\n" {
                            break;
                        }
                        if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length: ")
                        {
                            length = Some(n.trim().parse::<usize>().unwrap());
                        }
                    }
                    let length = length.unwrap();
                    assert!(length < 131072);
                    let mut body = vec![0; length];
                    socket.read_exact(&mut body).await.unwrap();
                    let body: Value = serde_json::from_slice(&body).unwrap();
                    let prompt = body["messages"].to_string();
                    if index == 0 {
                        assert!(prompt.contains("Independent advice"));
                    }
                    if index == 1 {
                        assert!(prompt.contains("Inspect input"));
                    }
                    if index == 2 {
                        assert!(prompt.contains("first recovered advice"));
                        assert!(prompt.contains("second recovered advice"));
                    }
                    let (_, _, nodes) =
                        graph::graph_from(&observed_store.lock().unwrap(), &observed_task).unwrap();
                    assert!(nodes.iter().all(|n| n.state != "not_recorded"));
                    let response = json!({"choices":[{"message":{"content":answer}}]}).to_string();
                    socket.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).as_bytes()).await.unwrap();
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
            assert_eq!(report["new_calls"], 3);
            server.await.unwrap();
            let audit = store.inference_audit(&task).unwrap();
            assert_eq!(audit["reserved_calls"], 4);
            assert_eq!(audit["reserved_output_tokens"], 1024);
            assert_eq!(
                audit["calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|c| c["role"] == "planner")
                    .count(),
                1
            );
            let trace = store.task(&task).unwrap();
            assert_eq!(
                trace["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["kind"] == "subtask"
                        && e["payload"]["origin"] == "operator_initial_graph_recovery")
                    .count(),
                2 - pending
            );
            assert!(resume(store.clone(), &task, true, std::future::pending())
                .await
                .is_err());
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn reconciles_recorded_advice_without_replay_and_preserves_custom_schedule() {
        for custom in [false, true] {
            let (store, task, root) = advisory_crash_fixture(custom).await;
            assert!(preview(&store, &task).is_err());
            let mut before = store.inference_audit(&task).unwrap();
            assert!(store.recover_advisory_result(&task, false).is_err());
            let lease = store.execution_lease().unwrap();
            assert!(store.recover_advisory_result(&task, true).is_err());
            drop(lease);
            let report = store.recover_advisory_result(&task, true).unwrap();
            assert_eq!(report["subtask_id"], if custom { "b" } else { "a" });
            assert_eq!(report["call"], 2);
            assert_eq!(report["result_verified"], false);
            assert_eq!(report["inference_replayed"], false);
            assert!(!report.to_string().contains("original advice"));
            let mut after = store.inference_audit(&task).unwrap();
            assert!(
                after["remaining_inference_admission"]["wall_time"]["remaining_ns"]
                    .as_u64()
                    .unwrap()
                    <= before["remaining_inference_admission"]["wall_time"]["remaining_ns"]
                        .as_u64()
                        .unwrap()
            );
            before["remaining_inference_admission"]
                .as_object_mut()
                .unwrap()
                .remove("wall_time");
            after["remaining_inference_admission"]
                .as_object_mut()
                .unwrap()
                .remove("wall_time");
            assert_eq!(after, before);
            let restored = prepare(&store.lock().unwrap(), &task).unwrap();
            assert_eq!(restored.state.resources.calls, 2);
            assert_eq!(restored.state.resources.reserved_output_tokens, 448);
            assert_eq!(
                restored
                    .subtasks
                    .iter()
                    .filter(|s| s.state == "unverified")
                    .count(),
                1
            );
            assert_eq!(preview(&store, &task).unwrap()["checkpoint_eligible"], true);
            assert!(store.recover_advisory_result(&task, true).is_err());
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn invalid_or_unresolved_advice_never_commits_a_graph_transition() {
        for alteration in [
            "DELETE FROM events WHERE kind='reasoning' AND json_extract(payload,'$.call')=2",
            "UPDATE events SET payload=json_set(payload,'$.text','tampered') WHERE kind='reasoning' AND json_extract(payload,'$.call')=2",
            "UPDATE events SET payload=json_set(payload,'$.execution_clock',NULL) WHERE kind='execution_settings'",
            "DELETE FROM events WHERE kind='subtask_schedule'",
            "UPDATE events SET id=(SELECT max(id)+1 FROM events) WHERE kind='subtask' AND json_extract(payload,'$.node.state')='running'",
            "UPDATE events SET payload=json_set(payload,'$.node.role','critic') WHERE kind='reasoning' AND json_extract(payload,'$.call')=2; UPDATE events SET payload=json_set(payload,'$.role','critic') WHERE kind='model_dispatch' AND json_extract(payload,'$.call')=2",
            "UPDATE events SET payload=json_set(payload,'$.execution_clock.started_ns',0,'$.execution_clock.deadline_ns',json_extract(payload,'$.configuration.max_wall_seconds')*1000000000) WHERE kind='execution_settings'",
        ] {
            let (store,task,root)=advisory_crash_fixture(true).await;
            store.lock().unwrap().execute_batch(alteration).unwrap();
            assert!(store.recover_advisory_result(&task,true).is_err(),"{alteration}");
            let count:u64=store.lock().unwrap().query_row("SELECT count(*) FROM events WHERE kind='advisory_recovery' OR (kind='subtask' AND json_extract(payload,'$.node.state')='unverified')",[],|r|r.get(0)).unwrap();
            assert_eq!(count,0,"{alteration}");
            drop(store); std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn failed_reconciliation_audit_rolls_back_the_linked_result() {
        let (store, task, root) = advisory_crash_fixture(false).await;
        store.lock().unwrap().execute_batch("CREATE TRIGGER reject_advisory_recovery BEFORE INSERT ON events WHEN NEW.kind='advisory_recovery' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        assert!(store.recover_advisory_result(&task, true).is_err());
        assert_eq!(
            graph::graph_from(&store.lock().unwrap(), &task).unwrap().2[0].state,
            "running"
        );
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_advisory_recovery")
            .unwrap();
        assert!(store.recover_advisory_result(&task, true).is_ok());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn preview_preserves_task_and_events_and_rejects_live_or_invalid_checkpoints() {
        let root = std::env::temp_dir().join(format!("fabric-preview-{}", uuid::Uuid::new_v4()));
        let (store, task) =
            fixture_in(Arc::new(Store::open(&root.join("state.db")).unwrap())).await;
        let snapshot = || {
            let db = store.lock().unwrap();
            let count: u64 = db
                .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
                .unwrap();
            let status: String = db
                .query_row("SELECT status FROM tasks WHERE id=?1", [&task], |r| {
                    r.get(0)
                })
                .unwrap();
            (count, status)
        };
        let before = snapshot();
        let result = preview(&store, &task).unwrap();
        assert_eq!(result["phase"], "pre_solver_graph");
        assert_eq!(result["resume_authorized"], false);
        assert_eq!(result["consumed"]["calls"], 2);
        assert_eq!(result["consumed"]["reserved_output_tokens"], 448);
        assert_eq!(
            result["remaining_admission"]["reserved_output_tokens"],
            2552
        );
        assert!(!result.to_string().contains("original advice"));
        assert!(!result.to_string().contains(QUERY));
        assert_eq!(snapshot(), before);
        let lease = ExecutionLease::acquire(store.runtime_lock.as_deref().unwrap(), false).unwrap();
        assert!(preview(&store, &task).is_err());
        drop(lease);
        assert!(preview(&store, &task).is_ok());
        store.lock().unwrap().execute("UPDATE events SET payload=json_remove(payload,'$.runtime_role_prompts_sha256') WHERE kind='execution_settings'", []).unwrap();
        assert!(preview(&store, &task).is_err());
        assert_eq!(snapshot(), before);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
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
            .continue_graph(
                saved.subtasks,
                &saved.query,
                &saved.context,
                &saved.mode,
                None,
            )
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
    async fn checkpoint_rejects_unavailable_changed_or_spoofed_escalation_policies() {
        use crate::escalation::EscalationPolicy;
        for variant in 0..3 {
            let (store, task) = fixture().await;
            let mut payload = json!({"format_version":1,"policy":crate::escalation::BudgetPolicy.descriptor(),"permissions_granted":false,"implementation_attested":false});
            store
                .event(&task, "escalation_policy", "jev-plus", payload.clone())
                .unwrap();
            assert!(prepare(&store.lock().unwrap(), &task).is_ok());
            if variant == 0 {
                payload["policy"]["id"] = json!("fixture.custom");
            }
            if variant == 1 {
                payload["policy"]["version"] = json!("2.0.0");
            }
            store
                .event(
                    &task,
                    "escalation_policy",
                    if variant == 2 { "foreign" } else { "jev-plus" },
                    payload,
                )
                .unwrap();
            let before = store.task(&task).unwrap();
            let error = prepare(&store.lock().unwrap(), &task).err().unwrap();
            assert!(error.to_string().contains("escalation policy"), "{error}");
            assert_eq!(store.task(&task).unwrap(), before);
        }
    }

    #[tokio::test]
    async fn compatible_prompt_metadata_preserves_original_context_and_charges() {
        let (store, task) = fixture().await;
        let original = prepare(&store.lock().unwrap(), &task).unwrap();
        store.lock().unwrap().execute(
            "UPDATE events SET payload=json_set(payload,'$.prompt_format',?2) WHERE task_id=?1 AND kind='reasoning_context'",
            rusqlite::params![task,crate::reasoning::PROMPT_FORMAT],
        ).unwrap();
        let declared = prepare(&store.lock().unwrap(), &task).unwrap();
        assert_eq!(declared.query, original.query);
        assert_eq!(declared.context, original.context);
        assert_eq!(
            declared.state.resources.calls,
            original.state.resources.calls
        );
        assert_eq!(
            declared.state.resources.reserved_output_tokens,
            original.state.resources.reserved_output_tokens
        );
    }

    #[tokio::test]
    async fn continuation_requires_matching_compiled_role_prompts() {
        for variant in 0..3 {
            let (store, task) = fixture().await;
            assert!(prepare(&store.lock().unwrap(), &task).is_ok());
            let before: u64 = store
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM events WHERE task_id=?1",
                    [&task],
                    |row| row.get(0),
                )
                .unwrap();
            let sql=match variant {
                0 => "UPDATE events SET payload=json_remove(payload,'$.runtime_role_prompts_sha256') WHERE task_id=?1 AND kind='execution_settings'",
                1 => "UPDATE events SET payload=json_set(payload,'$.runtime_role_prompts_sha256',NULL) WHERE task_id=?1 AND kind='execution_settings'",
                _ => "UPDATE events SET payload=json_set(payload,'$.runtime_role_prompts_sha256','0000000000000000000000000000000000000000000000000000000000000000') WHERE task_id=?1 AND kind='execution_settings'",
            };
            store.lock().unwrap().execute(sql, [&task]).unwrap();
            let error = prepare(&store.lock().unwrap(), &task)
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains("role prompts"), "{error}");
            let after: u64 = store
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM events WHERE task_id=?1",
                    [&task],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(before, after);
        }
    }

    #[tokio::test]
    async fn rejects_changed_graph_context_and_unresolved_dispatch() {
        for variant in 0..7 {
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
                3 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.context','Changed context') WHERE task_id=?1 AND kind='reasoning_context'",[&task]).unwrap();
                }
                4 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.prompt_format','unknown-future-format') WHERE task_id=?1 AND kind='reasoning_context'",[&task]).unwrap();
                }
                5 => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.prompt_format',NULL) WHERE task_id=?1 AND kind='reasoning_context'",[&task]).unwrap();
                }
                _ => {
                    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.prompt_base_hash',?2) WHERE task_id=?1 AND kind='reasoning_context'",rusqlite::params![task,crate::store::sha256(b"unsupported old prompt")]).unwrap();
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
        http_continuation(false, false, false, false, false).await;
    }
    #[tokio::test]
    async fn registered_resume_uses_the_original_http_model() {
        http_continuation(true, false, false, false, false).await;
    }
    #[tokio::test]
    async fn public_resume_continues_a_saved_revision_through_http() {
        http_continuation(false, true, false, false, false).await;
    }
    #[tokio::test]
    async fn public_resume_handles_a_whole_goal_revision_without_a_graph() {
        http_continuation(false, true, true, false, false).await;
    }
    #[tokio::test]
    async fn recovered_failed_solver_continues_through_real_http_without_replay() {
        http_continuation(false, true, false, true, false).await;
    }
    #[tokio::test]
    async fn public_resume_preserves_custom_order_through_real_http_without_replay() {
        http_continuation(false, false, false, false, true).await;
    }
    #[tokio::test]
    async fn corrupted_custom_schedule_denies_resume_before_http_or_state_change() {
        for mutation in [
            "UPDATE events SET payload=json_set(payload,'$.order',json('[0,1]')) WHERE kind='subtask_schedule'",
            "DELETE FROM events WHERE kind='subtask_schedule'",
            "UPDATE events SET payload=json_set(payload,'$.scheduler.version','2.0.0') WHERE kind='subtask_schedule'",
        ] {
            let root=std::env::temp_dir().join(format!("fabric-custom-denial-{}",uuid::Uuid::new_v4()));
            let (store,task)=fixture_in_order(Arc::new(Store::open(&root.join("state.db")).unwrap()),true).await;
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint=format!("http://{}/v1",listener.local_addr().unwrap());
            {
                let db=store.lock().unwrap();
                db.execute("UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",rusqlite::params![task,endpoint]).unwrap();
                db.execute_batch(mutation).unwrap();
            }
            let before=store.task(&task).unwrap();
            assert!(preview(&store,&task).is_err());
            assert!(resume(store.clone(),&task,true,std::future::pending()).await.is_err());
            assert_eq!(store.task(&task).unwrap(),before);
            assert_eq!(store.inference_audit(&task).unwrap()["reserved_calls"],2);
            assert!(tokio::time::timeout(Duration::from_millis(30),listener.accept()).await.is_err());
            drop(listener);drop(store);std::fs::remove_dir_all(root).unwrap();
        }
    }

    async fn http_continuation(
        registered: bool,
        revision: bool,
        no_graph: bool,
        failed: bool,
        custom_order: bool,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let root =
            std::env::temp_dir().join(format!("fabric-continuation-{}", uuid::Uuid::new_v4()));
        let (store, task) = fixture_in_order(
            Arc::new(Store::open(&root.join("state.db")).unwrap()),
            custom_order,
        )
        .await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",rusqlite::params![task,endpoint]).unwrap();
        if registered {
            register_saved_model(&store, &task);
        }
        if no_graph {
            save_no_graph_boundary(&store, &task).await;
        } else if revision {
            save_revision_boundary(&store, &task).await;
        }
        if failed {
            truncate_after_last_solver(&store, &task);
            let report = store.recover_verified_result(&task).unwrap();
            assert_eq!(report["revision_ready"], true);
            assert_eq!(store.inference_audit(&task).unwrap()["reserved_calls"], 4);
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
            let answers = if revision {
                vec!["[1,2,3]"]
            } else {
                vec!["remaining advice", "[1,2,3]"]
            };
            for (request_index, answer) in answers.into_iter().enumerate() {
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
                            if custom_order {
                                let messages = body["messages"].to_string();
                                if request_index == 0 {
                                    assert!(messages.contains("Solve only this advisory subgoal"));
                                    assert!(messages.contains("Inspect input"));
                                    assert!(!messages.contains("Independent advice"));
                                } else {
                                    assert!(messages.contains("original advice"));
                                    assert!(messages.contains("remaining advice"));
                                }
                            }
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
            if no_graph {
                3
            } else if revision {
                5
            } else {
                4
            }
        );
        if custom_order {
            let audit = inference::audit_from(&db, &task).unwrap();
            assert_eq!(audit["reserved_output_tokens"], 1024);
            assert_eq!(audit["remaining_inference_admission"]["calls"], 4);
            assert_eq!(
                audit["remaining_inference_admission"]["reserved_output_tokens"],
                1976
            );
            let (_, graph, nodes) = super::super::graph::graph_from(&db, &task).unwrap();
            let graph = graph.unwrap();
            assert_eq!(
                super::super::graph::order_from(&db, &task, &graph).unwrap(),
                vec![1, 0]
            );
            assert_eq!(nodes[1].result.as_deref(), Some("original advice"));
            assert_eq!(nodes[0].result.as_deref(), Some("remaining advice"));
            assert_eq!(db.query_row("SELECT count(*) FROM events WHERE task_id=?1 AND kind='subtask_schedule_intent'", [&task], |r|r.get::<_,u64>(0)).unwrap(), 1);
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM events WHERE task_id=?1 AND kind='subtask_schedule'",
                    [&task],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
                1
            );
        }
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
    async fn save_no_graph_boundary(store: &Arc<Store>, task: &str) {
        save_non_decomposed_boundary(store, task, "fallback").await;
    }
    async fn save_non_decomposed_boundary(store: &Arc<Store>, task: &str, kind: &str) {
        let raw: String = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM events WHERE task_id=?1 AND kind='execution_settings'",
                [task],
                |r| r.get(0),
            )
            .unwrap();
        let settings: Value = serde_json::from_str(&raw).unwrap();
        let mut cfg: Config = serde_json::from_value(settings["configuration"].clone()).unwrap();
        store.lock().unwrap().execute("DELETE FROM events WHERE task_id=?1 AND kind NOT IN ('execution_options','module_snapshot','model_registration')",[task]).unwrap();
        let mode = if kind == "fast" { "fast" } else { "deep" };
        let script = if kind == "fast" {
            vec![("solver", "[1,2,3]")]
        } else {
            vec![
                ("planner", if kind == "skipped" { PLAN } else { "no graph" }),
                ("solver", "[3,2,1]"),
                ("solver", "[1,2,3]"),
            ]
        };
        if kind == "skipped" {
            cfg.max_calls = 3;
        }
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE tasks SET mode=?2 WHERE id=?1",
                rusqlite::params![task, mode],
            )
            .unwrap();
        let backend = Arc::new(Script(Mutex::new(script.into())));
        let mut original = Session::new(cfg, store.clone(), backend, task.into(), mode).unwrap();
        original.solve(QUERY, "", mode).await.unwrap();
        let db = store.lock().unwrap();
        let id:u64=db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.cycles_before')=?2",rusqlite::params![task,if kind=="fast" {0} else {1}],|r|r.get(0)).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            rusqlite::params![task, id],
        )
        .unwrap();
    }
    #[tokio::test]
    async fn fast_and_budget_skipped_plans_resume_at_their_saved_boundaries() {
        for kind in ["fast", "skipped"] {
            let (store, task) = fixture().await;
            save_non_decomposed_boundary(&store, &task, kind).await;
            let saved = prepare(&store.lock().unwrap(), &task).unwrap();
            if kind == "skipped" {
                assert!(saved.subtasks.iter().all(|s| s.state == "skipped"));
            } else {
                assert!(saved.subtasks.is_empty());
            }
            let backend = Arc::new(Script(Mutex::new(vec![("solver", "[1,2,3]")].into())));
            let mut resumed = Session::restored(
                saved.cfg,
                store.clone(),
                backend,
                task.clone(),
                &saved.mode,
                saved.state,
            )
            .unwrap();
            resumed
                .continue_solver(
                    saved.boundary.as_ref().unwrap(),
                    &saved.query,
                    &saved.context,
                    &saved.mode,
                )
                .await
                .unwrap();
            assert!(resumed.verified);
            assert_eq!(resumed.resources.calls, if kind == "fast" { 1 } else { 3 });
            assert_eq!(resumed.resources.cycles, if kind == "fast" { 1 } else { 2 });
        }
    }
    #[tokio::test]
    async fn no_graph_recovery_refuses_missing_valid_plans_and_consumed_boundaries() {
        let (store, task) = fixture().await;
        save_no_graph_boundary(&store, &task).await;
        let saved = prepare(&store.lock().unwrap(), &task).unwrap();
        assert!(saved.subtasks.is_empty());
        assert_eq!(saved.state.resources.calls, 2);
        store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.text',?2,'$.node.content_hash',?3) WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='planner'",rusqlite::params![task,PLAN,sha256(PLAN.as_bytes())]).unwrap();
        assert!(prepare(&store.lock().unwrap(), &task).is_err());
        store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.text','no graph','$.node.content_hash',?2) WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='planner'",rusqlite::params![task,sha256(b"no graph")]).unwrap();
        store
            .event(
                &task,
                "cycle_reservation",
                "jev-r",
                json!({"format_version":1,"cycle":2}),
            )
            .unwrap();
        assert!(prepare(&store.lock().unwrap(), &task).is_err());
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
            .continue_graph(
                saved.subtasks,
                &saved.query,
                &saved.context,
                &saved.mode,
                None,
            )
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
    #[tokio::test]
    async fn repeated_revision_recovery_preserves_one_original_budget_and_context() {
        let (store, task) = fixture().await;
        save_revision_boundary(&store, &task).await;
        let saved = prepare(&store.lock().unwrap(), &task).unwrap();
        let backend = Arc::new(Script(Mutex::new(
            vec![("solver", "[2,3,1]"), ("solver", "[1,2,3]")].into(),
        )));
        let mut second = Session::restored(
            saved.cfg,
            store.clone(),
            backend,
            task.clone(),
            &saved.mode,
            saved.state,
        )
        .unwrap();
        second
            .continue_solver(
                saved.boundary.as_ref().unwrap(),
                &saved.query,
                &saved.context,
                &saved.mode,
            )
            .await
            .unwrap();
        {
            let db = store.lock().unwrap();
            let id:u64=db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.cycles_before')=2",[&task],|r|r.get(0)).unwrap();
            db.execute(
                "DELETE FROM events WHERE task_id=?1 AND id>?2",
                rusqlite::params![task, id],
            )
            .unwrap();
        }
        let saved = prepare(&store.lock().unwrap(), &task).unwrap();
        assert_eq!(saved.state.resources.calls, 5);
        assert_eq!(saved.state.resources.cycles, 2);
        let backend = Arc::new(Script(Mutex::new(vec![("solver", "[1,2,3]")].into())));
        let mut third = Session::restored(
            saved.cfg,
            store.clone(),
            backend,
            task.clone(),
            &saved.mode,
            saved.state,
        )
        .unwrap();
        third
            .continue_solver(
                saved.boundary.as_ref().unwrap(),
                &saved.query,
                &saved.context,
                &saved.mode,
            )
            .await
            .unwrap();
        assert!(third.verified);
        assert_eq!(third.resources.calls, 6);
        assert_eq!(third.resources.cycles, 3);
        let db = store.lock().unwrap();
        let settings: u32 = db
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='execution_settings'",
                [&task],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(settings, 1);
        let contexts: u32 = db
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='reasoning_context'",
                [&task],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(contexts, 4);
        assert!(
            prepare(&db, &task).is_err(),
            "Consumed final cycle cannot be reset"
        );
    }
    #[tokio::test]
    async fn repeated_contexts_must_match_original_bytes_and_chronological_charges() {
        let (store, task) = fixture().await;
        save_revision_boundary(&store, &task).await;
        let raw:String=store.lock().unwrap().query_row("SELECT payload FROM events WHERE task_id=?1 AND kind='reasoning_context' ORDER BY id DESC LIMIT 1",[&task],|r|r.get(0)).unwrap();
        for (field, value) in [
            ("context", json!("changed")),
            ("calls_before", json!(0)),
            ("cycles_before", json!(1)),
            ("permissions_restorable", json!(true)),
        ] {
            let mut changed: Value = serde_json::from_str(&raw).unwrap();
            changed[field] = value;
            store.lock().unwrap().execute("UPDATE events SET payload=?2 WHERE id=(SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_context')",rusqlite::params![task,changed.to_string()]).unwrap();
            assert!(
                prepare(&store.lock().unwrap(), &task).is_err(),
                "Accepted context mutation {field}"
            );
        }
        store.lock().unwrap().execute("UPDATE events SET payload=?2 WHERE id=(SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_context')",rusqlite::params![task,raw]).unwrap();
        let mut context: Value = serde_json::from_str(&raw).unwrap();
        context["calls_before"] = json!(4);
        context["cycles_before"] = json!(1);
        for _ in 0..15 {
            store
                .event(&task, "reasoning_context", "jev-r", context.clone())
                .unwrap();
        }
        assert!(prepare(&store.lock().unwrap(), &task).is_ok());
        store
            .event(&task, "reasoning_context", "jev-r", context)
            .unwrap();
        assert!(
            prepare(&store.lock().unwrap(), &task).is_err(),
            "Exceeded bounded context history"
        );
    }
    fn truncate_after_last_solver(store: &Store, task: &str) {
        let db = store.lock().unwrap();
        let id:u64=db.query_row("SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.role')='solver'",[task],|r|r.get(0)).unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            rusqlite::params![task, id],
        )
        .unwrap();
    }
    #[tokio::test]
    async fn failed_saved_solver_recovery_prepares_a_fresh_revision_without_repeating_inference() {
        let root =
            std::env::temp_dir().join(format!("fabric-failed-revision-{}", uuid::Uuid::new_v4()));
        let (store, task) =
            fixture_in(Arc::new(Store::open(&root.join("state.db")).unwrap())).await;
        save_revision_boundary(&store, &task).await;
        truncate_after_last_solver(&store, &task);
        assert!(prepare(&store.lock().unwrap(), &task).is_err());
        let report = store.recover_verified_result(&task).unwrap();
        assert_eq!(report["verified"], false);
        assert_eq!(report["revision_ready"], true);
        assert_eq!(report["inference_repeated"], false);
        let saved = prepare(&store.lock().unwrap(), &task).unwrap();
        assert_eq!(saved.state.resources.calls, 4);
        assert_eq!(saved.state.resources.cycles, 1);
        assert_eq!(saved.state.resources.tool_calls, 1);
        let backend = Arc::new(Script(Mutex::new(vec![("solver", "[1,2,3]")].into())));
        let mut resumed = Session::restored(
            saved.cfg,
            store.clone(),
            backend,
            task.clone(),
            &saved.mode,
            saved.state,
        )
        .unwrap();
        resumed
            .continue_solver(
                saved.boundary.as_ref().unwrap(),
                &saved.query,
                &saved.context,
                &saved.mode,
            )
            .await
            .unwrap();
        assert!(resumed.verified);
        assert_eq!(resumed.resources.calls, 5);
        assert_eq!(resumed.resources.tool_calls, 2);
        drop(resumed);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn recovered_revision_audit_failure_rolls_back_fresh_verification_charges() {
        let root = std::env::temp_dir().join(format!(
            "fabric-failed-revision-audit-{}",
            uuid::Uuid::new_v4()
        ));
        let (store, task) =
            fixture_in(Arc::new(Store::open(&root.join("state.db")).unwrap())).await;
        save_revision_boundary(&store, &task).await;
        truncate_after_last_solver(&store, &task);
        store.lock().unwrap().execute_batch("CREATE TRIGGER reject_revision BEFORE INSERT ON events WHEN NEW.kind='reasoning_boundary' BEGIN SELECT RAISE(ABORT,'fixture revision write failure'); END;").unwrap();
        assert!(store.recover_verified_result(&task).is_err());
        assert_eq!(
            store.inference_audit(&task).unwrap()["remaining_inference_admission"]["tool_calls"],
            4
        );
        assert_eq!(store.task(&task).unwrap()["task"]["status"], "interrupted");
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn repeated_failed_solver_results_do_not_receive_another_revision() {
        let root =
            std::env::temp_dir().join(format!("fabric-repeat-revision-{}", uuid::Uuid::new_v4()));
        let (store, task) =
            fixture_in(Arc::new(Store::open(&root.join("state.db")).unwrap())).await;
        save_revision_boundary(&store, &task).await;
        let saved = prepare(&store.lock().unwrap(), &task).unwrap();
        let backend = Arc::new(Script(Mutex::new(vec![("solver", "[3,2,1]")].into())));
        let mut resumed = Session::restored(
            saved.cfg,
            store.clone(),
            backend,
            task.clone(),
            &saved.mode,
            saved.state,
        )
        .unwrap();
        resumed
            .continue_solver(
                saved.boundary.as_ref().unwrap(),
                &saved.query,
                &saved.context,
                &saved.mode,
            )
            .await
            .unwrap();
        truncate_after_last_solver(&store, &task);
        let report = store.recover_verified_result(&task).unwrap();
        assert_eq!(report["revision_ready"], false);
        assert!(prepare(&store.lock().unwrap(), &task).is_err());
        assert_eq!(store.inference_audit(&task).unwrap()["reserved_calls"], 5);
        drop(resumed);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn parallel_reconciliation_preview_is_read_only_and_resume_publishes_without_replay() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

        let root = std::env::temp_dir().join(format!(
            "fabric-parallel-continuation-{}",
            uuid::Uuid::new_v4()
        ));
        let persistent = Arc::new(Store::open(&root.join("state.db")).unwrap());
        let (store, task) = unpublished_parallel_result_fixture(persistent).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        store.lock().unwrap().execute(
            "UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",
            rusqlite::params![task, endpoint],
        ).unwrap();

        let before = store.task(&task).unwrap();
        let preview = preview(&store, &task).unwrap();
        assert_eq!(preview["parallel_result_publications_to_reconcile"], 1);
        assert_eq!(preview["consumed"]["calls"], 3);
        assert_eq!(preview["resume_authorized"], false);
        assert_eq!(store.task(&task).unwrap(), before);
        assert_eq!(
            before["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(
                    |event| event["payload"]["origin"] == "operator_parallel_result_reconciliation"
                )
                .count(),
            0
        );

        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut length = None;
            loop {
                let mut line = String::new();
                assert!(socket.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length: ") {
                    length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            let mut body = vec![0; length.unwrap()];
            socket.read_exact(&mut body).await.unwrap();
            let payload: Value = serde_json::from_slice(&body).unwrap();
            assert!(payload["messages"]
                .to_string()
                .contains("first parallel advice"));
            assert!(payload["messages"]
                .to_string()
                .contains("second parallel advice"));
            let response = json!({"choices":[{"message":{"content":"[1,2,3]"}}]}).to_string();
            socket.get_mut().write_all(format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(), response
            ).as_bytes()).await.unwrap();
        });

        let report = tokio::time::timeout(
            Duration::from_secs(10),
            resume(store.clone(), &task, true, std::future::pending()),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        assert_eq!(report["status"], "completed");
        let after = store.task(&task).unwrap();
        assert_eq!(
            after["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(
                    |event| event["payload"]["origin"] == "operator_parallel_result_reconciliation"
                )
                .count(),
            1
        );
        let audit = store.inference_audit(&task).unwrap();
        assert_eq!(audit["reserved_calls"], 4);
        assert_eq!(audit["reserved_output_tokens"], 1024);
        assert_eq!(audit["results_not_recorded"], 0);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn parallel_reconciliation_audit_failure_rolls_back_transition_and_task_admission() {
        let root = std::env::temp_dir().join(format!(
            "fabric-parallel-reconciliation-rollback-{}",
            uuid::Uuid::new_v4()
        ));
        let persistent = Arc::new(Store::open(&root.join("state.db")).unwrap());
        let (store, task) = unpublished_parallel_result_fixture(persistent).await;
        store.lock().unwrap().execute_batch("CREATE TRIGGER reject_parallel_continuation_audit BEFORE INSERT ON events WHEN NEW.kind='continuation' AND json_extract(NEW.payload,'$.state')='started' BEGIN SELECT RAISE(ABORT,'fixture continuation audit failure'); END;").unwrap();
        let before = store.task(&task).unwrap();
        assert!(resume(store.clone(), &task, true, std::future::pending())
            .await
            .is_err());
        let after = store.task(&task).unwrap();
        assert_eq!(after, before);
        assert_eq!(after["task"]["status"], "interrupted");
        assert_eq!(
            after["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(
                    |event| event["payload"]["origin"] == "operator_parallel_result_reconciliation"
                )
                .count(),
            0
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
