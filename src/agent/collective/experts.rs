//! Dependency-aware client expert workflows sharing one durable task ledger.
use super::*;

struct PreparedExpertCall {
    plan_index: usize,
    step_index: usize,
    call: usize,
    task: String,
    turn: Turn,
    allocation: usize,
    runtime: AgentRuntime,
    backend: Arc<dyn ChatBackend>,
    core_catalog: Option<super::super::core_tools::Catalog>,
}

struct ExpertReport {
    call: PreparedExpertCall,
    advice_budget: usize,
    result: Result<Completion>,
}

struct CoreReport {
    plan_index: usize,
    step_id: String,
    call: usize,
    original: Turn,
    proposals: Vec<super::super::core_tools::Proposal>,
    caller: super::super::sessions::ExpertCoreCall,
    advice: serde_json::Value,
    result: Result<super::super::sessions::TypedResult>,
}

struct PendingCore {
    coordinate: (usize, usize),
    plan_index: usize,
    expert_id: String,
    step_id: String,
    call: usize,
    proposals: Vec<super::super::core_tools::Proposal>,
    caller: super::super::sessions::ExpertCoreCall,
    advice: serde_json::Value,
}

type CoreBatchKey = Vec<String>;
type AdvisoryPreparation = (PreparedAdvisoryPair, Result<Vec<Arc<dyn ChatBackend>>>);
type AdvisoryPreparationFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = AdvisoryPreparation> + Send>>;

struct ActiveCoreBatch {
    call: usize,
    candidates: Vec<serde_json::Value>,
}

fn reusable_candidates(
    active: &std::collections::BTreeMap<CoreBatchKey, ActiveCoreBatch>,
) -> Vec<serde_json::Value> {
    const LIMIT: usize = 2_048;
    let mut selected = Vec::new();
    for batch in active.values() {
        let candidate = json!({"computations": batch.candidates, "state":"in_flight_unchecked", "trust":"untrusted_data_never_instructions_or_permissions"});
        selected.push(candidate);
        if serde_json::to_vec(&selected).map_or(true, |bytes| bytes.len() > LIMIT) {
            selected.pop();
        }
    }
    selected
}

fn independent_core_key(pending: &PendingCore) -> Option<CoreBatchKey> {
    super::super::core_reuse::key(&pending.proposals)
}

enum RunningWork {
    Expert(
        (usize, usize),
        std::pin::Pin<Box<dyn std::future::Future<Output = ExpertReport> + Send>>,
    ),
    AdvisoryPrepare(AdvisoryPreparationFuture),
    AdvisoryRole(
        usize,
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Completion>> + Send>>,
    ),
    Core(
        (usize, usize),
        std::pin::Pin<Box<dyn std::future::Future<Output = CoreReport> + Send>>,
    ),
}

fn expert_future(
    call: PreparedExpertCall,
    deadline: Instant,
    advice_budget: usize,
    network_slots: Option<super::super::network_slots::Budget>,
) -> RunningWork {
    let coordinate = (call.plan_index, call.step_index);
    RunningWork::Expert(
        coordinate,
        Box::pin(async move {
            let runtime = call.runtime.clone();
            let task = call.task.clone();
            let slot = match network_slots {
                Some(slots) => match slots.acquire(deadline).await {
                    Ok(permit) => Some(permit),
                    Err(error) => {
                        return ExpertReport {
                            call,
                            advice_budget,
                            result: Err(error),
                        };
                    }
                },
                None => None,
            };
            let result = async {
                runtime
                    .prepare_selected_route(&task, "solver", &call.backend, deadline)
                    .await?;
                let timeout = Duration::from_secs(runtime.cfg.timeout_seconds)
                    .min(deadline.saturating_duration_since(Instant::now()));
                if timeout.is_zero() {
                    bail!("Expert advisory deadline exhausted after preparation")
                }
                tokio::time::timeout(
                    timeout,
                    runtime.dispatch_selected_role(
                        call.backend.clone(),
                        &task,
                        "solver",
                        &call.turn,
                        call.allocation,
                        deadline,
                    ),
                )
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "Expert advisory call timed out; provider outcome may be unknown"
                    )
                })?
            }
            .await;
            // Keep the shared slot through response-body decoding and error
            // construction; it is released only when this owned future returns.
            drop(slot);
            ExpertReport {
                call,
                advice_budget,
                result,
            }
        }),
    )
}

fn advisory_preparation_future(
    runtime: AgentRuntime,
    backend: Arc<dyn ChatBackend>,
    task: String,
    deadline: Instant,
    pair: PreparedAdvisoryPair,
    network_slots: Option<super::super::network_slots::Budget>,
) -> RunningWork {
    RunningWork::AdvisoryPrepare(Box::pin(async move {
        let slots = match network_slots {
            Some(slots) => match slots.acquire_many(2, deadline).await {
                Ok(permit) => Some(permit),
                Err(error) => return (pair, Err(error)),
            },
            None => None,
        };
        let result = runtime
            .prepare_role_wave(backend, &task, &["planner", "explorer"], deadline)
            .await;
        drop(slots);
        (pair, result)
    }))
}

fn advisory_role_future(
    runtime: AgentRuntime,
    backend: Arc<dyn ChatBackend>,
    task: String,
    deadline: Instant,
    index: usize,
    turn: Turn,
    network_slots: Option<super::super::network_slots::Budget>,
) -> RunningWork {
    RunningWork::AdvisoryRole(
        index,
        Box::pin(async move {
            let _slot = match network_slots {
                Some(slots) => Some(slots.acquire(deadline).await?),
                None => None,
            };
            let role = match index {
                0 => "planner",
                1 => "explorer",
                _ => bail!("Invalid advisory role index"),
            };
            let timeout = Duration::from_secs(runtime.cfg.timeout_seconds)
                .min(deadline.saturating_duration_since(Instant::now()));
            if timeout.is_zero() {
                bail!("Advisory role deadline exhausted before inference")
            }
            tokio::time::timeout(
                timeout,
                runtime.dispatch_selected_role(
                    backend,
                    &task,
                    role,
                    &turn,
                    ADVICE_TOKENS,
                    deadline,
                ),
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Client advisory role deadline exhausted; provider outcome may be unknown"
                )
            })?
        }),
    )
}

enum CompletedWork {
    Expert(Box<ExpertReport>),
    AdvisoryPrepare(Box<AdvisoryPreparation>),
    AdvisoryRole(usize, Result<Completion>),
    Core(Box<CoreReport>),
}

async fn ready_expert_work(operations: &mut Vec<RunningWork>) -> Vec<CompletedWork> {
    use std::future::poll_fn;
    use std::task::Poll;
    let ready = poll_fn(|context| {
        let mut ready = Vec::new();
        for (index, operation) in operations.iter_mut().enumerate() {
            match operation {
                RunningWork::Expert(_, future) => {
                    if let Poll::Ready(report) = future.as_mut().poll(context) {
                        ready.push((index, CompletedWork::Expert(Box::new(report))));
                    }
                }
                RunningWork::AdvisoryPrepare(future) => {
                    if let Poll::Ready((pair, result)) = future.as_mut().poll(context) {
                        ready.push((
                            index,
                            CompletedWork::AdvisoryPrepare(Box::new((pair, result))),
                        ));
                    }
                }
                RunningWork::AdvisoryRole(role, future) => {
                    if let Poll::Ready(result) = future.as_mut().poll(context) {
                        ready.push((index, CompletedWork::AdvisoryRole(*role, result)));
                    }
                }
                RunningWork::Core(_, future) => {
                    if let Poll::Ready(report) = future.as_mut().poll(context) {
                        ready.push((index, CompletedWork::Core(Box::new(report))));
                    }
                }
            }
        }
        if ready.is_empty() {
            Poll::Pending
        } else {
            Poll::Ready(ready)
        }
    })
    .await;
    let mut indices = ready.iter().map(|(index, _)| *index).collect::<Vec<_>>();
    indices.sort_unstable_by(|left, right| right.cmp(left));
    for index in indices {
        drop(operations.swap_remove(index));
    }
    ready.into_iter().map(|(_, work)| work).collect()
}

#[derive(Clone, Copy)]
struct ExpertWaveLimits {
    experts: usize,
    per_expert: usize,
    total: usize,
}

