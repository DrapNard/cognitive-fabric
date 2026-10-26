//! Transport-independent, bounded solver turns for external agent clients.
//! Tool calls are proposals; execution and approval belong to the client.
mod collective;
mod local;
pub mod process;
pub mod protocol;
mod session_cleanup;
mod session_context;
mod sessions;
mod types;
pub mod unix;
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
    /// Exact operator-configured input-token admission capability. Backends
    /// return None unless every task-scoped request is subject to that policy.
    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        None
    }
    /// Return a stable backend snapshot for profile selection and dispatch.
    /// Implementations that cannot pin their route must leave this unsupported.
    fn pin(self: Arc<Self>) -> Result<Option<Arc<dyn ChatBackend>>> {
        Ok(None)
    }
    /// Trusted local adapter metadata; unknown does not attest resident coverage.
    fn inference_target(&self) -> Result<Option<crate::backend::InferenceTarget>> {
        Ok(None)
    }
    fn complete_for<'a>(
        &'a self,
        _task: &'a str,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        self.complete(request, tokens)
    }

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
    managed: bool,
    resource_pool: Option<Arc<crate::resource_scheduler::ResourcePool>>,
}
impl AgentRuntime {
    pub fn local(cfg: Config, store: Arc<Store>) -> Result<Self> {
        // Native conversation RPC is deliberately separate from role-prompt RPC.
        // Never silently bypass an operator's managed worker requirement.
        if !cfg.role_profiles.is_empty()
            && (cfg.language_model().is_none()
                || cfg
                    .role_profiles
                    .keys()
                    .any(|role| !matches!(role.as_str(), "language" | "interface" | "synthesizer")))
        {
            bail!("Agent turns currently require no role overrides")
        }
        if cfg.inference_process {
            let mut expected = cfg.db().into_os_string();
            expected.push(".runtime.lock");
            if store
                .runtime_lock
                .as_deref()
                .map(std::path::Path::as_os_str)
                != Some(expected.as_os_str())
            {
                bail!("Native managed inference requires the persistent workspace database")
            }
        }
        store.pin_model(&cfg)?;
        let managed = cfg.inference_process;
        let mut runtime = Self::with_backend(cfg.clone(), store, Arc::new(LocalChat(cfg)))?;
        runtime.managed = managed;
        Ok(runtime)
    }
    pub fn with_backend(
        cfg: Config,
        store: Arc<Store>,
        backend: Arc<dyn ChatBackend>,
    ) -> Result<Self> {
        cfg.validate()?;
        if let Some(settings) = cfg.input_token_admission {
            crate::token_admission::validate_config(&cfg)?;
            if backend.input_token_admission() != Some(settings) {
                bail!("Configured agent backend cannot enforce input-token admission")
            }
        } else if backend.input_token_admission().is_some() {
            bail!("Agent backend token-admission capability differs from configuration")
        }
        let resource_pool = cfg
            .resource_admission
            .as_ref()
            .map(|configuration| {
                crate::resource_scheduler::ResourcePool::configured(store.clone(), configuration)
            })
            .transpose()?;
        Ok(Self {
            cfg,
            store,
            backend,
            managed: false,
            resource_pool,
        })
    }
    /// Bind native turns to the same configured budget as collective requests.
    /// Existing cloned runtimes keep their pool. Only trusted host setup calls this.
    pub fn with_resource_pool(
        mut self,
        actor: &str,
        pool: Option<Arc<crate::resource_scheduler::ResourcePool>>,
    ) -> Result<Self> {
        crate::security::authorize(actor, "resources.configure")?;
        match (&self.cfg.resource_admission, &pool) {
            (None, None) => (),
            (Some(configuration), Some(pool)) if pool.matches(&self.store, configuration) => (),
            _ => bail!("Agent resource pool differs from its operator configuration"),
        }
        self.resource_pool = pool;
        Ok(self)
    }
    pub fn validate(&self, request: &Turn) -> Result<usize> {
        if self.backend.input_token_admission() != self.cfg.input_token_admission {
            bail!("Configured agent backend cannot enforce input-token admission")
        }
        match (&self.cfg.resource_admission, &self.resource_pool) {
            (None, None) => (),
            (Some(configuration), Some(pool)) if pool.matches(&self.store, configuration) => (),
            _ => bail!("Configured agent resource pool is missing or changed"),
        }
        request.validate()?;
        if self.cfg.agent_collective && request.messages.len() > 254 {
            bail!("Collective conversations allow 254 messages plus two runtime role messages")
        }
        local::validate_token_request(&self.cfg, request)?;
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
        if self.managed {
            // Reject an unrepresentable native frame before task/child admission.
            protocol::encode(&protocol::Packet {
                identity: protocol::Identity {
                    correlation: uuid::Uuid::nil().to_string(),
                    task: uuid::Uuid::nil().to_string(),
                    expires_at_ms: 1,
                },
                payload: protocol::Payload::Request {
                    turn: request.clone(),
                    allocation: tokens,
                },
            })?;
        }
        Ok(tokens)
    }
    /// Dispatch one already reserved call. Collective managed calls each retain
    /// their own one-use worker. Dispatch uses the original deadline; owned
    /// cleanup may outlive cancellation and remains subject to shutdown draining.
    async fn dispatch(
        &self,
        backend: Arc<dyn ChatBackend>,
        task: &str,
        request: &Turn,
        tokens: usize,
        deadline: Instant,
    ) -> Result<Completion> {
        if Instant::now() >= deadline {
            bail!("Agent deadline exhausted before backend dispatch")
        }
        let process = if self.managed && self.cfg.agent_collective {
            Some(Arc::new(process::ProcessTask::new(
                self.cfg.clone(),
                self.store.clone(),
                task,
                deadline,
            )?))
        } else {
            None
        };
        let backend: Arc<dyn ChatBackend> = process
            .as_ref()
            .map(|p| p.clone() as Arc<dyn ChatBackend>)
            .unwrap_or(backend);
        let result = if let (Some(configuration), Some(pool)) =
            (&self.cfg.resource_admission, &self.resource_pool)
        {
            // All client roles share the same configured solver target. This is
            // resource admission, not permission for model/profile switching.
            let (completion, _) = if configuration.working_profiles.is_empty() {
                pool.execute(
                    task,
                    "solver",
                    configuration.allocation,
                    || backend.inference_target(),
                    || async {
                        if Instant::now() >= deadline {
                            bail!("Agent deadline exhausted after resource admission")
                        }
                        backend.complete_for(task, request, tokens).await
                    },
                )
                .await?
            } else {
                let pinned = backend.clone().pin()?.ok_or_else(|| {
                    anyhow::anyhow!("Configured working profiles require a pinnable agent backend")
                })?;
                pool.execute_profiled(
                    task,
                    "solver",
                    configuration.allocation,
                    || pinned.inference_target(),
                    || async {
                        if Instant::now() >= deadline {
                            bail!("Agent deadline exhausted after resource admission")
                        }
                        pinned.complete_for(task, request, tokens).await
                    },
                )
                .await?
            };
            Ok(completion)
        } else {
            if Instant::now() >= deadline {
                bail!("Agent deadline exhausted before inference")
            }
            backend.complete_for(task, request, tokens).await
        };
        if let Some(process) = process {
            process.shutdown().await?;
        }
        result
    }
    pub async fn run_until(
        &self,
        request: Turn,
        cancel: impl Future<Output = ()>,
    ) -> Result<TurnResult> {
        let tokens = self.validate(&request)?;
        if self.cfg.agent_collective {
            return sessions::run(self, request, tokens, cancel).await;
        }
        if request.messages.last().is_some_and(|m| {
            m.role == "tool"
                && m.tool_call_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("cgf_"))
        }) {
            bail!("Collective continuation is disabled; restore its original configuration")
        }
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
        let mode = if self.cfg.agent_collective {
            collective::mode(&self.cfg, &request)
        } else {
            "fast"
        };
        let task = self
            .store
            .create_task(&format!("Agent turn {}", sha256(&bytes)), mode)?;
        let mut guard = TurnGuard {
            store: self.store.clone(),
            task: task.clone(),
            finished: false,
        };
        let process = if self.managed && !self.cfg.agent_collective {
            Some(Arc::new(process::ProcessTask::new(
                self.cfg.clone(),
                self.store.clone(),
                &task,
                start + Duration::from_secs(self.cfg.max_wall_seconds),
            )?))
        } else {
            None
        };
        let backend: Arc<dyn ChatBackend> = process
            .as_ref()
            .map(|p| p.clone() as Arc<dyn ChatBackend>)
            .unwrap_or_else(|| self.backend.clone());
        let result = async {
            self.store.event(&task,"model_registration","runtime",json!({"registration":model_registration,"weight_identity_verified":false}))?;
            let policy = TaskPolicy::agent(&task)?;
            policy.authorize(&task,"jev-r","llm.generate")?;
            self.store.event(&task,"agent_admission","jev-plus",json!({"format_version":1,"request_hash":sha256(&bytes),"prompt_bytes":bytes.len(),"reserved_output_tokens":tokens,"calls":if self.cfg.agent_collective { crate::reasoning::mode_limits(mode)?.0.min(self.cfg.max_calls) } else { 1 },"cycles":if self.cfg.agent_collective { crate::reasoning::mode_limits(mode)?.1.min(self.cfg.max_cycles) } else { 1 },"collective":self.cfg.agent_collective,"mode":mode,"session_budget_scope":"request","max_wall_seconds":self.cfg.max_wall_seconds,"model":self.cfg.model,"client_tools_executed":false,"verification":"unverified"}))?;
            self.store.event(
                &task,
                "agent_client_capabilities",
                "jev-plus",
                client_capability_audit(&request)?,
            )?;
            self.store.event(&task,"policy","security-kernel",json!({"actor":"jev-r","capability":"llm.generate","allowed":true}))?;
            if start.elapsed() >= Duration::from_secs(self.cfg.max_wall_seconds) {
                self.store.finish_task(&task,"exhausted","")?; guard.finished=true;
                bail!("Agent turn deadline exhausted before dispatch")
            }
            let completion = tokio::select! {
                biased;
                _ = cancel => { self.store.finish_task(&task,"cancelled","")?; guard.finished=true; bail!("Agent turn cancelled") },
                result = tokio::time::timeout_at(tokio::time::Instant::from_std(start + Duration::from_secs(self.cfg.max_wall_seconds)),async {
                    let deadline = start + Duration::from_secs(self.cfg.max_wall_seconds);
                    self.dispatch(backend.clone(), &task, &request, tokens, deadline).await
                }) => match result {
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
        if let Some(process) = process {
            if let Err(error) = process.shutdown().await {
                self.store.finish_task(&task, "failed", "")?;
                guard.finished = true;
                return Err(error);
            }
        }
        if result.is_err() && !guard.finished {
            self.store.finish_task(&task, "failed", "")?;
            guard.finished = true;
        }
        result
    }
}

/// Record only hashes of client supplied descriptions and schemas. The catalog
/// is advisory model input: the client retains execution and the kernel grants
/// no tool rights based on these declarations.
fn client_capability_audit(request: &Turn) -> Result<serde_json::Value> {
    let functions = request
        .tools
        .iter()
        .map(|tool| {
            Ok(json!({
                "name": tool.function.name,
                "description_sha256": sha256(tool.function.description.as_bytes()),
                "schema_sha256": sha256(&serde_json::to_vec(&tool.function.parameters)?),
                "strict": tool.function.strict,
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let catalog_hash = sha256(&serde_json::to_vec(&functions)?);
    Ok(json!({
        "format_version": 1,
        "catalog_count": functions.len(),
        "catalog_sha256": catalog_hash,
        "functions": functions,
        "catalog_source": "client_supplied_untrusted_metadata",
        "client_owns_execution": true,
        "kernel_permissions_granted": false,
        "client_results_verified": false,
    }))
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
mod collective_tests;
#[cfg(test)]
mod session_context_tests;
#[cfg(test)]
mod sessions_tests;
#[cfg(test)]
mod tests;
