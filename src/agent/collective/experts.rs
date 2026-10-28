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
}

type AdvisoryOutcome = Result<(Result<Completion>, Result<Completion>)>;

struct ExpertReport {
    call: PreparedExpertCall,
    advice_budget: usize,
    result: Result<Completion>,
}

enum RunningWork {
    Expert(
        (usize, usize),
        std::pin::Pin<Box<dyn std::future::Future<Output = ExpertReport> + Send>>,
    ),
    Advisory(
        std::pin::Pin<
            Box<dyn std::future::Future<Output = (PreparedAdvisoryPair, AdvisoryOutcome)> + Send>,
        >,
    ),
}

fn expert_future(call: PreparedExpertCall, deadline: Instant, advice_budget: usize) -> RunningWork {
    let coordinate = (call.plan_index, call.step_index);
    RunningWork::Expert(
        coordinate,
        Box::pin(async move {
            let runtime = call.runtime.clone();
            let task = call.task.clone();
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
            ExpertReport {
                call,
                advice_budget,
                result,
            }
        }),
    )
}

fn advisory_future(
    runtime: AgentRuntime,
    backend: Arc<dyn ChatBackend>,
    task: String,
    deadline: Instant,
    pair: PreparedAdvisoryPair,
) -> RunningWork {
    RunningWork::Advisory(Box::pin(async move {
        let result = async {
            let routes = runtime
                .prepare_role_wave(backend, &task, &["planner", "explorer"], deadline)
                .await?;
            dispatch_advisory_pair(&runtime, &task, &routes, &pair, deadline).await
        }
        .await;
        (pair, result)
    }))
}