fn ready_expert_wave(
    plans: &[expert_workflow::BoundPlan],
    done: &[std::collections::BTreeSet<String>],
    results: &[serde_json::Map<String, serde_json::Value>],
    limits: ExpertWaveLimits,
    in_flight: &std::collections::BTreeSet<(usize, usize)>,
    waiting_reuse: &std::collections::BTreeSet<(usize, usize)>,
    typed_ready: Option<&[bool]>,
) -> Result<Vec<(usize, usize, serde_json::Value)>> {
    use super::super::graph_schedule::{self, Dependency, Evidence, Node};
    let ExpertWaveLimits {
        experts: expert_capacity,
        per_expert: per_expert_capacity,
        total: global_capacity,
    } = limits;
    // Optional overlap probes may have no remaining slots. They select no work,
    // rather than submitting a zero-width wave to the shared scheduler.
    if global_capacity == 0 {
        return Ok(Vec::new());
    }
    if done.len() != plans.len() || results.len() != plans.len() {
        bail!("Expert graph completion state does not match its plans")
    }
    if !in_flight.is_disjoint(waiting_reuse) {
        bail!("A waiting reuse alias cannot also own an execution slot")
    }
    let coordinates = plans
        .iter()
        .enumerate()
        .flat_map(|(plan_index, bound)| {
            (0..bound.plan.steps.len()).map(move |step_index| (plan_index, step_index))
        })
        .collect::<Vec<_>>();
    let nodes = coordinates
        .iter()
        .map(|&(plan_index, step_index)| {
            let bound = &plans[plan_index];
            let step = &bound.plan.steps[step_index];
            let mut sources = std::collections::BTreeSet::new();
            for dependency in &step.depends_on {
                let source = coordinates
                    .iter()
                    .position(|&(owner, index)| {
                        owner == plan_index && plans[owner].plan.steps[index].id == *dependency
                    })
                    .ok_or_else(|| anyhow::anyhow!("Expert step dependency is missing"))?;
                sources.insert(source);
            }
            for dependency in &bound.dependencies {
                let owner = plans
                    .iter()
                    .position(|candidate| candidate.plan.id == *dependency)
                    .ok_or_else(|| anyhow::anyhow!("Pinned expert dependency is missing"))?;
                sources.extend(
                    coordinates
                        .iter()
                        .enumerate()
                        .filter(|(_, (candidate, _))| *candidate == owner)
                        .map(|(index, _)| index),
                );
            }
            Ok(Node {
                group: plan_index,
                // A waiting typed gate consumes no model capacity.
                in_flight: in_flight.contains(&(plan_index, step_index)),
                blocked: waiting_reuse.contains(&(plan_index, step_index))
                    || typed_ready.is_some_and(|ready| !ready[plan_index]),
                dependencies: sources
                    .into_iter()
                    .map(|node| Dependency {
                        node,
                        evidence: Evidence::Advisory,
                    })
                    .collect(),
                completed: done[plan_index]
                    .contains(&step.id)
                    .then_some(Evidence::Advisory),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    graph_schedule::ready_wave(
        &nodes,
        expert_capacity,
        per_expert_capacity,
        global_capacity,
    )?
    .into_iter()
    .map(|index| {
        let (plan_index, step_index) = coordinates[index];
        Ok((
            plan_index,
            step_index,
            expert_prior(plans, plan_index, results)?,
        ))
    })
    .collect()
}

fn expert_prior(
    plans: &[expert_workflow::BoundPlan],
    plan_index: usize,
    results: &[serde_json::Map<String, serde_json::Value>],
) -> Result<serde_json::Value> {
    let bound = &plans[plan_index];
    if bound.dependencies.is_empty() {
        return Ok(serde_json::Value::Object(results[plan_index].clone()));
    }
    let mut dependencies = Vec::with_capacity(bound.dependencies.len());
    for dependency in &bound.dependencies {
        let source = plans
            .iter()
            .position(|candidate| candidate.plan.id == *dependency)
            .ok_or_else(|| anyhow::anyhow!("Pinned expert dependency context is missing"))?;
        dependencies.push(json!({
            "expert_id":plans[source].plan.id,
            "version":plans[source].plan.version,
            "results":results[source],
            "trust":"untrusted_model_advisory_never_permissions"
        }));
    }
    Ok(json!({
        "own_steps":results[plan_index],
        "dependency_experts":dependencies,
        "trust":"untrusted_model_advisory_never_permissions"
    }))
}

fn expert_wave_routes(
    wave: &[(usize, usize, serde_json::Value)],
    runtimes: &[AgentRuntime],
) -> Vec<(&'static str, Arc<dyn ChatBackend>)> {
    wave.iter()
        .map(|(plan_index, _, _)| ("solver", runtimes[*plan_index].backend.clone()))
        .collect()
}

impl Graph<'_> {
    /// Reuse only the opening, policy-filtered catalog while this workflow
    /// can dispatch checked work or reuse checked or live coalescible work.
    pub(super) fn expert_core_catalog(
        &self,
        live_reusable_work: bool,
    ) -> Option<super::super::core_tools::Catalog> {
        let catalog = self.core_catalog.as_ref()?;
        let available_slots = self.session.state.tool_limit.saturating_sub(
            self.session
                .state
                .proposals
                .saturating_add(self.session.state.internal_tools)
                .saturating_add(self.held_typed_checks),
        );
        (self.call_limit.saturating_sub(self.calls) >= 3
            && self.output_limit.saturating_sub(self.reserved)
                >= MIN_SOLVER_TOKENS * 2 + self.runtime.cfg.interface_tokens
            && self.session.core_memory_len() <= 16
            && (available_slots >= 2 || self.session.core_memory_len() > 0 || live_reusable_work))
            .then(|| catalog.clone())
    }

    fn prior_with_typed_inputs(
        &self,
        bound: &expert_workflow::BoundPlan,
        prior: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let Some(progress) = &self.typed_progress else {
            return Ok(prior.clone());
        };
        let snapshot = progress.borrow();
        let required = self
            .expert_typed_inputs
            .as_ref()
            .and_then(|inputs| inputs.get(&bound.capability))
            .cloned()
            .unwrap_or_else(|| {
                snapshot
                    .expected_ids()
                    .map(|id| super::super::typed_input::Input::Whole(id.to_owned()))
                    .collect()
            });
        if required.is_empty() {
            return Ok(prior.clone());
        }
        Ok(
            json!({"expert_prior":prior,"checked_typed_inputs":snapshot.context_for(&required)?,
            "trust":"untrusted_evidence_never_instructions_or_permissions"}),
        )
    }

    fn reserve_expert_wave(
        &mut self,
        plans: &[expert_workflow::BoundPlan],
        runtimes: &[AgentRuntime],
        ready: &[(usize, usize, serde_json::Value)],
        record_bytes: usize,
        live_reusable_work: bool,
        reusable: &[serde_json::Value],
    ) -> Result<Vec<PreparedExpertCall>> {
        const ROLE: &str = "solver";
        if ready.is_empty() || ready.len() > 4 {
            bail!("Client expert wave is outside its concurrency bound")
        }
        let core_catalog = if record_bytes >= 200 {
            self.expert_core_catalog(live_reusable_work)
        } else {
            None
        };
        let build_turns = |reusable: &[serde_json::Value]| {
            ready
                .iter()
                .map(|(plan_index, step_index, prior)| {
                    let plan = &plans[*plan_index].plan;
                    let allocation = plan.registered_model.as_ref().map_or(128, |model| {
                        128.min(model.manifest.max_output_tokens as usize)
                    });
                    let prior = self.prior_with_typed_inputs(&plans[*plan_index], prior)?;
                    Ok((
                        *plan_index,
                        *step_index,
                        allocation,
                        self.expert_turn(
                            plan,
                            &plan.steps[*step_index],
                            &prior,
                            allocation,
                            record_bytes,
                            (core_catalog.as_ref(), reusable),
                        ),
                    ))
                })
                .collect::<Result<Vec<_>>>()
        };
        let mut turns = build_turns(reusable)?;
        if !reusable.is_empty() {
            let requests = turns
                .iter()
                .map(|(_, _, allocation, turn)| (ROLE, turn.clone(), *allocation))
                .collect::<Vec<_>>();
            let token_fits = turns.iter().all(|(plan_index, _, _, turn)| {
                local::validate_token_request(&runtimes[*plan_index].cfg, turn).is_ok()
            });
            if !token_fits || !self.available_turns_with_presentation(&requests, false)? {
                turns = build_turns(&[])?;
            }
        }
        // Capture the exact catalog used by this serialized prompt. A later
        // budget or policy change must not make a request executable.
        let requests = turns
            .iter()
            .map(|(_, _, allocation, turn)| (ROLE, turn.clone(), *allocation))
            .collect::<Vec<_>>();
        let routes = turns
            .iter()
            .map(|(plan_index, _, _, _)| (ROLE, runtimes[*plan_index].backend.clone()))
            .collect::<Vec<_>>();
        if !self.runtime.inference_wave_fits(&routes)? {
            bail!("Client expert wave does not fit the shared resource admission snapshot")
        }
        for ((plan_index, _, _, _), (_, turn, _)) in turns.iter().zip(&requests) {
            local::validate_token_request(&runtimes[*plan_index].cfg, turn)?;
        }
        if !self.available_turns_with_presentation(&requests, false)? {
            bail!("Pinned expert workflow wave exceeds the shared client task budget")
        }
        let client_policy = TaskPolicy::client_collective(self.task)?;
        let agent_policy = TaskPolicy::agent(self.task)?;
        for _ in &turns {
            client_policy.authorize(self.task, "jev-plus", "expert.workflow.advice")?;
            agent_policy.authorize(self.task, "jev-r", "llm.generate")?;
        }
        // Resolve every route and serialize every request before the first
        // irreversible reservation. Inference futures never reselect a role.
        let selected = turns
            .iter()
            .map(|(plan_index, _, _, turn)| {
                let runtime = runtimes[*plan_index].clone();
                let route = runtime.backend.for_role(ROLE)?;
                if runtime.cfg.language_model().is_some() && route.is_none() {
                    bail!("Pinned expert backend cannot honor the configured reasoning route")
                }
                let backend = route.unwrap_or_else(|| runtime.backend.clone());
                runtime.validate_selected_target(ROLE, backend.as_ref())?;
                Ok((runtime, backend, serde_json::to_vec(turn)?))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut prepared = Vec::with_capacity(turns.len());
        for ((plan_index, step_index, allocation, turn), (runtime, backend, bytes)) in
            turns.into_iter().zip(selected)
        {
            self.session
                .reserve("expert-advisory", allocation, &bytes, self.cycles)?;
            self.calls += 1;
            self.reserved += allocation;
            self.prompt_bytes += bytes.len();
            prepared.push(PreparedExpertCall {
                plan_index,
                step_index,
                call: self.calls,
                task: self.task.to_owned(),
                turn,
                allocation,
                runtime,
                backend,
                core_catalog: core_catalog.clone(),
            });
        }
        self.runtime.store.event(
            self.task,
            "expert_workflow_wave_reserved",
            "jev-plus",
            json!({
                "bindings":prepared.iter().map(|call|json!({"capability":plans[call.plan_index].capability,"expert_id":plans[call.plan_index].plan.id,"step_id":plans[call.plan_index].plan.steps[call.step_index].id})).collect::<Vec<_>>(),
                "calls":prepared.iter().map(|call|call.call).collect::<Vec<_>>(),
                "provider_calls_reserved_before_poll":true,
                "permissions_granted":false
            }),
        )?;
        Ok(prepared)
    }

    fn record_expert_completion(
        &mut self,
        plans: &[expert_workflow::BoundPlan],
        report: ExpertReport,
        results: &mut [serde_json::Map<String, serde_json::Value>],
        done: &mut [std::collections::BTreeSet<String>],
    ) -> Result<Option<PendingCore>> {
        let call = report.call;
        let step_id = plans[call.plan_index].plan.steps[call.step_index]
            .id
            .clone();
        let completion = report.result?;
        let mut pending_core = None;
        let outcome = {
            let completion = self.record_completion(
                "solver",
                call.call,
                call.allocation,
                &call.turn,
                completion,
            )?;
            if completion.finish_reason != "stop" {
                bail!("Structured expert advice requires a confirmed complete provider response")
            }
            if !completion.tool_calls.is_empty() {
                bail!("Expert advisory role proposed a client tool")
            }
            let content = completion.content.unwrap_or_default();
            let parsed = super::super::expert_compute::parse(
                &content,
                report.advice_budget,
                call.core_catalog.as_ref(),
            )?;
            if !parsed.proposals.is_empty() {
                let dummy_references = (0..parsed.proposals.len())
                    .map(|index| {
                        json!({
                            "operation_id":format!("core_65535_{index}"),
                            "result_sha256":"0".repeat(64)
                        })
                    })
                    .collect::<Vec<_>>();
                let references_overhead =
                    serde_json::to_vec(&json!({"core_operations":dummy_references}))?
                        .len()
                        .saturating_sub(1);
                let advice_size = serde_json::to_vec(&parsed.advice)?.len();
                if advice_size.saturating_add(references_overhead) > report.advice_budget {
                    bail!("Expert advice and checked-operation references exceed their preflighted output budget")
                }
                // Only fresh source batches consume invocation slots. Cache
                // hits and in-flight aliases are classified before the shared
                // admission check below; they must not be rejected here.
                let caller = super::super::sessions::ExpertCoreCall {
                    call: call.call,
                    request_sha256: sha256(&serde_json::to_vec(&call.turn)?),
                    content_sha256: sha256(content.as_bytes()),
                    model: completion.model,
                };
                pending_core = Some(PendingCore {
                    coordinate: (call.plan_index, call.step_index),
                    plan_index: call.plan_index,
                    expert_id: plans[call.plan_index].plan.id.clone(),
                    step_id: step_id.clone(),
                    call: call.call,
                    proposals: parsed.proposals,
                    caller,
                    advice: parsed.advice,
                });
                "core_pending"
            } else {
                results[call.plan_index].insert(
                    step_id.clone(),
                    json!({
                        "advice":parsed.advice,
                        "partial":false,
                        "trust":"untrusted_model_advisory_never_permissions"
                    }),
                );
                done[call.plan_index].insert(step_id.clone());
                "completed"
            }
        };
        if let Some(pending) = pending_core {
            return Ok(Some(pending));
        }
        self.runtime.store.event(
            self.task,
            "expert_workflow_step_complete",
            "jev-plus",
            json!({
                "expert_id":plans[call.plan_index].plan.id,
                "step_id":step_id,
                "call":call.call,
                "result_state":outcome,
                "advice_format_version":3,
                "structured_advice_validated":true,
                "advice_partial":false,
                "permissions_granted":false
            }),
        )?;
        Ok(None)
    }

    fn schedule_expert_core(&mut self, pending: PendingCore) -> Result<RunningWork> {
        let available = self.session.state.tool_limit.saturating_sub(
            self.session
                .state
                .proposals
                .saturating_add(self.session.state.internal_tools)
                .saturating_add(self.held_typed_checks),
        );
        if (pending.proposals.len() as u32).saturating_mul(2) > available {
            bail!("Expert core work would consume held typed verification slots")
        }
        let mut prepared = self.session.prepare_expert_core_work(
            self.runtime,
            self.original,
            &pending.proposals,
            &pending.caller,
        )?;
        if let Some(slots) = self.network_slots.clone() {
            prepared.share_slots(slots)?;
        }
        self.runtime.store.event(
            self.task,
            "expert_core_batch_scheduled",
            "jev-plus",
            json!({
                "expert_id":pending.expert_id,
                "step_id":pending.step_id,
                "call":pending.call,
                "operation_count":pending.proposals.len(),
                "permissions_granted":false,
                "provider_calls_reserved_before_poll":true
            }),
        )?;
        let original = self.original.clone();
        let coordinate = pending.coordinate;
        Ok(RunningWork::Core(
            coordinate,
            Box::pin(async move {
                let result = prepared.execute().await;
                CoreReport {
                    plan_index: pending.plan_index,
                    step_id: pending.step_id,
                    call: pending.call,
                    original,
                    proposals: pending.proposals,
                    caller: pending.caller,
                    advice: pending.advice,
                    result,
                }
            }),
        ))
    }

    fn record_expert_core_completion(
        &mut self,
        plans: &[expert_workflow::BoundPlan],
        report: CoreReport,
        results: &mut [serde_json::Map<String, serde_json::Value>],
        done: &mut [std::collections::BTreeSet<String>],
    ) -> Result<()> {
        let result = report.result?;
        let references = result.core_references();
        let preview = self.session.preview_expert_core_context(
            &report.original,
            &report.proposals,
            &result,
            &report.caller,
        )?;
        let previous_context = self.core_context.replace(preview);
        let admitted = self.available(&[("solver", &"\0".repeat(2000), MIN_SOLVER_TOKENS)]);
        self.core_context = previous_context;
        if !admitted? {
            bail!("Checked expert core feedback exceeds the original solver and presentation input budget")
        }
        self.core_context = Some(self.session.complete_expert_core_work(
            &report.original,
            &report.proposals,
            &result,
            &report.caller,
        )?);
        results[report.plan_index].insert(
            report.step_id.clone(),
            json!({
                "advice":report.advice,
                "partial":false,
                "trust":"untrusted_model_advisory_never_permissions",
                "core_operations":references
            }),
        );
        done[report.plan_index].insert(report.step_id.clone());
        self.runtime.store.event(
            self.task,
            "expert_workflow_step_complete",
            "jev-plus",
            json!({
                "expert_id":plans[report.plan_index].plan.id,
                "step_id":report.step_id,
                "call":report.call,
                "result_state":"completed_with_checked_core_operations",
                "advice_format_version":4,
                "structured_advice_validated":true,
                "core_operation_count":report.proposals.len(),
                "permissions_granted":false
            }),
        )?;
        Ok(())
    }

    fn record_reused_expert_core(
        &mut self,
        plans: &[expert_workflow::BoundPlan],
        pending: PendingCore,
        reused: super::super::sessions::CoreReuse,
        source_call: Option<usize>,
        results: &mut [serde_json::Map<String, serde_json::Value>],
        done: &mut [std::collections::BTreeSet<String>],
    ) -> Result<()> {
        let previous_context = self.core_context.replace(reused.context.clone());
        let admitted = self.available(&[("solver", &"\0".repeat(2_000), MIN_SOLVER_TOKENS)]);
        self.core_context = previous_context;
        if !admitted? {
            bail!("Reused checked expert core feedback exceeds the original solver and presentation input budget")
        }
        self.core_context = Some(reused.context);
        results[pending.plan_index].insert(
            pending.step_id.clone(),
            json!({
                "advice":pending.advice,
                "partial":false,
                "trust":"untrusted_model_advisory_never_permissions",
                "core_operations":reused.references
            }),
        );
        done[pending.plan_index].insert(pending.step_id.clone());
        self.runtime.store.event(
            self.task,
            "expert_workflow_step_complete",
            "jev-plus",
            json!({
                "expert_id":plans[pending.plan_index].plan.id,
                "step_id":pending.step_id,
                "call":pending.call,
                "result_state":"completed_with_reused_checked_core_operations",
                "advice_format_version":4,
                "core_operation_count":pending.proposals.len(),
                "coalesced_source_call":source_call,
                "permissions_granted":false
            }),
        )?;
        Ok(())
    }

    pub(super) async fn expert_workflow(&mut self, prior_advice: &str) -> Result<Option<String>> {
        if self
            .expert_wave_capacity
            .is_some_and(|capacity| !(1..=4).contains(&capacity))
        {
            bail!("Expert role wave capacity is outside its shared task bound")
        }
        let interpretation_value = super::super::reasoning_memory::interpretation(
            prior_advice,
            self.session.original_objective(self.original)?,
            &self.session.state.references,
        )?;
        let interpretation = if let Some(value) = interpretation_value {
            crate::language::Interpretation::parse(&value.to_string())?
        } else {
            crate::language::Interpretation {
                objective: "Original user request".into(),
                constraints: Vec::new(),
                required_capabilities: Vec::new(),
                capability_versions: std::collections::BTreeMap::new(),
                clarification: None,
                compiled_solver_request: None,
            }
        };
        // An explicit empty scope means typed work covers the advertised
        // computations; it is not permission to use a legacy unrelated workflow.
        if self
            .expert_capability_scope
            .as_ref()
            .is_some_and(Vec::is_empty)
        {
            self.schedule("no_additional_model_expert_capabilities", false)?;
            return Ok(None);
        }
        let interpretation = self.expert_routing_interpretation(&interpretation)?;
        let expert_capacity = self.runtime.cfg.expert_parallelism.unwrap_or(1).min(4);
        let selection = expert_workflow::Plan::bound_plans(
            &self.session.state.references,
            &interpretation,
            expert_capacity,
        )?;
        for skipped in selection.skipped {
            self.runtime.store.event(
                self.task,
                "expert_workflow_skipped",
                "jev-plus",
                json!({"selection":skipped,"permissions_granted":false}),
            )?;
        }
        let plans = selection.plans;
        if plans.is_empty() {
            self.runtime.store.event(self.task, "expert_workflow_skipped", "jev-plus",
                json!({"reason":"no_complete_pinned_workflow_selection","permissions_granted":false}))?;
            return Ok(None);
        }
        let dependency_graph = plans.iter().any(|bound| !bound.dependencies.is_empty());
        let selected_plan_count = plans.len();
        let mut routed_runtimes = Vec::with_capacity(plans.len());
        let mut routed_plans = Vec::with_capacity(plans.len());
        for bound in plans {
            if bound
                .plan
                .registered_model
                .as_ref()
                .is_some_and(|registration| {
                    self.runtime.cfg.language_model() == Some(registration.manifest.model.as_str())
                })
            {
                self.runtime.store.event(
                    self.task,
                    "expert_workflow_skipped",
                    "jev-plus",
                    json!({"expert_id":bound.plan.id,"version":bound.plan.version,"reason":"language_only_model_is_not_an_expert_reasoning_route","permissions_granted":false}),
                )?;
                continue;
            }
            let route = if let (Some(selection), Some(registration)) =
                (&bound.plan.model_selection, &bound.plan.registered_model)
            {
                self.runtime.for_registered_model(selection, registration)?
            } else if bound.plan.model_selection.is_none() && bound.plan.registered_model.is_none()
            {
                Some(self.runtime.clone())
            } else {
                None
            };
            let Some(route) = route else {
                self.runtime.store.event(
                    self.task,
                    "expert_workflow_skipped",
                    "jev-plus",
                    json!({"expert_id":bound.plan.id,"version":bound.plan.version,"reason":"backend_does_not_support_exact_registered_model_selection","permissions_granted":false}),
                )?;
                continue;
            };
            routed_runtimes.push(route);
            routed_plans.push(bound);
        }
        let mut plans = routed_plans;
        if plans.is_empty() {
            return Ok(None);
        }
        // A live typed receiver means these expert calls are running beside
        // typed execution. Explicit capability inputs scope the extra gate;
        // unscoped capabilities (including dependency-only manifest nodes)
        // conservatively wait for every expected typed operation.
        let mut typed_requirements = self.typed_progress.as_ref().map(|progress| {
            let all = progress
                .borrow()
                .expected_ids()
                .map(str::to_owned)
                .collect::<std::collections::BTreeSet<_>>();
            plans
                .iter()
                .map(|bound| {
                    self.expert_typed_inputs
                        .as_ref()
                        .and_then(|inputs| inputs.get(&bound.capability))
                        .map(|inputs| {
                            inputs
                                .iter()
                                .map(|input| input.operation_id().to_owned())
                                .collect()
                        })
                        .unwrap_or_else(|| all.clone())
                })
                .collect::<Vec<std::collections::BTreeSet<String>>>()
        });
        if dependency_graph && plans.len() != selected_plan_count {
            self.runtime.store.event(self.task, "expert_workflow_skipped", "jev-plus",
                json!({"reason":"complete_dependency_graph_route_unavailable","permissions_granted":false}))?;
            return Ok(None);
        }
        let per_expert_capacity = self.runtime.cfg.subtask_parallelism.unwrap_or(1).min(4);
        let global_capacity = per_expert_capacity
            .max(expert_capacity)
            .min(4)
            .min(self.expert_wave_capacity.unwrap_or(4));
        if global_capacity > 0 && self.network_slots.is_none() {
            self.network_slots = Some(super::super::network_slots::Budget::new(global_capacity)?);
        }
        let empty_done = plans
            .iter()
            .map(|_| std::collections::BTreeSet::<String>::new())
            .collect::<Vec<_>>();
        let empty_results = plans
            .iter()
            .map(|_| serde_json::Map::new())
            .collect::<Vec<_>>();
        let mut first_wave_capacity = global_capacity;
        let mut first_wave = ready_expert_wave(
            &plans,
            &empty_done,
            &empty_results,
            ExpertWaveLimits {
                experts: expert_capacity,
                per_expert: per_expert_capacity,
                total: first_wave_capacity,
            },
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
            None,
        )?;
        while !first_wave.is_empty()
            && !self
                .runtime
                .inference_wave_fits(&expert_wave_routes(&first_wave, &routed_runtimes))?
            && first_wave_capacity > 1
        {
            first_wave_capacity -= 1;
            first_wave = ready_expert_wave(
                &plans,
                &empty_done,
                &empty_results,
                ExpertWaveLimits {
                    experts: expert_capacity,
                    per_expert: per_expert_capacity,
                    total: first_wave_capacity,
                },
                &std::collections::BTreeSet::new(),
                &std::collections::BTreeSet::new(),
                None,
            )?;
        }
        if first_wave.is_empty()
            || !self
                .runtime
                .inference_wave_fits(&expert_wave_routes(&first_wave, &routed_runtimes))?
        {
            self.runtime.store.event(
                self.task,
                "expert_workflow_skipped",
                "jev-plus",
                json!({"expert_ids":plans.iter().map(|bound|bound.plan.id.as_str()).collect::<Vec<_>>(),"reason":"first_exact_expert_route_does_not_fit_resource_admission","permissions_granted":false}),
            )?;
            return Ok(None);
        }
        if !dependency_graph && plans.len() > 1 && first_wave.len() < 2 {
            plans.truncate(1);
            routed_runtimes.truncate(1);
            typed_requirements = typed_requirements.map(|requirements| requirements[..1].to_vec());
            first_wave_capacity = global_capacity;
            first_wave = ready_expert_wave(
                &plans,
                &empty_done[..1],
                &empty_results[..1],
                ExpertWaveLimits {
                    experts: expert_capacity,
                    per_expert: per_expert_capacity,
                    total: first_wave_capacity,
                },
                &std::collections::BTreeSet::new(),
                &std::collections::BTreeSet::new(),
                None,
            )?;
        }
        let mut step_count = plans
            .iter()
            .map(|bound| bound.plan.steps.len())
            .sum::<usize>();
        const STEP_TOKENS: usize = 128;
        const MAX_SUMMARY_ADVICE_BYTES: usize = 1_200;
        const MAX_SUMMARY_CONTENT_BYTES: usize = 600;
        let mut step_advice_budget = MAX_SUMMARY_CONTENT_BYTES / step_count;
        if step_advice_budget < crate::agent::advisory_record::minimum_bytes() {
            self.runtime.store.event(
                self.task,
                "expert_workflow_skipped",
                "jev-plus",
                json!({
                    "reason":"structured_advice_minimum_exceeds_shared_summary_budget",
                    "step_count":step_count,
                    "step_advice_bytes":step_advice_budget,
                    "permissions_granted":false
                }),
            )?;
            return Ok(None);
        }
        let preflight_core_catalog = if step_advice_budget >= 200 {
            self.expert_core_catalog(false)
        } else {
            None
        };
        let mut hypothetical = Vec::new();
        let mut projected_summaries = Vec::new();
        loop {
            hypothetical.clear();
            projected_summaries.clear();
            let mut all_projected_results = Vec::with_capacity(plans.len());
            for bound in &plans {
                let plan = &bound.plan;
                let placeholder = crate::agent::advisory_record::sizing_value(step_advice_budget)?;
                let mut projected_results = serde_json::Map::new();
                for step in &plan.steps {
                    projected_results.insert(
                        step.id.clone(),
                        json!({
                            "advice":placeholder,
                            "partial":false,
                            "trust":"untrusted_model_advisory_never_permissions"
                        }),
                    );
                }
                all_projected_results.push(projected_results);
            }
            for (plan_index, bound) in plans.iter().enumerate() {
                let plan = &bound.plan;
                // Include every possible prerequisite output as well as this
                // expert's own prior steps in the complete prompt-size bound.
                let maximum_prior = expert_prior(&plans, plan_index, &all_projected_results)?;
                // Live typed values may arrive after this cost projection.
                // Each ready call rebuilds and admits its actual complete turn
                // before reservation/preparation in reserve_expert_wave.
                let maximum_prior = self.prior_with_typed_inputs(bound, &maximum_prior)?;
                for step in &plan.steps {
                    let allocation = plan.registered_model.as_ref().map_or(STEP_TOKENS, |model| {
                        STEP_TOKENS.min(model.manifest.max_output_tokens as usize)
                    });
                    hypothetical.push((
                        "solver",
                        self.expert_turn(
                            plan,
                            step,
                            &maximum_prior,
                            allocation,
                            step_advice_budget,
                            (preflight_core_catalog.as_ref(), &[]),
                        ),
                        allocation,
                    ));
                }
                let mut summary = json!({
                    "capability":bound.capability,
                    "expert_id":plan.id,
                    "version":plan.version,
                    "results":all_projected_results[plan_index]
                });
                if dependency_graph {
                    summary["dependencies"] = json!(bound.dependencies);
                }
                projected_summaries.push(summary);
            }
            let projected_summary = if plans.len() == 1 {
                json!({
                    "expert_id":plans[0].plan.id,
                    "version":plans[0].plan.version,
                    "results":projected_summaries[0]["results"],
                    "trust":"untrusted_model_advisory_never_permissions"
                })
            } else {
                json!({
                    "experts":projected_summaries,
                    "trust":"untrusted_model_advisory_never_permissions"
                })
            }
            .to_string();
            let projected_context = if projected_summary.len() <= MAX_SUMMARY_ADVICE_BYTES {
                Some(
                    super::super::expert_advice::Record::new(
                        &projected_summary,
                        &sha256(self.session.original_objective(self.original)?.as_bytes()),
                        &sha256(self.session.state.references.as_bytes()),
                    )?
                    .context()?,
                )
            } else {
                None
            };
            // Reserve the envelope overhead as well as worst-case later working
            // advice. Expert prompts above do not receive their own future result.
            let previous_context = self.expert_advice.take();
            self.expert_advice = projected_context.clone();
            let solver = self.turn("solver", &"\0".repeat(2_000), MIN_SOLVER_TOKENS);
            self.expert_advice = previous_context;
            hypothetical.push(("solver", solver, MIN_SOLVER_TOKENS));
            if projected_context.is_some() && self.available_turns(&hypothetical)? {
                break;
            }
            if !dependency_graph && plans.len() > 1 {
                plans.truncate(1);
                routed_runtimes.truncate(1);
                typed_requirements =
                    typed_requirements.map(|requirements| requirements[..1].to_vec());
                step_count = plans[0].plan.steps.len();
                step_advice_budget = MAX_SUMMARY_CONTENT_BYTES / step_count;
                first_wave = ready_expert_wave(
                    &plans,
                    &empty_done[..1],
                    &empty_results[..1],
                    ExpertWaveLimits {
                        experts: expert_capacity,
                        per_expert: per_expert_capacity,
                        total: first_wave_capacity,
                    },
                    &std::collections::BTreeSet::new(),
                    &std::collections::BTreeSet::new(),
                    None,
                )?;
                continue;
            }
            self.runtime.store.event(
                self.task,
                "expert_workflow_skipped",
                "jev-plus",
                json!({"expert_id":plans[0].plan.id,"version":plans[0].plan.version,"reason":"shared_budget_or_tool_reservation_unavailable","permissions_granted":false}),
            )?;
            return Ok(None);
        }
        // Budget fallback may have changed the plan set and therefore the
        // proposed first wave. Recheck that exact wave before any reservation.
        while first_wave.len() > 1
            && !self
                .runtime
                .inference_wave_fits(&expert_wave_routes(&first_wave, &routed_runtimes))?
        {
            first_wave.pop();
        }
        if first_wave.is_empty()
            || !self
                .runtime
                .inference_wave_fits(&expert_wave_routes(&first_wave, &routed_runtimes))?
        {
            return Ok(None);
        }
        let overlap_capacity = global_capacity.saturating_sub(2).min(first_wave.len());
        let overlap_wave = ready_expert_wave(
            &plans,
            &empty_done[..plans.len()],
            &empty_results[..plans.len()],
            ExpertWaveLimits {
                experts: expert_capacity,
                per_expert: per_expert_capacity,
                total: overlap_capacity,
            },
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
            None,
        )?;
        let overlap = if mode(&self.runtime.cfg, self.original) == "research"
            && self.overlapped_pair.is_none()
            && !overlap_wave.is_empty()
            && self.can_store_roles(prior_advice, &["planner", "explorer"])?
        {
            let mut combined_routes = expert_wave_routes(&overlap_wave, &routed_runtimes);
            combined_routes.extend([
                ("planner", self.backend.clone()),
                ("explorer", self.backend.clone()),
            ]);
            let mut combined = hypothetical.clone();
            // The last hypothetical solver already includes bounded working
            // advice and the projected durable expert-advice envelope.
            for role in ["planner", "explorer"] {
                combined.push((
                    role,
                    self.turn(role, prior_advice, ADVICE_TOKENS),
                    ADVICE_TOKENS,
                ));
            }
            self.available_turns(&combined)?
                && self.runtime.inference_wave_fits(&combined_routes)?
        } else {
            false
        };
        if overlap {
            first_wave = overlap_wave;
        }
        let remaining_tool_slots = self
            .session
            .state
            .tool_limit
            .saturating_sub(self.session.state.proposals + self.session.state.internal_tools)
            .saturating_sub(self.held_typed_checks);
        let client_requires_tool = self
            .original
            .tool_choice
            .as_ref()
            .is_some_and(|value| value == "required" || value.is_object());
        if self
            .session
            .state
            .proposals
            .saturating_add(self.session.state.internal_tools)
            .saturating_add(self.held_typed_checks)
            >= self.session.state.tool_limit
            || (client_requires_tool && remaining_tool_slots <= 1)
            || !self.available_turns(&hypothetical)?
        {
            self.runtime.store.event(
                self.task,
                "expert_workflow_skipped",
                "jev-plus",
                json!({"expert_ids":plans.iter().map(|bound|bound.plan.id.as_str()).collect::<Vec<_>>(),"reason":"shared_budget_or_tool_reservation_unavailable","permissions_granted":false}),
            )?;
            return Ok(None);
        }
        self.session.reserve_internal_tool(
            "jev-plus",
            "expert.workflow.advice",
            json!({
                "bindings":plans.iter().map(|bound|json!({
                    "capability":bound.capability,
                    "expert_id":bound.plan.id,
                    "version":bound.plan.version,
                    "model_selection":bound.plan.model_selection,
                    "dependencies":bound.dependencies,
                    "declared_context_tokens":bound.plan.registered_model.as_ref().map(|model|model.manifest.context_tokens)
                })).collect::<Vec<_>>(),
                "steps":step_count,
                "expert_parallelism":expert_capacity,
                "manifest_pinned_at_session_root":true
            }),
        )?;
        let mut pair = if overlap {
            Some(self.reserve_advisory_pair(prior_advice)?)
        } else {
            None
        };
        let mut results = plans
            .iter()
            .map(|_| serde_json::Map::new())
            .collect::<Vec<_>>();
        let mut done = plans
            .iter()
            .map(|_| std::collections::BTreeSet::<String>::new())
            .collect::<Vec<_>>();
        let mut wave_counts = vec![0usize; plans.len()];
        let mut running = Vec::<RunningWork>::new();
        // These maps live only for the lifetime of this exact workflow. Each
        // source remains an owned RunningWork future; aliases have no future
        // and therefore consume no physical model slot.
        let mut active_core_batches =
            std::collections::BTreeMap::<CoreBatchKey, ActiveCoreBatch>::new();
        let mut core_waiters = std::collections::BTreeMap::<usize, Vec<PendingCore>>::new();
        let mut failure: Option<anyhow::Error> = None;
        let mut pending_pair: Option<PreparedAdvisoryPair> = None;
        let mut advisory_results: [Option<Completion>; 2] = [None, None];
        let mut advisory_finished = 0usize;
        if let Some(pair) = pair.take() {
            // With a live typed producer, compute a fresh gated wave. The
            // preflight wave above only proves budgets; it is never authority
            // to start model work before its typed prerequisites are checked.
            let typed_ready = self.typed_progress.as_ref().map(|progress| {
                let completed = progress
                    .borrow()
                    .completed_ids()
                    .map(str::to_owned)
                    .collect::<std::collections::BTreeSet<_>>();
                typed_requirements
                    .as_ref()
                    .map(|requirements| {
                        requirements
                            .iter()
                            .map(|required| required.is_subset(&completed))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![false; plans.len()])
            });
            let overlap_experts = if typed_ready.is_some() {
                ready_expert_wave(
                    &plans,
                    &empty_done,
                    &empty_results,
                    ExpertWaveLimits {
                        experts: expert_capacity,
                        per_expert: per_expert_capacity,
                        total: global_capacity.saturating_sub(2),
                    },
                    &std::collections::BTreeSet::new(),
                    &std::collections::BTreeSet::new(),
                    typed_ready.as_deref(),
                )?
            } else {
                first_wave.clone()
            };
            let prepared = if overlap_experts.is_empty() {
                Vec::new()
            } else {
                self.reserve_expert_wave(
                    &plans,
                    &routed_runtimes,
                    &overlap_experts,
                    step_advice_budget,
                    false,
                    &[],
                )?
            };
            for plan_index in prepared
                .iter()
                .map(|call| call.plan_index)
                .collect::<std::collections::BTreeSet<_>>()
            {
                wave_counts[plan_index] += 1;
            }
            self.runtime.store.event(
                self.task,
                "agent_overlap_wave",
                "jev-plus",
                json!({
                    "planner_call":pair.first_call,
                    "explorer_call":pair.first_call + 1,
                    "expert_calls":prepared.iter().map(|call|call.call).collect::<Vec<_>>(),
                    "concurrency_limit":global_capacity,
                    "active_calls_bound":prepared.len() + 2,
                    "original_advice_sha256":sha256(prior_advice.as_bytes()),
                    "all_calls_reserved":true,
                    "inputs_independent":true,
                    "permissions_granted":false,
                    "results_verified":false
                }),
            )?;
            running.extend(prepared.into_iter().map(|call| {
                expert_future(
                    call,
                    self.deadline,
                    step_advice_budget,
                    self.network_slots.clone(),
                )
            }));
            running.push(advisory_preparation_future(
                self.runtime.clone(),
                self.backend.clone(),
                self.task.to_owned(),
                self.deadline,
                pair,
                self.network_slots.clone(),
            ));
        }
        while done
            .iter()
            .zip(&plans)
            .any(|(completed, plan)| completed.len() < plan.plan.steps.len())
            || !running.is_empty()
        {
            if let Some(progress) = self.typed_progress.as_mut() {
                let snapshot = progress.borrow_and_update().clone();
                if snapshot.failed() {
                    failure.get_or_insert_with(|| {
                        anyhow::anyhow!("Typed operation producer failed; expert refills stopped")
                    });
                }
            }
            if failure.is_none() {
                let in_flight = running
                    .iter()
                    .filter_map(|work| match work {
                        RunningWork::Expert(coordinate, _) | RunningWork::Core(coordinate, _) => {
                            Some(*coordinate)
                        }
                        RunningWork::AdvisoryPrepare(_) | RunningWork::AdvisoryRole(_, _) => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                let waiting_reuse = core_waiters
                    .values()
                    .flatten()
                    .map(|waiter| waiter.coordinate)
                    .collect::<std::collections::BTreeSet<_>>();
                let advisory_slots = running
                    .iter()
                    .map(|work| match work {
                        RunningWork::AdvisoryPrepare(_) => 2,
                        RunningWork::AdvisoryRole(_, _) => 1,
                        _ => 0,
                    })
                    .sum::<usize>();
                let capacity = global_capacity.saturating_sub(advisory_slots);
                let typed_ready = self.typed_progress.as_ref().map(|progress| {
                    let completed = progress
                        .borrow()
                        .completed_ids()
                        .map(str::to_owned)
                        .collect::<std::collections::BTreeSet<_>>();
                    typed_requirements
                        .as_ref()
                        .map(|requirements| {
                            requirements
                                .iter()
                                .map(|required| required.is_subset(&completed))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_else(|| vec![false; plans.len()])
                });
                let selected = ready_expert_wave(
                    &plans,
                    &done,
                    &results,
                    ExpertWaveLimits {
                        experts: expert_capacity,
                        per_expert: per_expert_capacity,
                        total: capacity,
                    },
                    &in_flight,
                    &waiting_reuse,
                    typed_ready.as_deref(),
                );
                let mut ready = match selected {
                    Ok(ready) => ready,
                    Err(error) => {
                        failure = Some(error);
                        Vec::new()
                    }
                };
                if !ready.is_empty() {
                    let mut fit_error = None;
                    while !ready.is_empty() {
                        match self
                            .runtime
                            .inference_wave_fits(&expert_wave_routes(&ready, &routed_runtimes))
                        {
                            Ok(true) => break,
                            Ok(false) => {}
                            Err(error) => {
                                fit_error = Some(error);
                                break;
                            }
                        }
                        if ready.len() > 1 {
                            ready.pop();
                        } else {
                            break;
                        }
                    }
                    if let Some(error) = fit_error {
                        failure = Some(error);
                        ready.clear();
                    }
                    let fits = if ready.is_empty() {
                        false
                    } else {
                        match self
                            .runtime
                            .inference_wave_fits(&expert_wave_routes(&ready, &routed_runtimes))
                        {
                            Ok(fits) => fits,
                            Err(error) => {
                                failure = Some(error);
                                false
                            }
                        }
                    };
                    if fits {
                        match self.reserve_expert_wave(
                            &plans,
                            &routed_runtimes,
                            &ready,
                            step_advice_budget,
                            !active_core_batches.is_empty(),
                            &reusable_candidates(&active_core_batches),
                        ) {
                            Ok(prepared) => {
                                for plan_index in prepared
                                    .iter()
                                    .map(|call| call.plan_index)
                                    .collect::<std::collections::BTreeSet<_>>()
                                {
                                    wave_counts[plan_index] += 1;
                                }
                                running.extend(prepared.into_iter().map(|call| {
                                    expert_future(
                                        call,
                                        self.deadline,
                                        step_advice_budget,
                                        self.network_slots.clone(),
                                    )
                                }));
                            }
                            Err(error) => failure = Some(error),
                        }
                    } else if running.is_empty() {
                        failure = Some(anyhow::anyhow!(
                            "Ready expert calls do not fit shared resource admission"
                        ));
                    }
                }
            }
            if running.is_empty() {
                if failure.is_some() {
                    break;
                }
                if done
                    .iter()
                    .zip(&plans)
                    .any(|(completed, plan)| completed.len() < plan.plan.steps.len())
                {
                    if let Some(progress) = self.typed_progress.as_mut() {
                        if progress.borrow().terminal() {
                            failure = Some(anyhow::anyhow!(
                                "Typed producer finished without satisfying expert prerequisites"
                            ));
                        } else {
                            match tokio::time::timeout_at(self.deadline.into(), progress.changed()).await {
                                Ok(Ok(())) => {}
                                Ok(Err(_)) => failure = Some(anyhow::anyhow!(
                                    "Typed progress stream closed before expert prerequisites completed"
                                )),
                                Err(_) => failure = Some(anyhow::anyhow!(
                                    "Timed out waiting for typed progress required by expert workflow"
                                )),
                            }
                        }
                        // A changed watch value is re-evaluated at the top of
                        // this loop; no synthetic completion or polling loop.
                    } else {
                        failure = Some(anyhow::anyhow!(
                            "Expert workflow has unfinished steps with no ready or in-flight calls"
                        ));
                    }
                    if failure.is_none() {
                        continue;
                    }
                }
                if failure.is_some()
                    || done
                        .iter()
                        .zip(&plans)
                        .all(|(completed, plan)| completed.len() == plan.plan.steps.len())
                {
                    break;
                }
            }
            let completed = if failure.is_none() {
                if let Some(progress) = self
                    .typed_progress
                    .as_mut()
                    .filter(|receiver| !receiver.borrow().terminal())
                {
                    // The work helper consumes ready futures only on the poll
                    // returning Ready; cancelling its pending wait loses nothing.
                    tokio::select! {
                        completed = ready_expert_work(&mut running) => Some(completed),
                        changed = progress.changed() => {
                            if changed.is_err() {
                                failure = Some(anyhow::anyhow!("Typed progress closed before final status"));
                            }
                            None
                        }
                    }
                } else {
                    Some(ready_expert_work(&mut running).await)
                }
            } else {
                Some(ready_expert_work(&mut running).await)
            };
            let Some(completed) = completed else {
                continue;
            };
            let mut pending_core = Vec::new();
            let mut pending_advisory = Vec::new();
            for work in completed {
                match work {
                    CompletedWork::Expert(report) => {
                        let call_id = report.call.call;
                        match self.record_expert_completion(
                            &plans,
                            *report,
                            &mut results,
                            &mut done,
                        ) {
                            Ok(Some(core)) => pending_core.push(core),
                            Ok(None) => {}
                            Err(error) => {
                                let _ = self.runtime.store.event(self.task, "expert_workflow_step_failed", "jev-plus", json!({"call":call_id,"result_state":"failed_or_unknown","retry_attempted":false,"permissions_granted":false}));
                                if failure.is_none() {
                                    failure = Some(error);
                                }
                            }
                        }
                    }
                    CompletedWork::AdvisoryPrepare(report) => {
                        let (pair, result) = *report;
                        match result {
                            Ok(_) if failure.is_some() => {
                                let _ = self.runtime.store.event(self.task, "agent_overlap_wave_failed", "jev-plus", json!({"result_state":"workflow_stopped_before_advisory_dispatch","retry_attempted":false,"permissions_granted":false}));
                            }
                            Ok(routes) if routes.len() == 2 => {
                                for (index, route) in routes.into_iter().enumerate() {
                                    pending_advisory.push(advisory_role_future(
                                        self.runtime.clone(),
                                        route,
                                        self.task.to_owned(),
                                        self.deadline,
                                        index,
                                        pair.turns[index].clone(),
                                        self.network_slots.clone(),
                                    ));
                                }
                                pending_pair = Some(pair);
                            }
                            Ok(_) => {
                                failure.get_or_insert_with(|| {
                                    anyhow::anyhow!(
                                        "Advisory preparation returned an invalid route count"
                                    )
                                });
                            }
                            Err(error) => {
                                let _ = self.runtime.store.event(self.task, "agent_overlap_wave_failed", "jev-plus", json!({"result_state":"failed_or_unknown","retry_attempted":false,"permissions_granted":false}));
                                failure.get_or_insert(error);
                            }
                        }
                    }
                    CompletedWork::AdvisoryRole(index, result) => {
                        advisory_finished += 1;
                        let recorded = match pending_pair.as_ref() {
                            Some(pair) => self.record_advisory_role(pair, index, result),
                            None => Err(anyhow::anyhow!(
                                "Advisory role completed without its reserved pair"
                            )),
                        };
                        match recorded {
                            Ok(completion)
                                if index < advisory_results.len()
                                    && advisory_results[index].is_none() =>
                            {
                                advisory_results[index] = Some(completion)
                            }
                            Ok(_) => {
                                failure.get_or_insert_with(|| {
                                    anyhow::anyhow!("Duplicate advisory role completion")
                                });
                            }
                            Err(error) => {
                                let _ = self.runtime.store.event(self.task, "agent_overlap_wave_failed", "jev-plus", json!({"result_state":"failed_or_unknown","retry_attempted":false,"permissions_granted":false}));
                                failure.get_or_insert(error);
                            }
                        }
                        if advisory_finished == 2 {
                            if let (Some(planner), Some(explorer)) =
                                (advisory_results[0].take(), advisory_results[1].take())
                            {
                                self.overlapped_pair = Some((planner, explorer));
                            }
                            pending_pair = None;
                        }
                    }
                    CompletedWork::Core(report) => {
                        let call_id = report.call;
                        let source_key = active_core_batches.iter().find_map(|(key, source)| {
                            (source.call == call_id).then(|| key.clone())
                        });
                        let source_completed = self.record_expert_core_completion(
                            &plans,
                            *report,
                            &mut results,
                            &mut done,
                        );
                        if let Some(key) = source_key {
                            active_core_batches.remove(&key);
                        }
                        if let Err(error) = source_completed {
                            let _ = self.runtime.store.event(self.task, "expert_workflow_step_failed", "jev-plus", json!({"call":call_id,"result_state":"failed_or_unknown","retry_attempted":false,"permissions_granted":false}));
                            if let Some(waiters) = core_waiters.remove(&call_id) {
                                for waiter in waiters {
                                    let _ = self.runtime.store.event(self.task, "expert_workflow_step_failed", "jev-plus", json!({"call":waiter.call,"coalesced_source_call":call_id,"result_state":"source_failed_or_unknown","retry_attempted":false,"permissions_granted":false}));
                                }
                            }
                            if failure.is_none() {
                                failure = Some(error);
                            }
                        } else if failure.is_none() {
                            if let Some(waiters) = core_waiters.remove(&call_id) {
                                for waiter in waiters {
                                    match self.session.reuse_expert_core_work(
                                        self.original,
                                        &waiter.proposals,
                                        &waiter.caller,
                                    ) {
                                        Ok(Some(reused)) => {
                                            if let Err(error) = self.record_reused_expert_core(
                                                &plans,
                                                waiter,
                                                reused,
                                                Some(call_id),
                                                &mut results,
                                                &mut done,
                                            ) {
                                                failure = Some(error);
                                                break;
                                            }
                                        }
                                        Ok(None) => {
                                            failure = Some(anyhow::anyhow!("Committed source core batch was not reusable for its validated waiter"));
                                            break;
                                        }
                                        Err(error) => {
                                            failure = Some(error);
                                            break;
                                        }
                                    }
                                }
                            }
                        } else if let Some(waiters) = core_waiters.remove(&call_id) {
                            for waiter in waiters {
                                let _ = self.runtime.store.event(self.task, "expert_workflow_step_failed", "jev-plus", json!({"call":waiter.call,"coalesced_source_call":call_id,"result_state":"workflow_stopped_before_reuse","retry_attempted":false,"permissions_granted":false}));
                            }
                        }
                    }
                }
            }
            if !pending_advisory.is_empty() {
                if failure.is_none() {
                    running.extend(pending_advisory);
                } else {
                    pending_pair = None;
                    let _ = self.runtime.store.event(self.task, "agent_overlap_wave_failed", "jev-plus", json!({"result_state":"workflow_stopped_before_advisory_dispatch","retry_attempted":false,"permissions_granted":false}));
                }
            }
            if failure.is_none()
                && self
                    .typed_progress
                    .as_ref()
                    .is_some_and(|progress| progress.borrow().failed())
            {
                failure = Some(anyhow::anyhow!(
                    "Typed operation producer failed; expert core admissions stopped"
                ));
            }
            if failure.is_none() {
                // Validate every eligible alias before any cache hit consumes
                // its call namespace or any new core work is admitted.
                for pending in &pending_core {
                    if independent_core_key(pending).is_some() {
                        if let Err(error) = self.session.validate_expert_core_reuse(
                            self.original,
                            &pending.proposals,
                            &pending.caller,
                        ) {
                            failure = Some(error);
                            break;
                        }
                    }
                }
            }
            if failure.is_none() {
                let mut unique_ready =
                    Vec::<(PendingCore, Option<CoreBatchKey>, Vec<PendingCore>)>::new();
                let mut same_ready = std::collections::BTreeMap::<CoreBatchKey, usize>::new();
                let mut all_fingerprints = std::collections::BTreeSet::new();
                let mut overlaps_different_batches = false;
                for pending in pending_core {
                    let key = independent_core_key(&pending);
                    if key.as_ref().is_some_and(|key| same_ready.contains_key(key)) {
                        let owner = same_ready[key.as_ref().expect("checked key")];
                        unique_ready[owner].2.push(pending);
                        continue;
                    }
                    if pending
                        .proposals
                        .iter()
                        .any(|proposal| !all_fingerprints.insert(proposal.fingerprint.clone()))
                    {
                        overlaps_different_batches = true;
                    }
                    let owner_index = unique_ready.len();
                    if let Some(key) = &key {
                        same_ready.insert(key.clone(), owner_index);
                    }
                    unique_ready.push((pending, key, Vec::new()));
                }
                if overlaps_different_batches {
                    failure = Some(anyhow::anyhow!("Ready expert core batches overlap computations outside an identical eligible batch"));
                }
                let mut scheduled =
                    Vec::<(PendingCore, Option<CoreBatchKey>, Vec<PendingCore>)>::new();
                if failure.is_none() {
                    for (pending, key, aliases) in unique_ready {
                        if let Some(key) = key {
                            match self.session.reuse_expert_core_work(
                                self.original,
                                &pending.proposals,
                                &pending.caller,
                            ) {
                                Ok(Some(reused)) => {
                                    if let Err(error) = self.record_reused_expert_core(
                                        &plans,
                                        pending,
                                        reused,
                                        None,
                                        &mut results,
                                        &mut done,
                                    ) {
                                        failure = Some(error);
                                        break;
                                    }
                                    for alias in aliases {
                                        match self.session.reuse_expert_core_work(
                                            self.original,
                                            &alias.proposals,
                                            &alias.caller,
                                        ) {
                                            Ok(Some(reused)) => {
                                                if let Err(error) = self.record_reused_expert_core(
                                                    &plans,
                                                    alias,
                                                    reused,
                                                    None,
                                                    &mut results,
                                                    &mut done,
                                                ) {
                                                    failure = Some(error);
                                                    break;
                                                }
                                            }
                                            Ok(None) => {
                                                failure = Some(anyhow::anyhow!("Checked core batch disappeared during same-ready reuse"));
                                                break;
                                            }
                                            Err(error) => {
                                                failure = Some(error);
                                                break;
                                            }
                                        }
                                    }
                                    if failure.is_some() {
                                        break;
                                    }
                                }
                                Ok(None) => {
                                    if let Some(source_call) =
                                        active_core_batches.get(&key).map(|source| source.call)
                                    {
                                        let waiters = core_waiters.entry(source_call).or_default();
                                        if waiters.len().saturating_add(1 + aliases.len())
                                            > self.session.state.call_limit
                                        {
                                            failure = Some(anyhow::anyhow!("Identical in-flight expert core aliases exceed the workflow call bound"));
                                            break;
                                        }
                                        let _ = self.runtime.store.event(
                                            self.task,
                                            "expert_core_batch_coalesced",
                                            "jev-plus",
                                            json!({"source_call":source_call,"coalesced_call":pending.call,"operation_count":pending.proposals.len(),"permissions_granted":false}),
                                        );
                                        waiters.push(pending);
                                        for alias in aliases {
                                            let _ = self.runtime.store.event(
                                                self.task,
                                                "expert_core_batch_coalesced",
                                                "jev-plus",
                                                json!({"source_call":source_call,"coalesced_call":alias.call,"operation_count":alias.proposals.len(),"permissions_granted":false}),
                                            );
                                            waiters.push(alias);
                                        }
                                    } else {
                                        if aliases.len() > self.session.state.call_limit {
                                            failure = Some(anyhow::anyhow!("Identical ready expert core aliases exceed the workflow call bound"));
                                            break;
                                        }
                                        scheduled.push((pending, Some(key), aliases));
                                    }
                                }
                                Err(error) => {
                                    failure = Some(error);
                                    break;
                                }
                            }
                        } else {
                            scheduled.push((pending, None, aliases));
                        }
                    }
                }
                let requested = scheduled
                    .iter()
                    .map(|(pending, _, _)| pending.proposals.len() as u32 * 2)
                    .sum::<u32>();
                let available = self.session.state.tool_limit.saturating_sub(
                    self.session
                        .state
                        .proposals
                        .saturating_add(self.session.state.internal_tools)
                        .saturating_add(self.held_typed_checks),
                );
                if failure.is_none() && requested > available {
                    failure = Some(anyhow::anyhow!(
                        "Ready expert core batches exceed shared tool capacity"
                    ));
                }
                if failure.is_none() {
                    for (pending, key, aliases) in scheduled {
                        let owner_call = pending.call;
                        let candidates = pending.proposals.iter().map(|proposal| json!({"name":proposal.name,"arguments":proposal.arguments})).collect();
                        match self.schedule_expert_core(pending) {
                            Ok(core) => {
                                if let Some(key) = key {
                                    active_core_batches.insert(
                                        key,
                                        ActiveCoreBatch {
                                            call: owner_call,
                                            candidates,
                                        },
                                    );
                                }
                                if !aliases.is_empty() {
                                    for alias in &aliases {
                                        let _ = self.runtime.store.event(
                                            self.task,
                                            "expert_core_batch_coalesced",
                                            "jev-plus",
                                            json!({"source_call":owner_call,"coalesced_call":alias.call,"operation_count":alias.proposals.len(),"permissions_granted":false}),
                                        );
                                    }
                                    core_waiters.entry(owner_call).or_default().extend(aliases);
                                }
                                running.push(core)
                            }
                            Err(error) => {
                                failure = Some(error);
                                break;
                            }
                        }
                    }
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        let summaries = plans
            .iter()
            .enumerate()
            .map(|(index, bound)| {
                let mut summary = json!({
                    "capability":bound.capability,
                    "expert_id":bound.plan.id,
                    "version":bound.plan.version,
                    "results":results[index]
                });
                if dependency_graph {
                    summary["dependencies"] = json!(bound.dependencies);
                }
                summary
            })
            .collect::<Vec<_>>();
        let summary = if summaries.len() == 1 {
            json!({
                "expert_id":plans[0].plan.id,
                "version":plans[0].plan.version,
                "results":results[0],
                "trust":"untrusted_model_advisory_never_permissions"
            })
            .to_string()
        } else {
            json!({"experts":summaries,"trust":"untrusted_model_advisory_never_permissions"})
                .to_string()
        };
        if summary.len() > MAX_SUMMARY_ADVICE_BYTES {
            bail!("Pinned expert workflow result exceeds its preflighted complete-record bound")
        }
        self.runtime.store.event(
            self.task,
            "expert_workflow_completed",
            "jev-plus",
            json!({
                "expert_ids":plans.iter().map(|bound|bound.plan.id.as_str()).collect::<Vec<_>>(),
                "steps":step_count,
                "wave_counts":wave_counts,
                "result_sha256":sha256(summary.as_bytes()),
                "permissions_granted":false,
                "client_tools_executed":false
            }),
        )?;
        Ok(Some(summary))
    }
}
