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
    pub expert_id: String,
    pub expert_version: String,
    pub capabilities: Vec<String>,
    pub call: u32,
    pub prompt: String,
    pub backend: Arc<dyn InferenceBackend>,
}

type CandidateFuture =
    Pin<Box<dyn Future<Output = (CandidateCall, Result<Generation>, f64)> + Send>>;

impl Session {
    pub(super) async fn execute_independent_expert_batch(
        &mut self,
        calls: Vec<CandidateCall>,
        phase_budget: Duration,
    ) -> Result<Vec<(String, String)>> {
        if calls.is_empty() || calls.len() > 4 || phase_budget.is_zero() {
            return Err(anyhow!("Invalid admitted expert batch"));
        }
        let timeout_metadata = calls
            .iter()
            .map(|call| {
                (
                    call.expert_id.clone(),
                    call.expert_version.clone(),
                    call.capabilities.clone(),
                )
            })
            .collect::<Vec<_>>();
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
                            call.backend
                                .generate_for(&task, "solver", &call.prompt, 192),
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
        let reports =
            match tokio::time::timeout(phase_budget, crate::reasoning::parallel::join_all(futures))
                .await
            {
                Ok(reports) => reports,
                Err(_) => {
                    self.complete_usage = false;
                    self.resources.input_tokens = None;
                    self.resources.output_tokens = None;
                    self.complete_queue_metrics = false;
                    self.resources.queue_wait_ms = None;
                    for call in timeout_metadata {
                        self.store.event(
                            &self.task,
                            "expert_failure",
                            "jev-plus",
                            json!({
                                "id":call.0,
                                "version":call.1,
                                "failure_kind":"batch_timeout",
                                "reason":"Expert result is unknown after the shared phase deadline",
                                "provider_stop_proven":false,
                                "partial_advice_discarded":true,
                                "capabilities":call.2,
                                "permissions_granted":false
                            }),
                        )?;
                    }
                    return Ok(Vec::new());
                }
            };

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
                        && !report.output_tokens.is_some_and(|count| count > 192);
                    if !valid {
                        self.mark_usage_incomplete();
                        self.store.event(
                            &self.task,
                            "expert_failure",
                            "jev-plus",
                            json!({"id":call.expert_id,"version":call.expert_version,"call":call.call,"failure_kind":"invalid_result","result_verified":false,"partial_advice_discarded":true,"capabilities":call.capabilities}),
                        )?;
                        failure.get_or_insert_with(|| anyhow!("Invalid bounded expert result"));
                        continue;
                    }
                    self.accumulate_parallel_usage(&report);
                    let advice = report.text.chars().take(2_000).collect::<String>();
                    let node = Node {
                        id: self.nodes.len(),
                        // Workflow dependency edges are recorded by expert_step;
                        // completion order does not establish a causal parent.
                        parent: None,
                        role: "solver".into(),
                        cycle: self.resources.cycles,
                        model: report.model,
                        content_hash: crate::store::sha256(report.text.as_bytes()),
                    };
                    self.store.event(
                        &self.task,
                        "reasoning",
                        "jev-r",
                        json!({"call":call.call,"node":node,"text":report.text,"input_tokens":report.input_tokens,"output_tokens":report.output_tokens,"inference_ms":report.elapsed_ms,"queue_wait_ms":report.queue_wait_ms}),
                    )?;
                    self.record_provider_metrics(
                        call.call,
                        "solver",
                        &node,
                        report.provider_metrics.as_ref(),
                    )?;
                    self.nodes.push(node);
                    self.store.event(
                        &self.task,
                        "expert_batch_result",
                        "jev-plus",
                        json!({"id":call.expert_id,"version":call.expert_version,"call":call.call,"capabilities":call.capabilities,"result_sha256":crate::store::sha256(advice.as_bytes()),"result_verified":false,"permissions_granted":false}),
                    )?;
                    results.push((call.expert_id, advice));
                }
                Err(error) => {
                    self.mark_usage_incomplete();
                    let is_timeout = error.is::<InferenceTimeout>();
                    self.store.event(
                        &self.task,
                        "expert_failure",
                        "jev-plus",
                        json!({"id":call.expert_id,"version":call.expert_version,"call":call.call,"failure_kind":if is_timeout {"inference_timeout"} else {"provider_error"},"reason":error.to_string().chars().take(250).collect::<String>(),"provider_stop_proven":false,"partial_advice_discarded":true,"capabilities":call.capabilities}),
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
