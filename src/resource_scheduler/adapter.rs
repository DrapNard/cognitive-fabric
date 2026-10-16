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
}
impl InferenceBackend for ScheduledBackend {
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
            let (mut report, wait_ms) = self
                .pool
                .execute(task, role, self.allocation, || {
                    self.backend.generate_for(task, role, prompt, tokens)
                })
                .await?;
            report.queue_wait_ms = Some(wait_ms);
            Ok(report)
        })
    }
}
