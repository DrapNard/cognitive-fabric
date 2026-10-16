use super::*;
use std::future::Future;
impl ResourcePool {
    fn require_running(&self, task: &str) -> Result<()> {
        let db = self.store.lock()?;
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
    pub(crate) async fn execute<T, F: Future<Output = Result<T>>>(
        self: &Arc<Self>,
        task: &str,
        role: &str,
        allocation: Allocation,
        invoke: impl FnOnce() -> F,
    ) -> Result<(T, f64)> {
        let actor = crate::fabric::protocol::actor(role)?;
        security::TaskPolicy::reasoning(task)?.authorize(task, actor, "llm.generate")?;
        self.require_running(task)?;
        let reservation_id = uuid::Uuid::new_v4().to_string();
        self.store.event(
                task,
                "resource_intent",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":self.id,"settings":self.settings,"reservation_id":reservation_id,
                    "allocation":allocation,"role":role,"permissions_granted":false,
                    "resident_allocated":self.resident_allocated,"resident_catalog_sha256":self.resident_catalog_sha256,"residency_attested":false,
                    "physical_limits_enforced":false,
                }),
            )?;
        let start = Instant::now();
        let lease = match self.acquire(task, allocation, reservation_id.clone()).await {
            Ok(lease) => lease,
            Err(error) => {
                self.store.event(task,"resource_refused","jev-plus",json!({
                        "format_version":1,"pool_id":self.id,"role":role,"reservation_id":reservation_id,
                        "wait_ms":start.elapsed().as_secs_f64()*1000.0,"provider_dispatched":false,
                    }))?;
                return Err(error);
            }
        };
        let wait_ms = start.elapsed().as_secs_f64() * 1000.0;
        // Queue waiting may outlive a task's terminal transition even when a
        // trusted direct caller retained its generation future.
        self.require_running(task)?;
        self.store.event(
            task,
            "resource_admitted",
            "jev-plus",
            json!({
                "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
                "allocation":allocation,"role":role,"wait_ms":wait_ms,
                "provider_dispatched":false,
            }),
        )?;
        self.store.event(
            task,
            "resource_dispatch",
            "jev-plus",
            json!({
                "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
                "provider_stop_proven":false,
            }),
        )?;
        lease.dispatched()?;
        // Dropping this future after dispatch retains the allocation. Neither a
        // client timeout nor an arbitrary backend error proves provider shutdown.
        let outcome = tokio::time::timeout(
            Duration::from_millis(self.settings.max_execution_ms),
            invoke(),
        )
        .await
        .unwrap_or_else(|_| Err(anyhow!("Resource execution deadline exhausted")));
        match outcome {
            Ok(report) => {
                self.store.event(
                    task,
                    "resource_completed",
                    "jev-plus",
                    json!({
                        "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
                        "provider_request_completed":true,"physical_limits_enforced":false,
                    }),
                )?;
                lease.release()?;
                Ok((report, wait_ms))
            }
            Err(error) => {
                self.store.event(
                    task,
                    "resource_outcome_unknown",
                    "jev-plus",
                    json!({
                        "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
                        "provider_stop_proven":false,"reservation_retained":true,
                    }),
                )?;
                Err(error)
            }
        }
    }
}
