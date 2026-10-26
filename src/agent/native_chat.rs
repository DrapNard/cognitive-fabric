//! Task-scoped OpenAI-compatible conversation dispatch through existing managed receipts.
//!
//! Standard client tool definitions and proposals remain on the OpenAI-compatible
//! route. The response is checked by model key and fresh inventory evidence only;
//! this protocol does not attest the exact responding model instance.
use super::{
    native_scope::{NativeClientScope, NativeModelKeyAndInventory},
    ChatBackend, Completion, Config, Turn,
};
use anyhow::{bail, Result};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

pub(crate) struct ScopedChat {
    inner: Arc<dyn ChatBackend>,
    cfg: Config,
    scope: Arc<NativeClientScope>,
    target: crate::backend::ModelTarget,
    uncertain: Arc<AtomicBool>,
    admission_gate: Arc<Mutex<()>>,
}

impl ScopedChat {
    pub(crate) fn new(
        inner: Arc<dyn ChatBackend>,
        cfg: Config,
        scope: Arc<NativeClientScope>,
    ) -> Result<Self> {
        scope.validate_config(&cfg)?;
        if !scope.model_key_evidence_approved() {
            bail!("Scoped client inference requires an explicitly pinned model-key and inventory evidence policy")
        }
        let target = local_target(inner.as_ref())?;
        validate_target(&target, &scope)?;
        Ok(Self {
            inner,
            cfg,
            scope,
            target,
            uncertain: Arc::new(AtomicBool::new(false)),
            admission_gate: Arc::new(Mutex::new(())),
        })
    }

    fn with_route(&self, inner: Arc<dyn ChatBackend>, cfg: Config, role: &str) -> Result<Self> {
        let target = local_target(inner.as_ref())?;
        let (expected_model, _) = cfg.model_settings(role)?;
        cfg.require_role_model(role, &target.model)?;
        if target.model != expected_model {
            bail!("Native routed model differs from the operator-configured role model")
        }
        validate_target(&target, &self.scope)?;
        Ok(Self {
            inner,
            cfg,
            scope: self.scope.clone(),
            target,
            uncertain: self.uncertain.clone(),
            admission_gate: self.admission_gate.clone(),
        })
    }

    fn dispatch<'a>(
        &'a self,
        task: &'a str,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async move {
            if task != self.scope.root_task_id() {
                bail!("Native client backend cannot dispatch for another task")
            }
            if self.uncertain.load(Ordering::Acquire) {
                bail!("A prior native client generation has an unknown outcome")
            }
            self.scope.ensure_model(&self.target.model).await?;
            let _generation = self.scope.generation_guard().await?;
            if self.uncertain.load(Ordering::Acquire) {
                bail!("A native client generation became unknown during activation")
            }
            let before = self
                .scope
                .validate_receipt_for(&self.target.model, &self.target.api_base)
                .await?;
            if self.uncertain.load(Ordering::Acquire) {
                bail!("A concurrent native client generation has an unknown outcome")
            }
            let call_id = uuid::Uuid::new_v4().to_string();
            let mut guard = GenerationGuard::start(
                self.scope.clone(),
                &before,
                &call_id,
                self.uncertain.clone(),
                self.admission_gate.clone(),
            )?;
            let deadline = self.scope.deadline()?;
            if std::time::Instant::now() >= deadline {
                bail!("Native client lifecycle deadline exhausted before inference")
            }
            let completion = self.inner.complete_for(task, request, tokens).await?;
            if completion.model != self.target.model {
                bail!("OpenAI-compatible response model differs from the pinned client route")
            }
            let after = self
                .scope
                .validate_receipt_for(&self.target.model, &self.target.api_base)
                .await?;
            if serde_json::to_value(&before.receipt)? != serde_json::to_value(&after.receipt)? {
                bail!("Managed receipt changed across the client inference call")
            }
            if std::time::Instant::now() >= self.scope.deadline()? {
                bail!("Native client lifecycle deadline exhausted after inference")
            }
            // Keep the client protocol intact: tool calls are validated by the
            // existing collective checkpoint and remain client-owned proposals.
            guard.confirm()?;
            Ok(completion)
        })
    }
}

