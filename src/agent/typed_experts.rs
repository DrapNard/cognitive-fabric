//! Host-pinned deterministic experts for the client collective.
//!
//! This snapshot only routes typed requests to trusted host modules already
//! registered with the broker. Frozen expert manifests can narrow those routes;
//! their declarations never register code or grant capabilities. Metadata digests
//! preserve provenance, not executable behavior or model-weight attestations.

mod goals;
mod graph_translation;
mod graphs;
mod plans;
mod providers;
pub(super) mod translation;
pub(super) use goals::Work;
pub(super) use graph_translation::Translation as GraphTranslation;
pub(super) use plans::{Entry, Plan, PlanIdentity};

use crate::{
    fabric::modules::{Broker, Capability, Grant, TaskRoutes},
    security::TaskPolicy,
};
use anyhow::{bail, Result};
use serde::Serialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_SNAPSHOT_BYTES: usize = 64 * 1024;
const MAX_MODULE_DEADLINE: Duration = Duration::from_secs(300);

/// Immutable first-turn routing snapshot for trusted, typed host experts.
///
/// The snapshot retains the exact registered implementations via broker pins.
/// It contains no prompt or model-generated authority and does not attest that
/// trusted in-process code is sandboxed.
pub(super) struct Snapshot {
    broker: Arc<Broker>,
    routes: TaskRoutes,
    digest: String,
    module_count: usize,
    broker_id: String,
    task: String,
    module_digest: String,
    providers: providers::Catalog,
}

#[derive(Serialize)]
struct ModuleMetadata<'a> {
    id: &'a str,
    version: &'a str,
    capabilities: Vec<&'static str>,
    source_fingerprint: Option<&'a str>,
    implementation_identity: Option<&'a serde_json::Value>,
    queue_capacity: usize,
    concurrency: usize,
}

#[derive(Serialize)]
struct SnapshotMetadata<'a> {
    format_version: u8,
    broker_id: &'a str,
    modules: Vec<ModuleMetadata<'a>>,
}

impl Snapshot {
    /// Capture every currently active module and capability route exactly once.
    /// Callers should invoke this while holding their task-opening transaction.
    pub(super) fn capture(
        broker: Arc<Broker>,
        task: &str,
        db: &rusqlite::Connection,
    ) -> Result<Self> {
        let (pins, routes) = broker.pin_active_routes(task)?;
        let broker_id = broker.identity_id().to_string();
        let modules = pins
            .iter()
            .map(|pin| {
                let descriptor = pin.descriptor();
                let limits = pin.limits();
                ModuleMetadata {
                    id: &descriptor.id,
                    version: &descriptor.version,
                    capabilities: descriptor
                        .capabilities
                        .iter()
                        .map(|capability| capability.as_str())
                        .collect(),
                    source_fingerprint: pin.source_fingerprint(),
                    implementation_identity: pin.implementation_identity(),
                    queue_capacity: limits.queue_capacity,
                    concurrency: limits.concurrency,
                }
            })
            .collect();
        let encoded = serde_json::to_vec(&SnapshotMetadata {
            format_version: 1,
            broker_id: &broker_id,
            modules,
        })?;
        if encoded.len() > MAX_SNAPSHOT_BYTES {
            bail!("Typed expert snapshot metadata exceeds its byte bound")
        }
        let module_digest = crate::store::sha256(&encoded);
        let providers = providers::Catalog::capture(db, &routes)?;
        let digest = crate::store::sha256(&serde_json::to_vec(&serde_json::json!({
            "format_version":2, "modules_sha256":module_digest,
            "expert_providers_sha256":providers.digest()
        }))?);
        Ok(Self {
            broker,
            routes,
            digest,
            module_count: pins.len(),
            broker_id,
            task: task.to_owned(),
            module_digest,
            providers,
        })
    }

    pub(super) fn digest(&self) -> &str {
        &self.digest
    }

    pub(super) fn module_count(&self) -> usize {
        self.module_count
    }

