use super::*;
use std::future::Future;
impl ResourcePool {
    pub(super) fn require_running(&self, task: &str) -> Result<()> {
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
        resolve_target: impl FnOnce() -> Result<Option<crate::backend::InferenceTarget>>,
        invoke: impl FnOnce() -> F,
    ) -> Result<(T, f64)> {
        self.execute_inner(task, role, allocation, false, resolve_target, invoke)
            .await
    }
    pub(crate) async fn execute_profiled<T, F: Future<Output = Result<T>>>(
        self: &Arc<Self>,
        task: &str,
        role: &str,
        fallback_allocation: Allocation,
        resolve_target: impl FnOnce() -> Result<Option<crate::backend::InferenceTarget>>,
        invoke: impl FnOnce() -> F,
    ) -> Result<(T, f64)> {
        self.execute_inner(
            task,
            role,
            fallback_allocation,
            true,
            resolve_target,
            invoke,
        )
        .await
    }
    async fn execute_inner<T, F: Future<Output = Result<T>>>(
        self: &Arc<Self>,
        task: &str,
        role: &str,
        mut allocation: Allocation,
        profiled: bool,
        resolve_target: impl FnOnce() -> Result<Option<crate::backend::InferenceTarget>>,
        invoke: impl FnOnce() -> F,
    ) -> Result<(T, f64)> {
        let actor = crate::fabric::protocol::actor(role)?;
        security::TaskPolicy::reasoning(task)?.authorize(task, actor, "llm.generate")?;
        self.require_running(task)?;
        let reservation_id = uuid::Uuid::new_v4().to_string();
        let profile_digest = if profiled {
            let digest = self
                .working_profiles_sha256
                .as_deref()
                .ok_or_else(|| anyhow!("Working profile digest is unavailable"))?;
            self.store.event(
                task,
                "resource_profile_intent",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":self.id,"reservation_id":reservation_id,
                    "role":role,"working_profiles_sha256":digest,"permissions_granted":false,
                }),
            )?;
            Some(digest)
        } else {
            None
        };
        if !profiled && !self.residents.is_empty() {
            self.store.event(task,"resource_target_intent","jev-plus",json!({
                "format_version":1,"pool_id":self.id,"reservation_id":reservation_id,"role":role,
                "resident_catalog_sha256":self.resident_catalog_sha256,"permissions_granted":false,
            }))?;
        }
        let resolved_target = if profiled || !self.residents.is_empty() {
            Some(resolve_target())
        } else {
            None
        };
        let selected_profile = if profiled {
            let raw_target = resolved_target
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .and_then(|target| target.clone());
            let canonical_target = match raw_target {
                Some(crate::backend::InferenceTarget::LocalHttp(target)) => target
                    .canonical()
                    .map(crate::backend::InferenceTarget::LocalHttp)
                    .map(Some),
                Some(crate::backend::InferenceTarget::Native(target)) => target
                    .canonical()
                    .map(crate::backend::InferenceTarget::Native)
                    .map(Some),
                Some(crate::backend::InferenceTarget::ManagedNative(target)) => target
                    .canonical()
                    .map(crate::backend::InferenceTarget::ManagedNative)
                    .map(Some),
                Some(crate::backend::InferenceTarget::Deterministic) | None => Ok(None),
            };
            let target = canonical_target
                .as_ref()
                .ok()
                .and_then(|target| target.clone());
            let profile = match resolved_target.as_ref() {
                Some(Err(_)) | None => {
                    Err(anyhow!("Profiled inference target metadata is unavailable"))
                }
                Some(Ok(_)) => match &canonical_target {
                    Err(_) => Err(anyhow!("Profiled inference target metadata is invalid")),
                    Ok(Some(crate::backend::InferenceTarget::LocalHttp(target))) => {
                        self.find_working_profile(role, target)
                    }
                    Ok(Some(crate::backend::InferenceTarget::Native(target))) => {
                        let resource_target = target.resource_target().map_err(|_| {
                            anyhow!("Profiled inference target metadata is invalid")
                        })?;
                        self.find_working_profile(role, &resource_target)
                    }
                    Ok(Some(crate::backend::InferenceTarget::ManagedNative(target))) => {
                        let resource_target = target.resource_target().map_err(|_| {
                            anyhow!("Profiled inference target metadata is invalid")
                        })?;
                        self.find_working_profile(role, &resource_target)
                    }
                    Ok(Some(crate::backend::InferenceTarget::Deterministic)) | Ok(None) => Err(
                        anyhow!("Configured working profiles require a known local model target"),
                    ),
                },
            };
            let allowed = profile.is_ok();
            let metadata_error =
                resolved_target.as_ref().is_none_or(Result::is_err) || canonical_target.is_err();
            let reason_code = if allowed {
                "configured_profile_match"
            } else if metadata_error {
                "profile_metadata_error"
            } else {
                "unknown_or_unlisted_target"
            };
            let profile_id = profile.as_ref().ok().map(|item| item.id.clone());
            let selected_allocation = profile.as_ref().ok().map(|item| item.allocation);
            self.store.event(
                task,
                "resource_profile",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":self.id,"reservation_id":reservation_id,
                    "role":role,"working_profiles_sha256":profile_digest,"target":target,
                    "profile_id":profile_id,"allocation":selected_allocation,
                    "allowed":allowed,"reason_code":reason_code,"permissions_granted":false,
                    "physical_limits_enforced":false,
                }),
            )?;
            let profile =
                profile.map_err(|_| anyhow!("Configured working profile admission denied"))?;
            allocation = profile.allocation;
            Some((target, profile))
        } else {
            None
        };
        if !self.residents.is_empty() {
            if profiled {
                self.store.event(task,"resource_target_intent","jev-plus",json!({
                    "format_version":1,"pool_id":self.id,"reservation_id":reservation_id,"role":role,
                    "resident_catalog_sha256":self.resident_catalog_sha256,"permissions_granted":false,
                }))?;
            }
            let target = if let Some((target, _)) = selected_profile.as_ref() {
                Ok(target.clone())
            } else {
                match resolved_target
                    .as_ref()
                    .ok_or_else(|| anyhow!("Resident target was not resolved"))?
                {
                    Ok(target) => Ok(target.clone()),
                    Err(error) => Err(anyhow!(error.to_string())),
                }
            };
            let coverage = target.and_then(|target| self.resident_coverage(target));
            self.store.event(task,"resource_target","jev-plus",json!({
                "format_version":1,"pool_id":self.id,"reservation_id":reservation_id,"role":role,
                "resident_catalog_sha256":self.resident_catalog_sha256,"allowed":coverage.is_ok(),
                "coverage":coverage.as_ref().ok(),"reason_code":if coverage.is_ok(){"configured_target_covered"}else{"unavailable_or_unlisted_target"},
                "permissions_granted":false,"weights_attested":false,
            }))?;
            coverage?;
        }
        let priority = self.settings.scheduling.as_ref().map(|p| p.priority(role));
        let mut intent = json!({
            "format_version":1,"pool_id":self.id,"settings":self.settings,"reservation_id":reservation_id,
            "allocation":allocation,"role":role,"permissions_granted":false,
            "resident_allocated":self.resident_allocated,"resident_catalog_sha256":self.resident_catalog_sha256,"residency_attested":false,
            "physical_limits_enforced":false,
        });
        if let Some(priority) = priority {
            intent["queue_priority"] = json!(priority);
        }
        if let Some(observation) = &self.gpu_observation {
            intent["gpu_observation"] = serde_json::to_value(observation)?;
        }
        self.store
            .event(task, "resource_intent", "jev-plus", intent)?;
        let start = Instant::now();
        let lease = match self
            .acquire_priority(
                task,
                allocation,
                reservation_id.clone(),
                priority.unwrap_or(0),
            )
            .await
        {
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
        let mut gpu_sampled_at = None;
        // Queue waiting may outlive a task's terminal transition even when a
        // trusted direct caller retained its generation future.
        self.require_running(task)?;
        if let Some(observation) = &self.gpu_observation {
            let sample_started = Instant::now();
            let sampled = self.observe_gpu(observation.sensor).await;
            let sampled_at = Instant::now();
            let sample_query_elapsed_ms = sample_started.elapsed().as_secs_f64() * 1000.0;
            let sample_age_ms_at_observation = sampled_at.elapsed().as_secs_f64() * 1000.0;
            let decision = sampled
                .as_ref()
                .map_err(|_| "gpu_observation_unavailable")
                .and_then(|sample| validate_gpu_observation(observation, allocation, sample));
            match decision {
                Ok(metrics) => {
                    gpu_sampled_at = Some(sampled_at);
                    self.store.event(
                        task,
                        "resource_gpu_observation",
                        "jev-plus",
                        json!({
                            "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
                            "role":role,"sensor":observation.sensor.name(),"device_uuid":observation.device_uuid,
                            "allocation":allocation,"headroom_bytes":observation.headroom_bytes,
                            "memory_total_bytes":metrics.total,"memory_used_bytes":metrics.used,
                            "memory_available_bytes":metrics.available,"minimum_available_bytes":metrics.required,
                            "sample_query_elapsed_ms":sample_query_elapsed_ms,
                            "sample_age_ms_at_observation":sample_age_ms_at_observation,
                            "sampled":true,"provider_dispatched":false,
                            "physical_limits_enforced":false,
                        }),
                    )?;
                }
                Err(reason_code) => {
                    self.store.event(
                        task,
                        "resource_refused",
                        "jev-plus",
                        json!({
                            "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
                            "role":role,"reason_code":reason_code,"provider_dispatched":false,
                            "wait_ms":start.elapsed().as_secs_f64()*1000.0,
                            "gpu_observation":observation,"allocation":allocation,
                        }),
                    )?;
                    bail!("GPU observation admission denied")
                }
            }
            // The outer task deadline/cancellation may have fired while the
            // bounded observation was running. Keep the reservation pre-dispatch.
            self.require_running(task)?;
        }
        let mut admitted = json!({
            "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
            "allocation":allocation,"role":role,"wait_ms":wait_ms,
            "provider_dispatched":false,
        });
        if let Some(priority) = priority {
            let bypasses = self.lock()?.entries[&lease.id].bypasses;
            admitted["queue_priority"] = json!(priority);
            admitted["queue_bypasses"] = json!(bypasses);
        }
        self.store
            .event(task, "resource_admitted", "jev-plus", admitted)?;
        if self.require_running(task).is_err() {
            self.store.event(
                task,
                "resource_refused",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
                    "role":role,"reason_code":"task_not_running","provider_dispatched":false,
                    "wait_ms":start.elapsed().as_secs_f64()*1000.0,
                }),
            )?;
            bail!("Resource scheduling requires an existing running task")
        }
        let mut dispatch = json!({
            "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
            "provider_stop_proven":false,
        });
        if let Some(sampled_at) = gpu_sampled_at {
            dispatch["sample_age_ms_at_dispatch_audit"] =
                json!(sampled_at.elapsed().as_secs_f64() * 1000.0);
        }
        self.store
            .event(task, "resource_dispatch", "jev-plus", dispatch)?;
        lease.dispatched()?;
        let dispatch_refusal = if gpu_sampled_at
            .is_some_and(|sampled_at| sampled_at.elapsed() > Duration::from_secs(2))
        {
            Some("gpu_observation_stale")
        } else if self.require_running(task).is_err() {
            Some("task_not_running")
        } else {
            None
        };
        if let Some(reason_code) = dispatch_refusal {
            self.store.event(
                task,
                "resource_outcome_unknown",
                "jev-plus",
                json!({
                    "format_version":1,"pool_id":self.id,"reservation_id":lease.id,
                    "provider_stop_proven":false,"reservation_retained":true,
                    "provider_invoked":false,"reason_code":reason_code,
                    "sample_age_ms_at_recheck":gpu_sampled_at.map(|sampled_at| sampled_at.elapsed().as_secs_f64()*1000.0),
                }),
            )?;
            bail!("Resource dispatch became stale or inactive before provider invocation")
        }
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
