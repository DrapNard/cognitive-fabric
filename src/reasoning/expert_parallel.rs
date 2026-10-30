//! Bounded execution for an explicitly admitted set of independent experts.
use super::{InferenceTimeout, Node, Session};
use crate::backend::{Generation, InferenceBackend};
use anyhow::{anyhow, Result};
use serde_json::json;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) struct CandidateCall {
    pub role: String,
    pub expert_id: String,
    pub expert_version: String,
    pub capabilities: Vec<String>,
    pub call: u32,
    pub allocation: usize,
    pub prompt: String,
    pub backend: Arc<dyn InferenceBackend>,
    pub overlap_id: Option<String>,
}

type CandidateFuture =
    Pin<Box<dyn Future<Output = (CandidateCall, Result<Generation>, f64)> + Send>>;
type RecordedCandidate = (u32, String, String, String);
type CandidateRecordOutcome = (Option<RecordedCandidate>, bool);

impl Session {
    pub(super) async fn prepare_inference_wave(
        &self,
        routes: &[(&Arc<dyn InferenceBackend>, &str, &str)],
        deadline: Instant,
    ) -> Result<()> {
        if routes.is_empty() || routes.len() > 4 {
            return Err(anyhow!("Invalid inference preparation wave"));
        }
        for (backend, role, _) in routes {
            self.validate_declared_role_boundary(role, backend.as_ref())?;
        }
        let deadline = deadline.min(self.deadline);
        let timeout_seconds = self.cfg.timeout_seconds;
        let task = &self.task;
        let preparations = routes
            .iter()
            .map(|(backend, role, prompt)| {
                Box::pin(async move {
                    let timeout = Duration::from_secs(timeout_seconds)
                        .min(deadline.saturating_duration_since(Instant::now()));
                    if timeout.is_zero() {
                        return Err(anyhow::Error::new(InferenceTimeout));
                    }
                    tokio::time::timeout(timeout, backend.prepare_for(task, role, prompt))
                        .await
                        .map_err(|_| anyhow::Error::new(InferenceTimeout))?
                }) as Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>
            })
            .collect();
        // Settle all started preparations before propagating a sibling failure.
        let outcomes = crate::reasoning::parallel::join_all(preparations).await;
        for outcome in outcomes {
            outcome?;
        }
        Ok(())
    }

    pub(super) async fn execute_independent_expert_batch(
        &mut self,
        calls: Vec<CandidateCall>,
        phase_budget: Duration,
        early_planner: Option<super::EarlyPlannerExecution>,
    ) -> Result<(Vec<(String, String, String)>, Option<String>)> {
        if calls.is_empty() || calls.len() > 4 || phase_budget.is_zero() {
            return Err(anyhow!("Invalid admitted expert batch"));
        }
        let phase_deadline = (Instant::now() + phase_budget).min(self.deadline);
        self.prepare_inference_wave(
            &calls
                .iter()
                .map(|call| (&call.backend, call.role.as_str(), call.prompt.as_str()))
                .collect::<Vec<_>>(),
            phase_deadline,
        )
        .await?;
        let phase_budget = phase_deadline.saturating_duration_since(Instant::now());
        let task = self.task.clone();
        let timeout_seconds = self.cfg.timeout_seconds;
        let futures: Vec<Option<CandidateFuture>> = calls
            .into_iter()
            .map(|call| {
                let task = task.clone();
                Box::pin(async move {
                    let started = Instant::now();
                    let timeout = Duration::from_secs(timeout_seconds).min(phase_budget);
                    let result = if timeout.is_zero() {
                        Err(anyhow::Error::new(InferenceTimeout))
                    } else {
                        tokio::time::timeout(
                            timeout,
                            call.backend.generate_for(
                                &task,
                                &call.role,
                                &call.prompt,
                                call.allocation,
                            ),
                        )
                        .await
                        .map_err(|_| anyhow::Error::new(InferenceTimeout))
                        .and_then(|result| result)
                    };
                    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                    (call, result, elapsed)
                }) as CandidateFuture
            })
            .map(Some)
            .collect();
        let mut futures = futures;
        let mut buffered = std::collections::VecDeque::new();
        let mut results = Vec::with_capacity(futures.len());
        let mut failure = None;
        let mut timed_out = false;
        let mut planner_execution = early_planner;
        let mut planner_strategy = None;

        while futures.iter().any(Option::is_some) || !buffered.is_empty() {
            let Some((call, report, elapsed_ms)) = (match buffered.pop_front() {
                Some(report) => Some(report),
                None => crate::reasoning::parallel::join_next(&mut futures).await,
            }) else {
                break;
            };
            let planner_call = call.role == "planner";
            let recorded = self.record_independent_expert_report(call, report, elapsed_ms);
            let (result, timed_out_call) = match recorded {
                Ok(value) => value,
                Err(error) => {
                    self.mark_usage_incomplete();
                    failure.get_or_insert(error);
                    continue;
                }
            };
            timed_out |= timed_out_call;
            let Some((call, expert_id, role, advice)) = result else {
                continue;
            };
            results.push((call, expert_id, role, advice.clone()));

            if planner_call {
                if let Some(reserve) = planner_execution.take() {
                    if let Ok(graph) = crate::planning::Plan::parse(&advice) {
                        self.store.event(
                            &self.task,
                            "plan",
                            "planner-r",
                            json!({
                                "graph":graph,
                                "planner_call":call,
                                "trusted_permissions":false,
                                "scheduler_required":true,
                                "overlap_outcome_bound":true
                            }),
                        )?;
                        match self
                            .execute_planner_while_draining(
                                graph,
                                reserve,
                                &mut futures,
                                &mut buffered,
                            )
                            .await
                        {
                            Ok(strategy) => planner_strategy = Some(strategy),
                            Err(error) => {
                                self.mark_usage_incomplete();
                                failure.get_or_insert(error);
                            }
                        }
                    }
                }
            }
        }
        if let Some(error) = failure {
            self.mark_usage_incomplete();
            return Err(error);
        }
        if timed_out {
            return Ok((Vec::new(), planner_strategy));
        }
        results.sort_by_key(|(call, _, _, _)| *call);
        Ok((
            results
                .into_iter()
                .map(|(_, expert_id, role, advice)| (expert_id, role, advice))
                .collect(),
            planner_strategy,
        ))
    }