    pub(super) fn available(&self, capability: Capability) -> bool {
        self.routes.contains_key(&capability)
            && self.providers.resolve(capability, None, None).is_ok()
    }

    pub(super) fn bind_plan(&self, plan: &mut Plan) -> Result<bool> {
        for entry in plan.entries() {
            entry.work.request().validate()?;
        }
        if plan.routes_present() != 0 {
            if plan.routes_present() != plan.len() {
                bail!("Typed plan has partially pinned provider routes")
            }
            for entry in plan.entries() {
                let proof = entry
                    .provider_route
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Typed provider route proof is missing"))?;
                self.providers
                    .route_for(entry, plan.query_digest(), &self.task, proof)?;
            }
            return Ok(true);
        }
        let entries = plan.entries().collect::<Vec<_>>();
        let proofs = self
            .providers
            .bind_plan(&entries, plan.query_digest(), &self.task)?;
        if entries
            .iter()
            .any(|entry| !self.routes.contains_key(&entry.work.capability()))
        {
            return Ok(false);
        }
        plan.bind_routes(proofs)?;
        Ok(true)
    }

    /// Bounded audit summary only; never includes module bodies, requests, or results.
    pub(super) fn audit_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "snapshot_sha256": self.digest,
            "module_count": self.module_count,
            "broker_id": self.broker_id,
            "format_version":2,
            "modules_sha256":self.module_digest,
            "expert_providers_sha256":self.providers.digest(),
            "expert_provider_count":self.providers.len(),
        })
    }

    /// Dispatch validated original-goal work through its first-turn pinned route
    /// and verify the typed response before returning it.
    pub(super) async fn evaluate(
        &self,
        task: &str,
        entry: &Entry,
        store: &crate::store::Store,
        root_query_sha256: &str,
        deadline: Instant,
        policy: &TaskPolicy,
    ) -> Result<String> {
        self.broker.require_store(store)?;
        let work = &entry.work;
        let capability = work.capability();
        policy.authorize(task, "jev-plus", capability.as_str())?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || remaining > MAX_MODULE_DEADLINE {
            bail!("Typed expert deadline is expired or exceeds the task limit")
        }
        let candidates = self
            .routes
            .get(&capability)
            .ok_or_else(|| anyhow::anyhow!("No first-turn pinned typed expert is available"))?;
        if candidates.task() != task {
            bail!("Typed expert snapshot belongs to another task")
        }
        let operation = async {
            if self.task != task {
                bail!("Typed provider snapshot belongs to another task")
            }
            let proof = entry
                .provider_route
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Typed operation has no admitted provider plan"))?;
            let route = self
                .providers
                .route_for(entry, root_query_sha256, task, proof)?;
            let (reservation, binding) = match route {
                providers::Route::Host => (
                    self.broker
                        .reserve_candidate(candidates, capability, deadline)
                        .await?,
                    None,
                ),
                providers::Route::Bound { pin, audit } => (
                    self.broker.reserve_exact(&pin, deadline).await?,
                    Some(audit),
                ),
            };
            store.event(task, "agent_typed_provider_binding", "jev-plus", serde_json::json!({
                "format_version":1, "operation_id":entry.id, "root_query_sha256":root_query_sha256,
                "criterion_sha256":work.criterion_digest(), "snapshot_sha256":self.digest,
                "capability":capability.as_str(), "binding":binding,
                "module_id":reservation.pin().descriptor().id,
                "module_version":reservation.pin().descriptor().version,
                "requested_capability_requirement":entry.capability_requirement,
                "host_registered_code_only":true,"permissions_granted":false,
                "weight_identity_verified":false,"live_registry_read":false
            }))?;
            let grant = Grant::narrow(task, "jev-plus", [capability], 1, policy)?;
            let response = self
                .broker
                .dispatch_reserved(
                    reservation,
                    &grant,
                    policy,
                    "jev-plus",
                    work.request().clone(),
                    deadline,
                )
                .await?;
            work.verify_response(response).await
        };
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), operation)
            .await
            .map_err(|_| anyhow::anyhow!("Typed expert deadline exhausted"))?
    }
}
