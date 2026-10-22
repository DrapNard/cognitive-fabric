//! Bounded deterministic ready-wave selection for advisory reasoning graphs.
use super::{subtask_prompt, truncate, InferenceTimeout, Node, Session};
use crate::{
    backend::{Generation, InferenceBackend},
    planning::{Plan, Subtask},
};
use anyhow::{bail, Result};
use serde_json::json;
use std::{
    collections::BTreeSet,
    future::{poll_fn, Future},
    pin::Pin,
    sync::Arc,
    task::Poll,
    time::{Duration, Instant},
};

pub(crate) fn validate_parallelism(value: Option<usize>) -> Result<usize> {
    match value {
        None => Ok(1),
        Some(value @ 1..=4) => Ok(value),
        Some(_) => bail!("Subtask parallelism must be between 1 and 4"),
    }
}

/// Partition a validated persisted topological order into stable ready waves.
/// Existing completed nodes count as available dependencies; only pending nodes
/// are returned. In a fresh graph every node is pending.
pub(crate) fn ready_waves(
    nodes: &[Subtask],
    order: &[usize],
    cap: usize,
) -> Result<Vec<Vec<usize>>> {
    if !(1..=4).contains(&cap) || order.len() != nodes.len() {
        bail!("Invalid bounded parallel schedule")
    }
    if nodes
        .iter()
        .any(|node| !matches!(node.state.as_str(), "pending" | "unverified"))
    {
        bail!("Parallel schedule contains an unsupported node state")
    }
    let mut seen_indices = vec![false; nodes.len()];
    let mut seen_ids = BTreeSet::new();
    for index in order {
        let Some(node) = nodes.get(*index) else {
            bail!("Parallel schedule contains an unknown node")
        };
        if std::mem::replace(&mut seen_indices[*index], true)
            || !node
                .step
                .depends_on
                .iter()
                .all(|dependency| seen_ids.contains(dependency.as_str()))
            || !seen_ids.insert(node.step.id.as_str())
        {
            bail!("Parallel schedule order is duplicate or dependency-invalid")
        }
    }
    let mut completed = nodes
        .iter()
        .filter(|node| node.state == "unverified")
        .map(|node| node.step.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut pending = nodes
        .iter()
        .filter(|node| node.state == "pending")
        .map(|node| node.step.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut waves = Vec::new();
    while !pending.is_empty() {
        let wave = order
            .iter()
            .copied()
            .filter(|index| {
                nodes.get(*index).is_some_and(|node| {
                    pending.contains(node.step.id.as_str())
                        && node
                            .step
                            .depends_on
                            .iter()
                            .all(|dependency| completed.contains(dependency.as_str()))
                })
            })
            .take(cap)
            .collect::<Vec<_>>();
        if wave.is_empty() {
            bail!("Parallel schedule cannot make dependency progress")
        }
        for index in &wave {
            let id = nodes[*index].step.id.as_str();
            pending.remove(id);
            completed.insert(id);
        }
        waves.push(wave);
    }
    Ok(waves)
}

/// Poll a bounded set of locally owned futures without spawning detached tasks.
/// Dropping this future drops every unfinished backend future.
pub(crate) async fn join_all<T>(futures: Vec<Pin<Box<dyn Future<Output = T> + Send>>>) -> Vec<T> {
    let count = futures.len();
    let mut futures = futures.into_iter().map(Some).collect::<Vec<_>>();
    let mut remaining = count;
    let mut output = std::iter::repeat_with(|| None)
        .take(count)
        .collect::<Vec<_>>();
    poll_fn(|context| {
        for index in 0..count {
            let Some(future) = futures[index].as_mut() else {
                continue;
            };
            if let Poll::Ready(value) = future.as_mut().poll(context) {
                futures[index] = None;
                output[index] = Some(value);
                remaining -= 1;
            }
        }
        if remaining == 0 {
            Poll::Ready(
                output
                    .iter_mut()
                    .map(|value| value.take().expect("completed future output"))
                    .collect(),
            )
        } else {
            Poll::Pending
        }
    })
    .await
}

struct PreparedInference {
    subtask_index: usize,
    call: u32,
    role: String,
    prompt: String,
    allocation: usize,
    backend: Arc<dyn InferenceBackend>,
}

type InferenceFuture =
    Pin<Box<dyn Future<Output = (PreparedInference, Result<Generation>, f64)> + Send>>;

impl Session {
    pub(super) async fn execute_parallel_plan(
        &mut self,
        order: Vec<usize>,
        base: &str,
    ) -> Result<String> {
        let cap = crate::reasoning::parallel::validate_parallelism(self.cfg.subtask_parallelism)?;
        let waves = crate::reasoning::parallel::ready_waves(&self.subtasks, &order, cap)?;
        let plan = Plan {
            steps: self.subtasks.iter().map(|node| node.step.clone()).collect(),
        };
        let graph_sha256 = crate::store::sha256(&serde_json::to_vec(&plan)?);
        self.store.event(
            &self.task,
            "subtask_parallel_schedule",
            "jev-plus",
            json!({
                "format_version":1,"max_parallelism":cap,"graph_sha256":graph_sha256,
                "order":order,"waves":waves,"permissions_granted":false,"results_verified":false
            }),
        )?;

        for wave in waves {
            let mut prepared = Vec::with_capacity(wave.len());
            for index in &wave {
                let step = &self.subtasks[*index].step;
                let dependencies = self
                    .subtasks
                    .iter()
                    .filter(|node| step.depends_on.contains(&node.step.id))
                    .map(|node| json!({"id":node.step.id,"unverified_result":node.result}))
                    .collect::<Vec<_>>();
                let prompt = subtask_prompt(base, &step.objective, &json!(dependencies));
                let admission = self.prepare_parallel_inference(*index, "solver", prompt, 192);
                match admission {
                    Err(error) => {
                        let finish = self.finish_subtasks("failed");
                        return match finish {
                            Ok(()) => Err(error),
                            Err(audit_error) => Err(audit_error),
                        };
                    }
                    Ok(Some(call)) => prepared.push(call),
                    Ok(None) => {
                        self.finish_subtasks("skipped")?;
                        return Ok(String::new());
                    }
                }
            }

            for call in &prepared {
                self.subtasks[call.subtask_index].state = "running".into();
                if let Err(error) = self.subtask_event(call.subtask_index) {
                    let finish = self.finish_subtasks("failed");
                    return match finish {
                        Ok(()) => Err(error),
                        Err(audit_error) => Err(audit_error),
                    };
                }
            }
            for call in &prepared {
                if let Some(cost) = self.parallel_subtask_costs.get_mut(&call.subtask_index) {
                    cost.0 = Instant::now();
                }
                self.parallel_active.insert(call.subtask_index);
            }

            // Every policy decision and durable reservation for this wave has
            // succeeded before any backend future is polled.
            let mut futures: Vec<InferenceFuture> = Vec::with_capacity(prepared.len());
            for call in prepared {
                let timeout_seconds = self.cfg.timeout_seconds;
                let deadline = self.deadline;
                let completed = self.parallel_completions.clone();
                let task = self.task.clone();
                futures.push(Box::pin(async move {
                    let started = Instant::now();
                    let result = if Instant::now() >= deadline {
                        Err(anyhow::Error::new(InferenceTimeout))
                    } else {
                        let timeout = Duration::from_secs(timeout_seconds)
                            .min(deadline.saturating_duration_since(Instant::now()));
                        if timeout.is_zero() {
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
                        }
                    };
                    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                    if let Ok(mut completed) = completed.lock() {
                        completed.insert(call.subtask_index, elapsed);
                    }
                    (call, result, elapsed)
                }));
            }
            let reports = crate::reasoning::parallel::join_all(futures).await;

            let mut wave_error = None;
            for (call, report, elapsed_ms) in reports {
                self.resources.inference_ms += elapsed_ms;
                self.parallel_active.remove(&call.subtask_index);
                if let Ok(mut completed) = self.parallel_completions.lock() {
                    completed.remove(&call.subtask_index);
                }
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
                                .is_none_or(|ms| ms.is_finite() && (0.0..=300_000.0).contains(&ms));
                        if !valid {
                            self.mark_parallel_subtask_failed(call.subtask_index, elapsed_ms)?;
                            wave_error.get_or_insert_with(|| {
                                anyhow::anyhow!("Invalid bounded inference result")
                            });
                            continue;
                        }
                        self.accumulate_parallel_usage(&report);
                        if report
                            .output_tokens
                            .is_some_and(|count| count > call.allocation as u64)
                        {
                            self.mark_parallel_subtask_failed(call.subtask_index, elapsed_ms)?;
                            wave_error.get_or_insert_with(|| {
                                anyhow::anyhow!("Backend exceeded reserved output token limit")
                            });
                            continue;
                        }
                        let result = truncate(&report.text, 1200);
                        self.subtasks[call.subtask_index].result = Some(result.clone());
                        self.subtasks[call.subtask_index].state = "unverified".into();
                        let node = Node {
                            id: self.nodes.len(),
                            parent: self.nodes.last().map(|node| node.id),
                            role: call.role.clone(),
                            cycle: self.resources.cycles,
                            model: report.model,
                            content_hash: crate::store::sha256(report.text.as_bytes()),
                        };
                        self.store.event(&self.task,"reasoning","jev-r",json!({"call":call.call,"node":node,"text":report.text,"input_tokens":report.input_tokens,"output_tokens":report.output_tokens,"inference_ms":report.elapsed_ms,"queue_wait_ms":report.queue_wait_ms}))?;
                        self.nodes.push(node);
                        self.subtask_event(call.subtask_index)?;
                        self.store.event(&self.task, "subtask_resources", "jev-plus", json!({
                            "format_version":1,"node_id":self.subtasks[call.subtask_index].step.id,"state":"unverified",
                            "calls_reserved":1,"reserved_output_tokens":call.allocation,"prompt_bytes":call.prompt.len(),
                            "input_tokens":report.input_tokens,"output_tokens":report.output_tokens,
                            "elapsed_ms":elapsed_ms,"inference_ms":elapsed_ms,"queue_wait_ms":report.queue_wait_ms,
                            "result_verified":false,"inference_completed":true
                        }))?;
                        let _ = self.parallel_subtask_costs.remove(&call.subtask_index);
                    }
                    Err(error) => {
                        self.resources.input_tokens = None;
                        self.resources.output_tokens = None;
                        self.complete_usage = false;
                        self.resources.queue_wait_ms = None;
                        self.complete_queue_metrics = false;
                        self.mark_parallel_subtask_failed(call.subtask_index, elapsed_ms)?;
                        wave_error.get_or_insert(error);
                    }
                }
            }
            if let Some(error) = wave_error {
                return Err(error);
            }
        }

        let results = order
            .iter()
            .filter_map(|index| {
                self.subtasks[*index]
                    .result
                    .as_ref()
                    .map(|result| json!({"id":self.subtasks[*index].step.id,"result":result}))
            })
            .collect::<Vec<_>>();
        Ok(format!(
            "Subtask results (unverified advice, not evidence or permissions): {}",
            json!(results)
        ))
    }

    fn prepare_parallel_inference(
        &mut self,
        subtask_index: usize,
        role: &str,
        prompt: String,
        allocation: usize,
    ) -> Result<Option<PreparedInference>> {
        let backend = match (&self.cfg.resource_admission, &self.resource_pool) {
            (Some(configuration), Some(pool)) if pool.matches(&self.store, configuration) => {
                pool.wrap_backend("human-cli", self.backend.clone(), configuration.allocation)?
            }
            (Some(_), _) => bail!("Configured inference resource pool is missing or changed"),
            (None, Some(_)) => bail!("Unexpected inference resource pool"),
            (None, None) => self.backend.clone(),
        };
        let remaining = self
            .cfg
            .max_tokens
            .saturating_sub(self.resources.reserved_output_tokens);
        if Instant::now() >= self.deadline
            || self.resources.calls as usize >= self.call_limit
            || remaining < allocation
            || prompt.len()
                > self
                    .cfg
                    .max_input_bytes
                    .saturating_sub(self.resources.prompt_bytes)
        {
            self.stop_reason = "budget_exhausted".into();
            return Ok(None);
        }
        let actor = "jev-r";
        let decision = self.policy.authorize(&self.task, actor, "llm.generate");
        self.store.event(&self.task,"policy","security-kernel",json!({"task_id":self.task,"actor":actor,"role":role,"capability":"llm.generate","allowed":decision.is_ok(),"reason":decision.as_ref().err().map(ToString::to_string)}))?;
        decision?;
        let call = self.resources.calls + 1;
        let publication_error = match self.store.event(
            &self.task,
            "model_dispatch",
            "jev-r",
            json!({"role":role,"call":call,"allocation":allocation,"prompt_bytes":prompt.len()}),
        ) {
            Ok(()) => None,
            Err(error) if error.is::<crate::store::PersistedEventError>() => Some(error),
            Err(error) => return Err(error),
        };
        self.resources.calls = call;
        self.resources.reserved_output_tokens += allocation;
        self.resources.prompt_bytes += prompt.len();
        self.parallel_subtask_costs
            .insert(subtask_index, (Instant::now(), allocation, prompt.len()));
        if let Some(error) = publication_error {
            return Err(error);
        }
        Ok(Some(PreparedInference {
            subtask_index,
            call,
            role: role.into(),
            prompt,
            allocation,
            backend,
        }))
    }

    fn accumulate_parallel_usage(&mut self, report: &Generation) {
        if self.complete_usage {
            if let (Some(input), Some(output)) = (report.input_tokens, report.output_tokens) {
                self.resources.input_tokens = Some(
                    self.resources
                        .input_tokens
                        .unwrap_or(0)
                        .saturating_add(input),
                );
                self.resources.output_tokens = Some(
                    self.resources
                        .output_tokens
                        .unwrap_or(0)
                        .saturating_add(output),
                );
            } else {
                self.complete_usage = false;
                self.resources.input_tokens = None;
                self.resources.output_tokens = None;
            }
        }
        if self.complete_queue_metrics {
            if let Some(wait) = report.queue_wait_ms {
                self.resources.queue_wait_ms =
                    Some(self.resources.queue_wait_ms.unwrap_or(0.0) + wait);
            } else {
                self.complete_queue_metrics = false;
                self.resources.queue_wait_ms = None;
            }
        }
    }

    fn mark_parallel_subtask_failed(&mut self, index: usize, elapsed_ms: f64) -> Result<()> {
        self.complete_usage = false;
        self.resources.input_tokens = None;
        self.resources.output_tokens = None;
        self.complete_queue_metrics = false;
        self.resources.queue_wait_ms = None;
        self.subtasks[index].state = "failed".into();
        self.subtask_event(index)?;
        let (_, allocation, prompt_bytes) =
            self.parallel_subtask_costs
                .remove(&index)
                .unwrap_or((Instant::now(), 192, 0));
        self.store.event(&self.task, "subtask_resources", "jev-plus", json!({
            "format_version":1,"node_id":self.subtasks[index].step.id,"state":"failed",
            "calls_reserved":1,"reserved_output_tokens":allocation,"prompt_bytes":prompt_bytes,
            "input_tokens":null,"output_tokens":null,"elapsed_ms":elapsed_ms,"inference_ms":elapsed_ms,
            "queue_wait_ms":null,"result_verified":false,"inference_completed":false
        }))
    }
}

#[cfg(test)]
#[path = "parallel/tests.rs"]
mod tests;