    fn record_independent_expert_report(
        &mut self,
        call: CandidateCall,
        report: Result<Generation>,
        elapsed_ms: f64,
    ) -> Result<CandidateRecordOutcome> {
        self.resources.inference_ms += elapsed_ms;
        match report {
            Ok(report) => {
                let valid = report.text.len() <= 96_000
                    && !report.text.trim().is_empty()
                    && !report.model.is_empty()
                    && report.model.len() <= 256
                    && self
                        .validate_returned_role_boundary(&call.role, &report.model)
                        .is_ok()
                    && report.elapsed_ms.is_finite()
                    && report.elapsed_ms >= 0.0
                    && report
                        .queue_wait_ms
                        .is_none_or(|ms| ms.is_finite() && (0.0..=300_000.0).contains(&ms))
                    && report
                        .provider_metrics
                        .as_ref()
                        .is_none_or(|metrics| metrics.validate().is_ok())
                    && !report
                        .output_tokens
                        .is_some_and(|count| count > call.allocation as u64);
                if !valid {
                    self.mark_usage_incomplete();
                    self.store.event(
                        &self.task,
                        "expert_failure",
                        "jev-plus",
                        json!({"id":call.expert_id,"version":call.expert_version,"call":call.call,"role":call.role,"failure_kind":"invalid_result","result_verified":false,"partial_advice_discarded":true,"capabilities":call.capabilities}),
                    )?;
                    return Err(anyhow!("Invalid bounded expert result"));
                }
                self.accumulate_parallel_usage(&report);
                let advice = if matches!(call.role.as_str(), "planner" | "explorer") {
                    report.text.clone()
                } else {
                    report.text.chars().take(2_000).collect::<String>()
                };
                let node = Node {
                    id: self.nodes.len(),
                    parent: None,
                    role: call.role.clone(),
                    cycle: self.resources.cycles,
                    model: report.model,
                    content_hash: crate::store::sha256(report.text.as_bytes()),
                };
                let recorded = self.store.event(
                    &self.task,
                    "reasoning",
                    "jev-r",
                    json!({"call":call.call,"node":node,"text":report.text,"input_tokens":report.input_tokens,"output_tokens":report.output_tokens,"inference_ms":report.elapsed_ms,"queue_wait_ms":report.queue_wait_ms,"overlap_id":call.overlap_id}),
                );
                match recorded {
                    Ok(()) => self.nodes.push(node.clone()),
                    Err(error) if error.is::<crate::store::PersistedEventError>() => {
                        self.nodes.push(node.clone());
                        return Err(error);
                    }
                    Err(error) => return Err(error),
                }
                self.record_provider_metrics(
                    call.call,
                    &call.role,
                    &node,
                    report.provider_metrics.as_ref(),
                )?;
                if call.role == "planner" {
                    self.store.event(
                        &self.task,
                        "planner_overlap_result",
                        "jev-plus",
                        json!({"overlap_id":call.overlap_id,"call":call.call,"result_sha256":crate::store::sha256(advice.as_bytes()),"permissions_granted":false}),
                    )?;
                } else if call.role == "explorer" {
                    self.store.event(
                        &self.task,
                        "explorer_overlap_result",
                        "jev-r",
                        json!({"overlap_id":call.overlap_id,"call":call.call,"result_sha256":crate::store::sha256(advice.as_bytes()),"permissions_granted":false}),
                    )?;
                } else {
                    self.store.event(
                        &self.task,
                        "expert_batch_result",
                        "jev-plus",
                        json!({"id":call.expert_id,"version":call.expert_version,"call":call.call,"capabilities":call.capabilities,"result_sha256":crate::store::sha256(advice.as_bytes()),"result_verified":false,"permissions_granted":false,"overlap_id":call.overlap_id}),
                    )?;
                }
                Ok((Some((call.call, call.expert_id, call.role, advice)), false))
            }
            Err(error) => {
                self.mark_usage_incomplete();
                let is_timeout = error.is::<InferenceTimeout>();
                self.store.event(
                    &self.task,
                    "expert_failure",
                    "jev-plus",
                    json!({"id":call.expert_id,"version":call.expert_version,"call":call.call,"role":call.role,"overlap_id":call.overlap_id,"failure_kind":if is_timeout {"inference_timeout"} else {"provider_error"},"reason":error.to_string().chars().take(250).collect::<String>(),"provider_stop_proven":false,"partial_advice_discarded":true,"capabilities":call.capabilities}),
                )?;
                if is_timeout {
                    Ok((None, true))
                } else {
                    Err(error)
                }
            }
        }
    }

