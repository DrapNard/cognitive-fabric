use super::*;
use crate::{
    backend::Generation,
    fabric::{Availability, ModuleDescriptor},
};
use std::{future::Future, pin::Pin};

pub(super) struct ScheduledBackend {
    pub pool: Arc<ResourcePool>,
    pub backend: Arc<dyn InferenceBackend>,
    pub allocation: Allocation,
    pub use_working_profiles: bool,
}
impl InferenceBackend for ScheduledBackend {
    fn requires_preparation(&self, role: &str) -> Result<bool> {
        self.backend.requires_preparation(role)
    }
    fn prepare_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if !self.backend.requires_preparation(role)? {
                return Ok(());
            }
            let actor = crate::fabric::protocol::actor(role)?;
            security::TaskPolicy::reasoning(task)?.authorize(task, actor, "llm.generate")?;
            self.pool.require_running(task)?;
            if self.pool.lock()?.closed || prompt.len() > 32_000 {
                bail!("Scheduled inference route is unavailable for preparation")
            }
            self.pool
                .prepare_route(
                    task,
                    role,
                    self.allocation,
                    self.use_working_profiles,
                    || self.backend.inference_target(role),
                    || self.backend.prepare_for(task, role, prompt),
                )
                .await?;
            Ok(())
        })
    }
    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        self.backend.input_token_admission()
    }
    fn inference_target(&self, role: &str) -> Result<Option<crate::backend::InferenceTarget>> {
        self.backend.inference_target(role)
    }
    fn availability(&self) -> Availability {
        if self.pool.lock().map_or(true, |state| state.closed) {
            Availability::Closed
        } else {
            self.backend.availability()
        }
    }
    fn module_descriptor(&self) -> Option<&ModuleDescriptor> {
        self.backend.module_descriptor()
    }
    fn modules(&self) -> Result<Vec<ModuleDescriptor>> {
        self.backend.modules()
    }
    fn pin(&self) -> Result<Option<Arc<dyn InferenceBackend>>> {
        Ok(Some(Arc::new(Self {
            pool: self.pool.clone(),
            backend: self.backend.pin()?.unwrap_or_else(|| self.backend.clone()),
            allocation: self.allocation,
            use_working_profiles: self.use_working_profiles,
        })))
    }
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async { bail!("Resource-scheduled inference requires an explicit existing task") })
    }
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            let actor = crate::fabric::protocol::actor(role)?;
            security::TaskPolicy::reasoning(task)?.authorize(task, actor, "llm.generate")?;
            if prompt.len() > 32_000 || !(1..=16_384).contains(&tokens) {
                bail!("Inference request exceeds limits")
            }
            // Freeze dynamic routes before checking target metadata, so a registry
            // replacement while waiting cannot substitute an unaccounted backend.
            let backend = if self.use_working_profiles || !self.pool.residents.is_empty() {
                self.pool.require_running(task)?;
                self.backend.pin()?.unwrap_or_else(|| self.backend.clone())
            } else {
                self.backend.clone()
            };
            let (mut report, wait_ms) = if self.use_working_profiles {
                self.pool
                    .execute_profiled(
                        task,
                        role,
                        self.allocation,
                        || backend.inference_target(role),
                        || backend.generate_for(task, role, prompt, tokens),
                    )
                    .await?
            } else {
                self.pool
                    .execute(
                        task,
                        role,
                        self.allocation,
                        || backend.inference_target(role),
                        || backend.generate_for(task, role, prompt, tokens),
                    )
                    .await?
            };
            let queue_wait_ms = report.queue_wait_ms.unwrap_or(0.0) + wait_ms;
            report.queue_wait_ms = (queue_wait_ms <= 300_000.0).then_some(queue_wait_ms);
            Ok(report)
        })
    }
    fn generate_for_with_shared_admission<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        tokens: usize,
        shared_limit: Option<Arc<tokio::sync::Semaphore>>,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            let actor = crate::fabric::protocol::actor(role)?;
            security::TaskPolicy::reasoning(task)?.authorize(task, actor, "llm.generate")?;
            if prompt.len() > 32_000 || !(1..=16_384).contains(&tokens) {
                bail!("Inference request exceeds limits")
            }
            let backend = if self.use_working_profiles || !self.pool.residents.is_empty() {
                self.pool.require_running(task)?;
                self.backend.pin()?.unwrap_or_else(|| self.backend.clone())
            } else {
                self.backend.clone()
            };
            let generate = || {
                backend.generate_for_with_shared_admission(
                    task,
                    role,
                    prompt,
                    tokens,
                    shared_limit.clone(),
                )
            };
            let (mut report, wait_ms) = if self.use_working_profiles {
                self.pool
                    .execute_profiled(
                        task,
                        role,
                        self.allocation,
                        || backend.inference_target(role),
                        generate,
                    )
                    .await?
            } else {
                self.pool
                    .execute(
                        task,
                        role,
                        self.allocation,
                        || backend.inference_target(role),
                        generate,
                    )
                    .await?
            };
            let queue_wait_ms = report.queue_wait_ms.unwrap_or_default() + wait_ms;
            report.queue_wait_ms = (queue_wait_ms <= 300_000.0).then_some(queue_wait_ms);
            Ok(report)
        })
    }
}