enum CompletedWork {
    Expert(Box<ExpertReport>),
    Advisory(Box<(PreparedAdvisoryPair, AdvisoryOutcome)>),
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
                RunningWork::Advisory(future) => {
                    if let Poll::Ready((pair, result)) = future.as_mut().poll(context) {
                        ready.push((index, CompletedWork::Advisory(Box::new((pair, result)))));
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

fn ready_expert_wave(
    plans: &[expert_workflow::BoundPlan],
    done: &[std::collections::BTreeSet<String>],
    results: &[serde_json::Map<String, serde_json::Value>],
    expert_capacity: usize,
    per_expert_capacity: usize,
    global_capacity: usize,
    in_flight: &std::collections::BTreeSet<(usize, usize)>,
) -> Result<Vec<(usize, usize, serde_json::Value)>> {
    use super::super::graph_schedule::{self, Dependency, Evidence, Node};
    // Optional overlap probes may have no remaining slots. They select no work,
    // rather than submitting a zero-width wave to the shared scheduler.
    if global_capacity == 0 {
        return Ok(Vec::new());
    }
    if done.len() != plans.len() || results.len() != plans.len() {
        bail!("Expert graph completion state does not match its plans")
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
                in_flight: in_flight.contains(&(plan_index, step_index)),
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
    fn reserve_expert_wave(
        &mut self,
        plans: &[expert_workflow::BoundPlan],
        runtimes: &[AgentRuntime],
        ready: &[(usize, usize, serde_json::Value)],
    ) -> Result<Vec<PreparedExpertCall>> {
        const ROLE: &str = "solver";
        if ready.is_empty() || ready.len() > 4 {
            bail!("Client expert wave is outside its concurrency bound")
        }
        let turns = ready
            .iter()
            .map(|(plan_index, step_index, prior)| {
                let plan = &plans[*plan_index].plan;
                let allocation = plan.registered_model.as_ref().map_or(128, |model| {
                    128.min(model.manifest.max_output_tokens as usize)
                });
                (
                    *plan_index,
                    *step_index,
                    allocation,
                    self.expert_turn(plan, &plan.steps[*step_index], prior, allocation),
                )
            })
            .collect::<Vec<_>>();
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
    ) -> Result<()> {
        let call = report.call;
        let step_id = plans[call.plan_index].plan.steps[call.step_index]
            .id
            .clone();
        let completion = report.result?;
        let outcome = {
            let completion = self.record_completion(
                "solver",
                call.call,
                call.allocation,
                &call.turn,
                completion,
            )?;
            if !completion.tool_calls.is_empty() {
                bail!("Expert advisory role proposed a client tool")
            }
            let content = completion.content.unwrap_or_default();
            let (advice, partial) = bounded_json_string(&content, report.advice_budget);
            results[call.plan_index].insert(
                step_id.clone(),
                json!({
                    "advice":advice,
                    "partial":partial,
                    "trust":"untrusted_model_advisory_never_permissions"
                }),
            );
            done[call.plan_index].insert(step_id.clone());
            "completed"
        };
        self.runtime.store.event(
            self.task,
            "expert_workflow_step_complete",
            "jev-plus",
            json!({
                "expert_id":plans[call.plan_index].plan.id,
                "step_id":step_id,
                "call":call.call,
                "result_state":outcome,
                "permissions_granted":false
            }),
        )?;
        Ok(())
    }

    fn record_advisory_completion(
        &mut self,
        pair: PreparedAdvisoryPair,
        result: AdvisoryOutcome,
    ) -> Result<()> {
        let (planner, explorer) = result?;
        let recorded = self.record_advisory_pair(&pair, planner, explorer)?;
        self.overlapped_pair = Some(recorded);
        Ok(())
    }

    pub(super) async fn expert_workflow(&mut self, prior_advice: &str) -> Result<Option<String>> {
        if self
            .expert_wave_capacity
            .is_some_and(|capacity| !(1..=4).contains(&capacity))
        {
            bail!("Expert role wave capacity is outside its shared task bound")
        }
        let parsed_advice: serde_json::Value =
            serde_json::from_str(prior_advice).unwrap_or_else(|_| serde_json::json!({}));
        let interpretation = if let Some(value) = parsed_advice.get("interpretation") {
            crate::language::Interpretation::parse(&value.to_string())?
        } else {
            crate::language::Interpretation {
                objective: "Original user request".into(),
                constraints: Vec::new(),
                required_capabilities: Vec::new(),
                capability_versions: std::collections::BTreeMap::new(),
                clarification: None,
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
            expert_capacity,
            per_expert_capacity,
            first_wave_capacity,
            &std::collections::BTreeSet::new(),
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
                expert_capacity,
                per_expert_capacity,
                first_wave_capacity,
                &std::collections::BTreeSet::new(),
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
            first_wave_capacity = global_capacity;
            first_wave = ready_expert_wave(
                &plans,
                &empty_done[..1],
                &empty_results[..1],
                expert_capacity,
                per_expert_capacity,
                first_wave_capacity,
                &std::collections::BTreeSet::new(),
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
        let mut hypothetical = Vec::new();
        let mut projected_summaries = Vec::new();
        loop {
            hypothetical.clear();
            projected_summaries.clear();
            let mut all_projected_results = Vec::with_capacity(plans.len());
            for bound in &plans {
                let plan = &bound.plan;
                // Quotes maximize escaping when scalar advice is embedded in
                // JSON and then again in a chat request. Plain ASCII would
                // underestimate the same bounded advice after nested encoding.
                let quote_count = step_advice_budget.saturating_sub(2) / 2;
                let used = 2 + quote_count * 2;
                let placeholder = format!(
                    "{}{}",
                    "\"".repeat(quote_count),
                    "x".repeat(step_advice_budget.saturating_sub(used) % 2)
                );
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
                for step in &plan.steps {
                    let allocation = plan.registered_model.as_ref().map_or(STEP_TOKENS, |model| {
                        STEP_TOKENS.min(model.manifest.max_output_tokens as usize)
                    });
                    hypothetical.push((
                        "solver",
                        self.expert_turn(plan, step, &maximum_prior, allocation),
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
                step_count = plans[0].plan.steps.len();
                step_advice_budget = MAX_SUMMARY_CONTENT_BYTES / step_count;
                first_wave = ready_expert_wave(
                    &plans,
                    &empty_done[..1],
                    &empty_results[..1],
                    expert_capacity,
                    per_expert_capacity,
                    first_wave_capacity,
                    &std::collections::BTreeSet::new(),
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
            expert_capacity,
            per_expert_capacity,
            overlap_capacity,
            &std::collections::BTreeSet::new(),
        )?;
        let overlap = if mode(&self.runtime.cfg, self.original) == "research"
            && self.overlapped_pair.is_none()
            && !overlap_wave.is_empty()
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
        let mut first_wave_pending = true;
        let mut wave_counts = vec![0usize; plans.len()];
        let mut running = Vec::<RunningWork>::new();
        let mut failure: Option<anyhow::Error> = None;
        let mut pair_active = false;
        if let Some(pair) = pair.take() {
            let prepared = self.reserve_expert_wave(&plans, &routed_runtimes, &first_wave)?;
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
            running.extend(
                prepared
                    .into_iter()
                    .map(|call| expert_future(call, self.deadline, step_advice_budget)),
            );
            running.push(advisory_future(
                self.runtime.clone(),
                self.backend.clone(),
                self.task.to_owned(),
                self.deadline,
                pair,
            ));
            pair_active = true;
            first_wave_pending = false;
        }
        while done
            .iter()
            .zip(&plans)
            .any(|(completed, plan)| completed.len() < plan.plan.steps.len())
            || !running.is_empty()
        {
            if failure.is_none() {
                let in_flight = running
                    .iter()
                    .filter_map(|work| match work {
                        RunningWork::Expert(coordinate, _) => Some(*coordinate),
                        RunningWork::Advisory(_) => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                let capacity = global_capacity.saturating_sub(if pair_active { 2 } else { 0 });
                let selected = if first_wave_pending {
                    first_wave_pending = false;
                    Ok(first_wave.clone())
                } else {
                    ready_expert_wave(
                        &plans,
                        &done,
                        &results,
                        expert_capacity,
                        per_expert_capacity,
                        capacity,
                        &in_flight,
                    )
                };
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
                        match self.reserve_expert_wave(&plans, &routed_runtimes, &ready) {
                            Ok(prepared) => {
                                for plan_index in prepared
                                    .iter()
                                    .map(|call| call.plan_index)
                                    .collect::<std::collections::BTreeSet<_>>()
                                {
                                    wave_counts[plan_index] += 1;
                                }
                                running.extend(prepared.into_iter().map(|call| {
                                    expert_future(call, self.deadline, step_advice_budget)
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
                    failure = Some(anyhow::anyhow!(
                        "Expert workflow has unfinished steps with no ready or in-flight calls"
                    ));
                }
                break;
            }
            let completed = ready_expert_work(&mut running).await;
            for work in completed {
                match work {
                    CompletedWork::Expert(report) => {
                        let call_id = report.call.call;
                        if let Err(error) =
                            self.record_expert_completion(&plans, *report, &mut results, &mut done)
                        {
                            let _ = self.runtime.store.event(self.task, "expert_workflow_step_failed", "jev-plus", json!({"call":call_id,"result_state":"failed_or_unknown","retry_attempted":false,"permissions_granted":false}));
                            if failure.is_none() {
                                failure = Some(error);
                            }
                        }
                    }
                    CompletedWork::Advisory(report) => {
                        let (pair, result) = *report;
                        pair_active = false;
                        if let Err(error) = self.record_advisory_completion(pair, result) {
                            let _ = self.runtime.store.event(self.task, "agent_overlap_wave_failed", "jev-plus", json!({"result_state":"failed_or_unknown","retry_attempted":false,"permissions_granted":false}));
                            if failure.is_none() {
                                failure = Some(error);
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
