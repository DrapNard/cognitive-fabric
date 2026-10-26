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
    fn prepare_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let actor = crate::fabric::protocol::actor(role)?;
            security::TaskPolicy::reasoning(task)?.authorize(task, actor, "llm.generate")?;
            self.pool.require_running(task)?;
            if self.pool.lock()?.closed || prompt.len() > 32_000 {
                bail!("Scheduled inference route is unavailable for preparation")
            }
            // Preparation owns no prediction allocation. Native model loading
            // retains its separate lifecycle/GPU admission; generate_for still
            // admits this exact inference through the configured resource pool.
            self.backend.prepare_for(task, role, prompt).await
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
            report.queue_wait_ms = Some(wait_ms);
            Ok(report)
        })
    }
}
