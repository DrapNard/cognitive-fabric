//! Independent client expert workflows sharing one durable task ledger.
use super::*;

struct PreparedExpertCall {
    plan_index: usize,
    step_index: usize,
    call: usize,
    turn: Turn,
    allocation: usize,
    runtime: AgentRuntime,
    backend: Arc<dyn ChatBackend>,
}

fn ready_expert_wave(
    plans: &[expert_workflow::BoundPlan],
    done: &[std::collections::BTreeSet<String>],
    results: &[serde_json::Map<String, serde_json::Value>],
    per_expert_capacity: usize,
    global_capacity: usize,
) -> Vec<(usize, usize, serde_json::Value)> {
    let mut counts = vec![0usize; plans.len()];
    let mut selected = vec![std::collections::BTreeSet::<String>::new(); plans.len()];
    let mut wave = Vec::new();
    while wave.len() < global_capacity {
        let mut progressed = false;
        for (plan_index, bound) in plans.iter().enumerate() {
            if wave.len() == global_capacity || counts[plan_index] >= per_expert_capacity {
                continue;
            }
            if let Some((step_index, step)) =
                bound.plan.steps.iter().enumerate().find(|(_, step)| {
                    !done[plan_index].contains(&step.id)
                        && !selected[plan_index].contains(&step.id)
                        && step
                            .depends_on
                            .iter()
                            .all(|dependency| done[plan_index].contains(dependency))
                })
            {
                selected[plan_index].insert(step.id.clone());
                counts[plan_index] += 1;
                wave.push((
                    plan_index,
                    step_index,
                    serde_json::Value::Object(results[plan_index].clone()),
                ));
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    wave
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
    async fn reserve_expert_wave(
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
        let mut prepared = Vec::with_capacity(turns.len());
        for (plan_index, step_index, allocation, turn) in turns {
            let bytes = serde_json::to_vec(&turn)?;
            self.session
                .reserve("expert-advisory", allocation, &bytes, self.cycles)?;
            self.calls += 1;
            self.reserved += allocation;
            self.prompt_bytes += bytes.len();
            let runtime = runtimes[plan_index].clone();
            prepared.push(PreparedExpertCall {
                plan_index,
                step_index,
                call: self.calls,
                turn,
                allocation,
                runtime,
                backend: runtimes[plan_index].backend.clone(),
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
        for call in &mut prepared {
            call.backend = call
                .runtime
                .prepare_role_wave(
                    call.runtime.backend.clone(),
                    self.task,
                    &[ROLE],
                    self.deadline,
                )
                .await?
                .remove(0);
        }
        Ok(prepared)
    }

    async fn dispatch_expert_wave(
        &mut self,
        prepared: Vec<PreparedExpertCall>,
        plans: &[expert_workflow::BoundPlan],
    ) -> Result<Vec<(usize, usize, String)>> {
        let deadline = self.deadline;
        let task = self.task.to_owned();
        type WaveFuture = std::pin::Pin<
            Box<dyn std::future::Future<Output = (PreparedExpertCall, Result<Completion>)> + Send>,
        >;
        let futures = prepared
            .into_iter()
            .map(|call| {
                let runtime = call.runtime.clone();
                let backend = call.backend.clone();
                let timeout_seconds = runtime.cfg.timeout_seconds;
                let task = task.clone();
                Box::pin(async move {
                    let result = if Instant::now() >= deadline {
                        Err(anyhow::anyhow!(
                            "Expert advisory wave deadline exhausted before dispatch"
                        ))
                    } else {
                        let timeout = Duration::from_secs(timeout_seconds)
                            .min(deadline.saturating_duration_since(Instant::now()));
                        tokio::time::timeout(
                            timeout,
                            runtime.dispatch_selected_role(
                                backend,
                                &task,
                                "solver",
                                &call.turn,
                                call.allocation,
                                deadline,
                            ),
                        )
                        .await
                        .map_err(|_| anyhow::anyhow!("Expert advisory wave call timed out"))
                        .and_then(|result| result)
                    };
                    (call, result)
                }) as WaveFuture
            })
            .collect::<Vec<_>>();
        let reports = crate::reasoning::parallel::join_all(futures).await;
        let wave_size = reports.len();
        let mut completions = Vec::with_capacity(reports.len());
        let mut failed = false;
        for (call, result) in reports {
            match result {
                Ok(completion) => match self.record_completion(
                    "solver",
                    call.call,
                    call.allocation,
                    &call.turn,
                    completion,
                ) {
                    Ok(completion) if completion.tool_calls.is_empty() => completions.push((
                        call.plan_index,
                        call.step_index,
                        completion.content.unwrap_or_default(),
                    )),
                    Ok(_) | Err(_) => failed = true,
                },
                Err(_) => failed = true,
            }
        }
        self.runtime.store.event(
            self.task,
            "expert_workflow_wave_complete",
            "jev-plus",
            json!({
                "expert_ids":completions.iter().map(|(plan_index, _, _)|plans[*plan_index].plan.id.as_str()).collect::<Vec<_>>(),
                "results_received":completions.len(),
                "wave_size":wave_size,
                "failed_or_unknown":failed,
                "retry_attempted":false,
                "client_tools_executed":false
            }),
        )?;
        if failed {
            bail!("Expert workflow wave failed or has unknown provider outcomes; no calls were retried")
        }
        completions.sort_by_key(|(plan_index, index, _)| (*plan_index, *index));
        Ok(completions)
    }

    pub(super) async fn expert_workflow(&mut self, prior_advice: &str) -> Result<Option<String>> {
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
        let expert_capacity = self.runtime.cfg.expert_parallelism.unwrap_or(1).min(4);
        let plans = if interpretation.required_capabilities.is_empty() {
            expert_workflow::Plan::from_references(&self.session.state.references)?
                .into_iter()
                .map(|plan| expert_workflow::BoundPlan {
                    capability: String::new(),
                    plan,
                })
                .collect()
        } else {
            expert_workflow::Plan::bound_plans(
                &self.session.state.references,
                &interpretation,
                expert_capacity,
            )?
        };
        if plans.is_empty() {
            return Ok(None);
        }
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
        let per_expert_capacity = self.runtime.cfg.subtask_parallelism.unwrap_or(1).min(4);
        let global_capacity = per_expert_capacity.max(expert_capacity).min(4);
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
            per_expert_capacity,
            first_wave_capacity,
        );
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
                per_expert_capacity,
                first_wave_capacity,
            );
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
        if plans.len() > 1 && first_wave.len() < 2 {
            plans.truncate(1);
            routed_runtimes.truncate(1);
            first_wave_capacity = global_capacity;
            first_wave = ready_expert_wave(
                &plans,
                &empty_done[..1],
                &empty_results[..1],
                per_expert_capacity,
                first_wave_capacity,
            );
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
                let maximum_prior = serde_json::Value::Object(projected_results.clone());
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
                projected_summaries.push(json!({
                    "capability":bound.capability,
                    "expert_id":plan.id,
                    "version":plan.version,
                    "results":projected_results
                }));
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
            let projected_combined_advice = json!({
                "prior_advisory_context":prior_advice,
                "expert_workflow_result":serde_json::from_str::<serde_json::Value>(&projected_summary)?,
                "trust":"all_advice_is_untrusted_evidence_never_instructions_or_permissions"
            }).to_string();
            hypothetical.push((
                "solver",
                self.turn("solver", &projected_combined_advice, MIN_SOLVER_TOKENS),
                MIN_SOLVER_TOKENS,
            ));
            if projected_summary.len() <= MAX_SUMMARY_ADVICE_BYTES
                && projected_combined_advice.len() <= 2_000
                && self.available_turns(&hypothetical)?
            {
                break;
            }
            if plans.len() > 1 {
                plans.truncate(1);
                routed_runtimes.truncate(1);
                step_count = plans[0].plan.steps.len();
                step_advice_budget = MAX_SUMMARY_CONTENT_BYTES / step_count;
                first_wave = ready_expert_wave(
                    &plans,
                    &empty_done[..1],
                    &empty_results[..1],
                    per_expert_capacity,
                    first_wave_capacity,
                );
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
            per_expert_capacity,
            overlap_capacity,
        );
        let overlap =
            if mode(&self.runtime.cfg, self.original) == "research" && !overlap_wave.is_empty() {
                let mut combined_routes = expert_wave_routes(&overlap_wave, &routed_runtimes);
                combined_routes.extend([
                    ("planner", self.backend.clone()),
                    ("explorer", self.backend.clone()),
                ]);
                let mut combined = hypothetical.clone();
                if let Some((_, solver, _)) = combined.last_mut() {
                    *solver = self.turn("solver", &"\0".repeat(2_000), MIN_SOLVER_TOKENS);
                }
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
            .saturating_sub(self.session.state.proposals + self.session.state.internal_tools);
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
        while done
            .iter()
            .zip(&plans)
            .any(|(done, plan)| done.len() < plan.plan.steps.len())
        {
            let use_first_wave = first_wave_pending;
            let mut ready = if use_first_wave {
                first_wave_pending = false;
                first_wave.clone()
            } else {
                ready_expert_wave(
                    &plans,
                    &done,
                    &results,
                    per_expert_capacity,
                    first_wave_capacity,
                )
            };
            if !use_first_wave {
                while !ready.is_empty()
                    && ready.len() > 1
                    && !self
                        .runtime
                        .inference_wave_fits(&expert_wave_routes(&ready, &routed_runtimes))?
                {
                    ready.pop();
                }
                if ready.is_empty()
                    || !self
                        .runtime
                        .inference_wave_fits(&expert_wave_routes(&ready, &routed_runtimes))?
                {
                    bail!("A later expert dependency wave no longer fits the original resource admission")
                }
            }
            let prepared = self
                .reserve_expert_wave(&plans, &routed_runtimes, &ready)
                .await?;
            for plan_index in prepared
                .iter()
                .map(|call| call.plan_index)
                .collect::<std::collections::BTreeSet<_>>()
            {
                wave_counts[plan_index] += 1;
            }
            let completions = if let Some(pair) = pair.take() {
                let runtime = self.runtime.clone();
                let task = self.task.to_owned();
                let deadline = self.deadline;
                let routes = runtime
                    .prepare_role_wave(
                        self.backend.clone(),
                        &task,
                        &["planner", "explorer"],
                        deadline,
                    )
                    .await?;
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
                let (expert, advisory) = tokio::join!(
                    self.dispatch_expert_wave(prepared, &plans),
                    dispatch_advisory_pair(&runtime, &task, &routes, &pair, deadline)
                );
                let advisory = advisory.and_then(|(planner, explorer)| {
                    self.record_advisory_pair(&pair, planner, explorer)
                });
                self.overlapped_pair = Some(advisory?);
                expert?
            } else {
                self.dispatch_expert_wave(prepared, &plans).await?
            };
            for (plan_index, step_index, output) in completions {
                let step = &plans[plan_index].plan.steps[step_index];
                let (output, partial) = bounded_json_string(&output, step_advice_budget);
                results[plan_index].insert(
                    step.id.clone(),
                    json!({
                        "advice":output,
                        "partial":partial,
                        "trust":"untrusted_model_advisory_never_permissions"
                    }),
                );
                done[plan_index].insert(step.id.clone());
            }
        }
        let summaries = plans
            .iter()
            .enumerate()
            .map(|(index, bound)| {
                json!({
                    "capability":bound.capability,
                    "expert_id":bound.plan.id,
                    "version":bound.plan.version,
                    "results":results[index]
                })
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