    async fn execute_planner_while_draining(
        &mut self,
        graph: crate::planning::Plan,
        reserve: super::EarlyPlannerExecution,
        futures: &mut [Option<CandidateFuture>],
        buffered: &mut std::collections::VecDeque<(CandidateCall, Result<Generation>, f64)>,
    ) -> Result<String> {
        let previous_call_limit = self.call_limit;
        let previous_output_limit = self.cfg.max_tokens;
        let previous_prompt_limit = self.cfg.max_input_bytes;
        self.call_limit = previous_call_limit.saturating_sub(reserve.reserved_calls);
        self.cfg.max_tokens = previous_output_limit.saturating_sub(reserve.reserved_output_tokens);
        self.cfg.max_input_bytes =
            previous_prompt_limit.saturating_sub(reserve.reserved_prompt_bytes);
        let remaining = reserve.deadline.saturating_duration_since(Instant::now());
        let base = reserve.base;
        let result = {
            let plan = tokio::time::timeout(remaining, self.execute_plan(graph, &base));
            tokio::pin!(plan);
            loop {
                if futures.iter().any(Option::is_some) {
                    tokio::select! {
                        outcome = &mut plan => {
                            break match outcome {
                                Ok(result) => result,
                                Err(_) => Err(anyhow::Error::new(InferenceTimeout)),
                            };
                        }
                        report = crate::reasoning::parallel::join_next(futures) => {
                            if let Some(report) = report {
                                buffered.push_back(report);
                            }
                        }
                    }
                } else {
                    break match plan.await {
                        Ok(result) => result,
                        Err(_) => Err(anyhow::Error::new(InferenceTimeout)),
                    };
                }
            }
        };
        self.call_limit = previous_call_limit;
        self.cfg.max_tokens = previous_output_limit;
        self.cfg.max_input_bytes = previous_prompt_limit;
        result
    }

    fn mark_usage_incomplete(&mut self) {
        self.complete_usage = false;
        self.resources.input_tokens = None;
        self.resources.output_tokens = None;
        self.complete_queue_metrics = false;
        self.resources.queue_wait_ms = None;
    }
}
