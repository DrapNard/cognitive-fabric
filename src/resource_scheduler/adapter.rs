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
impl ScheduledBackend {
    fn require_running(&self, task: &str) -> Result<()> {
        let db = self.pool.store.lock()?;
        let running: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 AND status='running')",
            [task],
            |r| r.get(0),
        )?;
        if !running {
            bail!("Resource scheduling requires an existing running task")
        }
        Ok(())
    }
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
            self.require_running(task)?;
            let reservation_id = uuid::Uuid::new_v4().to_string();
            self.pool.store.event(
                task,
                "resource_intent",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":self.pool.id,"settings":self.pool.settings,"reservation_id":reservation_id,
                    "allocation":self.allocation,"role":role,"permissions_granted":false,
                    "physical_limits_enforced":false,
                }),
            )?;
            let start = Instant::now();
            let lease = match self
                .pool
                .acquire(task, self.allocation, reservation_id.clone())
                .await
            {
                Ok(lease) => lease,
                Err(error) => {
                    self.pool.store.event(task,"resource_refused","jev-plus",json!({
                        "format_version":1,"pool_id":self.pool.id,"role":role,"reservation_id":reservation_id,
                        "wait_ms":start.elapsed().as_secs_f64()*1000.0,"provider_dispatched":false,
                    }))?;
                    return Err(error);
                }
            };
            let wait_ms = start.elapsed().as_secs_f64() * 1000.0;
            // Queue waiting may outlive a task's terminal transition even when a
            // trusted direct caller retained its generation future.
            self.require_running(task)?;
            self.pool.store.event(
                task,
                "resource_admitted",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":self.pool.id,"reservation_id":lease.id,
                    "allocation":self.allocation,"role":role,"wait_ms":wait_ms,
                    "provider_dispatched":false,
                }),
            )?;
            self.pool.store.event(
                task,
                "resource_dispatch",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":self.pool.id,"reservation_id":lease.id,
                    "provider_stop_proven":false,
                }),
            )?;
            lease.dispatched()?;
            // Dropping this future after dispatch retains the allocation. Neither a
            // client timeout nor an arbitrary backend error proves provider shutdown.
            let outcome = tokio::time::timeout(
                Duration::from_millis(self.pool.settings.max_execution_ms),
                self.backend.generate_for(task, role, prompt, tokens),
            )
            .await
            .unwrap_or_else(|_| Err(anyhow!("Resource execution deadline exhausted")));
            match outcome {
                Ok(mut report) => {
                    self.pool.store.event(
                        task,
                        "resource_completed",
                        "jev-plus",
                        json!({
                            "format_version":1,"pool_id":self.pool.id,"reservation_id":lease.id,
                            "provider_request_completed":true,"physical_limits_enforced":false,
                        }),
                    )?;
                    lease.release()?;
                    report.queue_wait_ms = Some(wait_ms);
                    Ok(report)
                }
                Err(error) => {
                    self.pool.store.event(
                        task,
                        "resource_outcome_unknown",
                        "jev-plus",
                        json!({
                            "format_version":1,"pool_id":self.pool.id,"reservation_id":lease.id,
                            "provider_stop_proven":false,"reservation_retained":true,
                        }),
                    )?;
                    Err(error)
                }
            }
        })
    }
}