impl ChatBackend for ScopedChat {
    fn prepare_for<'a>(
        &'a self,
        task: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if task != self.scope.root_task_id() {
                bail!("Native client preparation cannot activate for another task")
            }
            if self.uncertain.load(Ordering::Acquire) {
                bail!("A prior native client generation has an unknown outcome")
            }
            self.scope.ensure_model(&self.target.model).await?;
            let _generation = self.scope.generation_guard().await?;
            if self.uncertain.load(Ordering::Acquire) {
                bail!("A native client generation became unknown during preparation")
            }
            Ok(())
        })
    }

    fn for_role(&self, role: &str) -> Result<Option<Arc<dyn ChatBackend>>> {
        let Some(inner) = self.inner.for_role(role)? else {
            return Ok(None);
        };
        Ok(Some(Arc::new(self.with_route(
            inner,
            self.cfg.clone(),
            role,
        )?)))
    }

    fn for_registered_model(
        &self,
        selection: &crate::models::Selection,
        registration: &crate::models::Registered,
        cfg: &Config,
    ) -> Result<Option<Arc<dyn ChatBackend>>> {
        registration.validate_snapshot(selection)?;
        if cfg.inference_auth_scope != self.cfg.inference_auth_scope
            || cfg.resource_admission != self.cfg.resource_admission
            || cfg.worker_process_limits != self.cfg.worker_process_limits
            || cfg.inference_process != self.cfg.inference_process
            || cfg.api_base != self.cfg.api_base
        {
            bail!("Registered client route changes an operator security or resource setting")
        }
        cfg.require_role_model("solver", &registration.manifest.model)?;
        let (configured_model, _) = cfg.model_settings("solver")?;
        if configured_model != registration.manifest.model {
            bail!("Registered expert model differs from the task-pinned solver configuration")
        }
        let Some(inner) = self
            .inner
            .for_registered_model(selection, registration, cfg)?
        else {
            return Ok(None);
        };
        let target = local_target(inner.as_ref())?;
        if target.model != registration.manifest.model {
            bail!("Registered route differs from its pinned model manifest")
        }
        validate_target(&target, &self.scope)?;
        Ok(Some(Arc::new(Self {
            inner,
            cfg: cfg.clone(),
            scope: self.scope.clone(),
            target,
            uncertain: self.uncertain.clone(),
            admission_gate: self.admission_gate.clone(),
        })))
    }

    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        self.inner.input_token_admission()
    }

    fn pin(self: Arc<Self>) -> Result<Option<Arc<dyn ChatBackend>>> {
        let Some(inner) = self.inner.clone().pin()? else {
            return Ok(None);
        };
        let target = local_target(inner.as_ref())?;
        if target != self.target {
            bail!("Pinned native client route changed its selected endpoint or model")
        }
        Ok(Some(Arc::new(Self {
            inner,
            cfg: self.cfg.clone(),
            scope: self.scope.clone(),
            target,
            uncertain: self.uncertain.clone(),
            admission_gate: self.admission_gate.clone(),
        })))
    }

    fn inference_target(&self) -> Result<Option<crate::backend::InferenceTarget>> {
        Ok(Some(self.scope.managed_target(&self.target.model)?))
    }

    fn complete_for<'a>(
        &'a self,
        task: &'a str,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        self.dispatch(task, request, tokens)
    }

    fn complete<'a>(
        &'a self,
        _request: &'a Turn,
        _tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async { bail!("Managed client inference requires an exact task identity") })
    }
}

struct GenerationGuard {
    scope: Arc<NativeClientScope>,
    model: String,
    receipt_id: String,
    call_id: String,
    uncertain: Arc<AtomicBool>,
    admission_gate: Arc<Mutex<()>>,
    terminal: bool,
}

impl GenerationGuard {
    fn start(
        scope: Arc<NativeClientScope>,
        evidence: &NativeModelKeyAndInventory,
        call_id: &str,
        uncertain: Arc<AtomicBool>,
        admission_gate: Arc<Mutex<()>>,
    ) -> Result<Self> {
        if evidence.policy_sha256 != scope.policy_sha256()
            || evidence.evidence != "model_key_and_inventory_only"
            || evidence.inventory_sha256.len() != 64
        {
            bail!("Native receipt admission lacks explicit model-key and inventory evidence")
        }
        let _admission = admission_gate.lock().map_err(|_| {
            uncertain.store(true, Ordering::Release);
            anyhow::anyhow!("Native client admission state is poisoned; reconciliation is required")
        })?;
        if uncertain.load(Ordering::Acquire) {
            bail!("A native client generation has an unknown outcome")
        }
        let mut guard = Self {
            scope,
            model: evidence.model.clone(),
            receipt_id: evidence.receipt.receipt_id.clone(),
            call_id: call_id.to_owned(),
            uncertain,
            admission_gate: admission_gate.clone(),
            terminal: true,
        };
        if let Err(error) = guard.scope.lifecycle_event(
            "generation_started",
            &guard.model,
            &guard.receipt_id,
            &guard.call_id,
        ) {
            guard.uncertain.store(true, Ordering::Release);
            return Err(error);
        }
        guard.terminal = false;
        Ok(guard)
    }

    fn confirm(&mut self) -> Result<()> {
        let _admission = self.admission_gate.lock().map_err(|_| {
            self.uncertain.store(true, Ordering::Release);
            anyhow::anyhow!("Native client admission state is poisoned; reconciliation is required")
        })?;
        if let Err(error) = self.scope.lifecycle_event(
            "generation_confirmed",
            &self.model,
            &self.receipt_id,
            &self.call_id,
        ) {
            self.uncertain.store(true, Ordering::Release);
            return Err(error);
        }
        self.terminal = true;
        Ok(())
    }
}

impl Drop for GenerationGuard {
    fn drop(&mut self) {
        if !self.terminal {
            let _admission = self
                .admission_gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.uncertain.store(true, Ordering::Release);
            let _ = self.scope.lifecycle_event(
                "generation_outcome_unknown",
                &self.model,
                &self.receipt_id,
                &self.call_id,
            );
        }
    }
}

fn local_target(backend: &dyn ChatBackend) -> Result<crate::backend::ModelTarget> {
    match backend.inference_target()? {
        Some(crate::backend::InferenceTarget::LocalHttp(target)) => target.canonical(),
        _ => bail!("Managed client route requires an exact OpenAI-compatible local target"),
    }
}

fn validate_target(target: &crate::backend::ModelTarget, scope: &NativeClientScope) -> Result<()> {
    scope.managed_target(&target.model)?;
    let expected = url::Url::parse(scope.endpoint_for_validation())?;
    let actual = url::Url::parse(&target.api_base)?;
    if expected.origin() != actual.origin() {
        bail!("OpenAI-compatible client route is outside the pinned native endpoint origin")
    }
    Ok(())
}
