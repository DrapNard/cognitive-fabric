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

impl Session {
    pub(super) async fn prepare_inference_wave(
        &self,
        routes: &[(&Arc<dyn InferenceBackend>, &str, &str)],
        deadline: Instant,
    ) -> Result<()> {
        if routes.is_empty() || routes.len() > 4 {
            return Err(anyhow!("Invalid inference preparation wave"));
        }
        let deadline = deadline.min(self.deadline);
        for (backend, role, prompt) in routes {
            let timeout = Duration::from_secs(self.cfg.timeout_seconds)
                .min(deadline.saturating_duration_since(Instant::now()));
            if timeout.is_zero() {
                return Err(anyhow::Error::new(InferenceTimeout));
            }
            tokio::time::timeout(timeout, backend.prepare_for(&self.task, role, prompt))
                .await
                .map_err(|_| anyhow::Error::new(InferenceTimeout))??;
        }
        Ok(())
    }

    pub(super) async fn execute_independent_expert_batch(
        &mut self,
        calls: Vec<CandidateCall>,
        phase_budget: Duration,
    ) -> Result<Vec<(String, String, String)>> {
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
        let futures: Vec<CandidateFuture> = calls
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
            .collect();
        // Every member future owns its timeout. Await all of them so one slow
        // provider cannot cause the parent to drop still-running siblings.
        let reports = crate::reasoning::parallel::join_all(futures).await;

        let mut results = Vec::with_capacity(reports.len());
        let mut failure = None;
        let mut timed_out = false;
        for (call, report, elapsed_ms) in reports {
            self.resources.inference_ms += elapsed_ms;
            match report {
                Ok(report) => {
                    let valid = report.text.len() <= 96_000
                        && !report.text.trim().is_empty()
                        && !report.model.is_empty()
                        && report.model.len() <= 256
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
                        failure.get_or_insert_with(|| anyhow!("Invalid bounded expert result"));
                        continue;
                    }
                    self.accumulate_parallel_usage(&report);
                    let advice = if call.role == "planner" {
                        report.text.clone()
                    } else {
                        report.text.chars().take(2_000).collect::<String>()
                    };
                    let node = Node {
                        id: self.nodes.len(),
                        // Workflow dependency edges are recorded by expert_step;
                        // completion order does not establish a causal parent.
                        parent: None,
                        role: call.role.clone(),
                        cycle: self.resources.cycles,
                        model: report.model,
                        content_hash: crate::store::sha256(report.text.as_bytes()),
                    };
                    self.store.event(
                        &self.task,
                        "reasoning",
                        "jev-r",
                        json!({"call":call.call,"node":node,"text":report.text,"input_tokens":report.input_tokens,"output_tokens":report.output_tokens,"inference_ms":report.elapsed_ms,"queue_wait_ms":report.queue_wait_ms,"overlap_id":call.overlap_id}),
                    )?;
                    self.record_provider_metrics(
                        call.call,
                        &call.role,
                        &node,
                        report.provider_metrics.as_ref(),
                    )?;
                    self.nodes.push(node);
                    if call.role == "planner" {
                        self.store.event(
                            &self.task,
                            "planner_overlap_result",
                            "jev-plus",
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
                    results.push((call.expert_id, call.role, advice));
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
                        timed_out = true;
                    } else {
                        failure.get_or_insert(error);
                    }
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        if timed_out {
            return Ok(Vec::new());
        }
        Ok(results)
    }

    fn mark_usage_incomplete(&mut self) {
        self.complete_usage = false;
        self.resources.input_tokens = None;
        self.resources.output_tokens = None;
        self.complete_queue_metrics = false;
        self.resources.queue_wait_ms = None;
    }
}
