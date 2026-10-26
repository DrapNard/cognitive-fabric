//! Transport-independent, bounded solver turns for external agent clients.
//! Tool calls are proposals; execution and approval belong to the client.
mod collective;
mod expert_workflow;
mod local;
mod native_chat;
pub mod native_scope;
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
    /// Prepare this exact already-admitted task route without generating text.
    /// The default has no lifecycle work and does not attest model readiness.
    /// Managed adapters must retain their operator policy and original clock.
    fn prepare_for<'a>(
        &'a self,
        _task: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    /// Return a task-independent, pinned adapter for one trusted host-assigned role.
    /// None means role-specific routing is unsupported, never permission to guess.
    fn for_role(&self, _role: &str) -> Result<Option<Arc<dyn ChatBackend>>> {
        Ok(None)
    }
    /// Select an exact, checksum-validated local model registration for this
    /// task. Backends must opt in; callers never substitute their default route.
    fn for_registered_model(
        &self,
        _selection: &crate::models::Selection,
        _registration: &crate::models::Registered,
        _cfg: &Config,
    ) -> Result<Option<Arc<dyn ChatBackend>>> {
        Ok(None)
    }
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
    task_scoped_resources: bool,
    native_policy: Option<Arc<crate::residency::lifecycle::NativeLifecyclePolicy>>,
    native_owner: Option<Arc<crate::residency::lifecycle::Ownership>>,
    native_on_demand: bool,
    resource_pool: Option<Arc<crate::resource_scheduler::ResourcePool>>,
}
impl AgentRuntime {
    pub fn local(cfg: Config, store: Arc<Store>) -> Result<Self> {
        // Native conversation RPC is deliberately separate from role-prompt RPC.
        // Never silently bypass an operator's managed worker requirement.
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
            task_scoped_resources: false,
            native_policy: None,
            native_owner: None,
            native_on_demand: false,
            resource_pool,
        })
    }
    /// Bind existing managed model instances to each durable client session.
    /// OpenAI tool proposals identify a model key, not the generating instance;
    /// the operator must explicitly accept that weaker evidence. No model is
    /// loaded, released or retried by this opt-in path.
    pub fn with_existing_native_models(
        mut self,
        policy: crate::residency::lifecycle::NativeLifecyclePolicy,
        human_approve: bool,
        allow_model_key_evidence: bool,
    ) -> Result<Self> {
        crate::security::authorize("human-cli", "model.residency.inspect")?;
        crate::security::authorize("human-cli", "resources.configure")?;
        if !human_approve || !allow_model_key_evidence {
            bail!(
                "Client native routing requires explicit approval of model-key/inventory evidence"
            )
        }
        if !self.collective_enabled() || self.managed {
            bail!("Client native routing requires direct collective conversation inference")
        }
        native_scope::validate_configuration(&self.cfg, &policy)?;
        let mut expected_lock = self.cfg.db().into_os_string();
        expected_lock.push(".runtime.lock");
        if self
            .store
            .runtime_lock
            .as_deref()
            .map(std::path::Path::as_os_str)
            != Some(expected_lock.as_os_str())
        {
            bail!("Native client routing requires the configured persistent workspace database")
        }
        self.native_policy = Some(Arc::new(policy));
        self.native_on_demand = false;
        self.resource_pool = None;
        self.task_scoped_resources = true;
        Ok(self)
    }
    /// Activate only the operator-approved model selected by an admitted call.
    /// The same explicit OpenAI evidence approval and durable session ceilings
    /// apply; activation failures are never retried automatically.
    pub fn with_on_demand_native_models(
        self,
        policy: crate::residency::lifecycle::NativeLifecyclePolicy,
        human_approve: bool,
        allow_model_key_evidence: bool,
    ) -> Result<Self> {
        crate::security::authorize("human-cli", "model.residency.change")?;
        let mut selected =
            self.with_existing_native_models(policy, human_approve, allow_model_key_evidence)?;
        selected.native_on_demand = true;
        Ok(selected)
    }
    /// Return a task-local runtime for a model registration captured at the
    /// session root. It shares the original Store, resource pool, managed-worker
    /// policy and durable session ledger; it performs no registry reread.
    pub(crate) fn for_registered_model(
        &self,
        selection: &crate::models::Selection,
        registration: &crate::models::Registered,
    ) -> Result<Option<Self>> {
        registration.validate_snapshot(selection)?;
        if !registration.enabled || registration.manifest.max_output_tokens < 128 {
            return Ok(None);
        }
        // Check the language-only boundary against the original role map before
        // deriving the expert route, so profile pruning cannot bypass it.
        self.cfg
            .require_role_model("solver", &registration.manifest.model)?;
        let cfg = crate::models::configuration_from_snapshot(&self.cfg, selection, registration)?;
        if cfg.inference_auth_scope != self.cfg.inference_auth_scope
            || cfg.resource_admission != self.cfg.resource_admission
            || cfg.worker_process_limits != self.cfg.worker_process_limits
            || cfg.inference_process != self.cfg.inference_process
            || cfg.api_base != registration.manifest.api_base
            || cfg.model != registration.manifest.model
            || cfg.max_tokens > registration.manifest.max_output_tokens as usize
        {
            bail!("Registered expert route changed an operator security or resource setting")
        }
        let Some(backend) = self
            .backend
            .for_registered_model(selection, registration, &cfg)?
        else {
            return Ok(None);
        };
        if backend.input_token_admission() != cfg.input_token_admission {
            bail!("Registered expert backend cannot preserve input-token admission")
        }
        let routed = backend
            .for_role("solver")?
            .unwrap_or_else(|| backend.clone());
        let expected = crate::backend::ModelTarget {
            api_base: registration.manifest.api_base.clone(),
            model: registration.manifest.model.clone(),
        }
        .canonical()?;
        let target_matches = match routed.inference_target()? {
            Some(crate::backend::InferenceTarget::LocalHttp(target)) => target == expected,
            Some(crate::backend::InferenceTarget::ManagedNative(target)) => {
                let Some(policy) = &self.native_policy else {
                    bail!("Registered expert returned an unapproved managed native target")
                };
                let target = target.canonical()?;
                target.model == expected.model
                    && target.policy_sha256 == sha256(&serde_json::to_vec(policy.as_ref())?)
                    && url::Url::parse(&target.api_base)?.origin()
                        == url::Url::parse(&expected.api_base)?.origin()
                    && policy
                        .models
                        .iter()
                        .any(|model| model.model == target.model)
            }
            _ => false,
        };
        if !target_matches {
            bail!("Registered expert backend differs from its pinned local target and approved policy")
        }
        let mut selected = self.clone();
        selected.cfg = cfg;
        selected.backend = backend;
        Ok(Some(selected))
    }
    /// Bind native turns to the same configured budget as collective requests.
    /// Existing cloned runtimes keep their pool. Only trusted host setup calls this.
    pub fn with_resource_pool(
        mut self,
        actor: &str,
        pool: Option<Arc<crate::resource_scheduler::ResourcePool>>,
    ) -> Result<Self> {
        crate::security::authorize(actor, "resources.configure")?;
        if self.native_policy.is_some() {
            bail!("Native client model scope requires task-scoped resource ownership")
        }
        match (&self.cfg.resource_admission, &pool) {
            (None, None) => (),
            (Some(configuration), Some(pool)) if pool.matches(&self.store, configuration) => (),
            _ => bail!("Agent resource pool differs from its operator configuration"),
        }
        self.resource_pool = pool;
        self.task_scoped_resources = false;
        Ok(self)
    }
    /// Release idle client resource ownership so an approved lifecycle host can
    /// take exclusive model-mutation leases between requests. Active requests
    /// still construct the exact configured pool and validate unresolved history.
    pub fn with_task_scoped_resources(
        mut self,
        actor: &str,
        store: &Arc<Store>,
        configuration: Option<&crate::resource_scheduler::Configuration>,
    ) -> Result<Self> {
        crate::security::authorize(actor, "resources.configure")?;
        if !Arc::ptr_eq(&self.store, store) || self.cfg.resource_admission.as_ref() != configuration
        {
            bail!("Task-scoped agent resources differ from the host workspace or configuration")
        }
        self.resource_pool = None;
        self.task_scoped_resources = true;
        Ok(self)
    }
    fn collective_enabled(&self) -> bool {
        self.cfg.agent_collective || self.cfg.language_model().is_some()
    }
    fn worker_role_config(&self, role: &str) -> Result<Config> {
        let (model, temperature) = self.cfg.model_settings(role)?;
        let mut selected = self.cfg.clone();
        selected.model = model.to_owned();
        selected.temperature = temperature;
        // One-use worker delegation carries the already authorized exact target.
        // Root configuration/hash and role reservation retain the full network map.
        selected.reasoning_effort = self
            .cfg
            .role_reasoning_effort(role, None)
            .map(str::to_owned);
        selected.role_profiles.clear();
        selected.model_registration = None;
        selected.validate()?;
        Ok(selected)
    }
    pub fn validate(&self, request: &Turn) -> Result<usize> {
        if self.backend.input_token_admission() != self.cfg.input_token_admission {
            bail!("Configured agent backend cannot enforce input-token admission")
        }
        match (&self.cfg.resource_admission, &self.resource_pool) {
            (None, None) => (),
            (Some(_), None) if self.task_scoped_resources => (),
            (Some(configuration), Some(pool)) if pool.matches(&self.store, configuration) => (),
            _ => bail!("Configured agent resource pool is missing or changed"),
        }
        request.validate()?;
        if self.cfg.language_model().is_some() {
            for role in [
                "interface",
                "planner",
                "solver",
                "explorer",
                "critic",
                "reflector",
                "synthesizer",
            ] {
                if self.backend.for_role(role)?.is_none() {
                    bail!("Language-boundary conversations require explicit backend role routing")
                }
            }
        }

        if self.collective_enabled() && request.messages.len() > 254 {
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
        self.dispatch_role(backend, task, "solver", request, tokens, deadline)
            .await
    }
    async fn dispatch_role(
        &self,
        backend: Arc<dyn ChatBackend>,
        task: &str,
        role: &str,
        request: &Turn,
        tokens: usize,
        deadline: Instant,
    ) -> Result<Completion> {
        let selected = backend.for_role(role)?;
        if selected.is_none() && self.cfg.language_model().is_some() {
            bail!("Selected backend cannot honor the pinned language/reasoning role map")
        }
        let backend = selected.unwrap_or(backend);
        self.dispatch_selected_role(backend, task, role, request, tokens, deadline)
            .await
    }

    /// Select all wave routes before lifecycle preparation, then prepare them
    /// before polling any generation. Call only after the full wave reservation.
    async fn prepare_role_wave(
        &self,
        backend: Arc<dyn ChatBackend>,
        task: &str,
        roles: &[&str],
        deadline: Instant,
    ) -> Result<Vec<Arc<dyn ChatBackend>>> {
        if roles.is_empty() || roles.len() > 4 {
            bail!("Client role wave is outside its concurrency bound")
        }
        let selected = roles
            .iter()
            .map(|role| {
                crate::fabric::protocol::actor(role)?;
                let route = backend.for_role(role)?;
                if route.is_none() && self.cfg.language_model().is_some() {
                    bail!("Selected backend cannot honor the pinned language/reasoning role map")
                }
                Ok(route.unwrap_or_else(|| backend.clone()))
            })
            .collect::<Result<Vec<_>>>()?;
        // Resource admission may include resident-target restrictions and a GPU
        // observation. Keep activation inside that admitted callback until a
        // separately audited lifecycle-preparation admission exists.
        if self.resource_pool.is_some() {
            return Ok(selected);
        }
        for route in &selected {
            let timeout = Duration::from_secs(self.cfg.timeout_seconds)
                .min(deadline.saturating_duration_since(Instant::now()));
            if timeout.is_zero() {
                bail!("Client role wave deadline exhausted before preparation")
            }
            tokio::time::timeout(timeout, route.prepare_for(task))
                .await
                .map_err(|_| anyhow::anyhow!("Client role wave preparation timed out"))??;
        }
        Ok(selected)
    }

    async fn dispatch_selected_role(
        &self,
        backend: Arc<dyn ChatBackend>,
        task: &str,
        role: &str,
        request: &Turn,
        tokens: usize,
        deadline: Instant,
    ) -> Result<Completion> {
        if Instant::now() >= deadline {
            bail!("Agent deadline exhausted before backend dispatch")
        }
        let process = if self.managed && self.collective_enabled() {
            Some(Arc::new(process::ProcessTask::new(
                self.worker_role_config(role)?,
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
            // Admit the exact role target selected from the pinned operator map.
            let (completion, _) = if configuration.working_profiles.is_empty() {
                pool.execute(
                    task,
                    role,
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
                    role,
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
        if let Ok(completion) = &result {
            if self.cfg.language_model().is_some() || self.cfg.model_registration.is_some() {
                let (expected, _) = self.cfg.model_settings(role)?;
                if completion.model != expected {
                    bail!("Completion differs from its pinned role or registered model target")
                }
            }
        }
        result
    }
    pub async fn run_until(
        &self,
        request: Turn,
        cancel: impl Future<Output = ()>,
    ) -> Result<TurnResult> {
        let mut scoped;
        let runtime = if self.task_scoped_resources {
            scoped = self.clone();
            if self.native_policy.is_some() {
                scoped.native_owner = Some(crate::residency::lifecycle::Ownership::acquire(
                    self.store.clone(),
                )?);
            }
            scoped.resource_pool = self
                .cfg
                .resource_admission
                .as_ref()
                .map(|configuration| {
                    crate::resource_scheduler::ResourcePool::configured_owned(
                        self.store.clone(),
                        configuration,
                        scoped.native_owner.clone(),
                    )
                })
                .transpose()?;
            scoped.task_scoped_resources = false;
            &scoped
        } else {
            self
        };
        runtime.run_with_resources(request, cancel).await
    }
    async fn run_with_resources(
        &self,
        request: Turn,
        cancel: impl Future<Output = ()>,
    ) -> Result<TurnResult> {
        let tokens = self.validate(&request)?;
        if self.collective_enabled() {
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
