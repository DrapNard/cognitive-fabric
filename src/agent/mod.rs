//! Transport-independent, bounded solver turns for external agent clients.
//! Tool calls are proposals; execution and approval belong to the client.
mod local;
mod types;
use crate::{
    config::Config,
    recovery::ExecutionLease,
    security::TaskPolicy,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
pub use local::LocalChat;
use serde_json::json;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
pub use types::*;

pub trait ChatBackend: Send + Sync {
    fn complete<'a>(
        &'a self,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct AgentRuntime {
    cfg: Config,
    store: Arc<Store>,
    backend: Arc<dyn ChatBackend>,
}
impl AgentRuntime {
    pub fn local(cfg: Config, store: Arc<Store>) -> Result<Self> {
        // Native conversation RPC is deliberately separate from role-prompt RPC.
        // Never silently bypass an operator's managed worker requirement.
        if cfg.inference_process || !cfg.role_profiles.is_empty() {
            bail!("Agent turns currently require a direct backend without role overrides")
        }
        store.pin_model(&cfg)?;
        Self::with_backend(cfg.clone(), store, Arc::new(LocalChat(cfg)))
    }
    pub fn with_backend(
        cfg: Config,
        store: Arc<Store>,
        backend: Arc<dyn ChatBackend>,
    ) -> Result<Self> {
        cfg.validate()?;
        Ok(Self {
            cfg,
            store,
            backend,
        })
    }
    pub fn validate(&self, request: &Turn) -> Result<usize> {
        request.validate()?;
        let bytes = serde_json::to_vec(request)?.len();
        if bytes > self.cfg.max_input_bytes || self.cfg.max_calls == 0 || self.cfg.max_cycles == 0 {
            bail!("Agent turn exceeds configured input or inference admission")
        }
        if self.cfg.max_tool_calls == 0
            && !request.tools.is_empty()
            && request.tool_choice.as_ref().is_none_or(|v| v != "none")
        {
            bail!("Tool proposals are disabled by the operator budget")
        }
        let tokens = request.max_tokens.unwrap_or(self.cfg.max_tokens.min(4096));
        if tokens == 0 || tokens > self.cfg.max_tokens || tokens > 16384 {
            bail!("Requested output tokens exceed configured admission")
        }
        Ok(tokens)
    }
    pub async fn run_until(
        &self,
        request: Turn,
        cancel: impl Future<Output = ()>,
    ) -> Result<TurnResult> {
        let tokens = self.validate(&request)?;
        let start = Instant::now();
        let model_registration = self.store.pin_model(&self.cfg)?;
        let _lease = self
            .store
            .runtime_lock
            .as_deref()
            .map(|p| ExecutionLease::acquire(p, false))
            .transpose()?;
        // Store only a digest here: client conversations may contain source code.
        let bytes = serde_json::to_vec(&request)?;
        let task = self
            .store
            .create_task(&format!("Agent turn {}", sha256(&bytes)), "fast")?;
        let mut guard = TurnGuard {
            store: self.store.clone(),
            task: task.clone(),
            finished: false,
        };
        let result = async {
            self.store.event(&task,"model_registration","runtime",json!({"registration":model_registration,"weight_identity_verified":false}))?;
            let policy = TaskPolicy::agent(&task)?;
            policy.authorize(&task,"jev-r","llm.generate")?;
            self.store.event(&task,"agent_admission","jev-plus",json!({"format_version":1,"request_hash":sha256(&bytes),"prompt_bytes":bytes.len(),"reserved_output_tokens":tokens,"calls":1,"cycles":1,"max_wall_seconds":self.cfg.max_wall_seconds,"model":self.cfg.model,"client_tools_executed":false,"verification":"unverified"}))?;
            self.store.event(&task,"policy","security-kernel",json!({"actor":"jev-r","capability":"llm.generate","allowed":true}))?;
            if start.elapsed() >= Duration::from_secs(self.cfg.max_wall_seconds) {
                self.store.finish_task(&task,"exhausted","")?; guard.finished=true;
                bail!("Agent turn deadline exhausted before dispatch")
            }
            let completion = tokio::select! {
                biased;
                _ = cancel => { self.store.finish_task(&task,"cancelled","")?; guard.finished=true; bail!("Agent turn cancelled") },
                result = tokio::time::timeout_at(tokio::time::Instant::from_std(start + Duration::from_secs(self.cfg.max_wall_seconds)),self.backend.complete(&request,tokens)) => match result {
                    Ok(result) => result?,
                    Err(_) => {self.store.finish_task(&task,"exhausted","")?; guard.finished=true; bail!("Agent turn deadline exhausted")},
                },
            };
            completion.validate(&request, self.cfg.max_tool_calls)?;
            if completion.output_tokens.is_some_and(|n|n > tokens as u64) { bail!("Provider exceeded reserved output tokens") }
            self.store.event(&task,"agent_result","solver-r",json!({"model":completion.model,"input_tokens":completion.input_tokens,"output_tokens":completion.output_tokens,"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"proposed_tools":completion.tool_calls.len(),"content_hash":sha256(completion.content.as_deref().unwrap_or("").as_bytes()),"verified":false,"tools_executed":0}))?;
            // Tool requests and client-reported results never establish verified success.
            self.store.finish_task(&task,"unverified","")?;
            guard.finished=true;
            Ok(TurnResult {task_id:task.clone(), completion})
        }.await;
        if result.is_err() && !guard.finished {
            self.store.finish_task(&task, "failed", "")?;
            guard.finished = true;
        }
        result
    }
}
pub struct TurnResult {
    pub task_id: String,
    pub completion: Completion,
}
struct TurnGuard {
    store: Arc<Store>,
    task: String,
    finished: bool,
}
impl Drop for TurnGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.store.finish_task(&self.task, "interrupted", "");
        }
    }
}

#[cfg(test)]
mod tests;
