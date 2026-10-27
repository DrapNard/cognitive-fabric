//! Host-pinned deterministic experts for the client collective.
//!
//! This snapshot only routes typed requests to trusted host modules already
//! registered with the broker. It is separate from the manifest-backed Expert
//! Network, and the metadata digest is provenance bookkeeping rather than an
//! attestation of executable code or model weights.

use crate::{
    fabric::modules::{Broker, Capability, Grant, Request, Response, TaskRoutes},
    math,
    security::TaskPolicy,
};
use anyhow::{bail, Result};
use serde::Serialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_SNAPSHOT_BYTES: usize = 64 * 1024;
const MAX_EXPRESSION_BYTES: usize = 120;
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
    pub(super) fn capture(broker: Arc<Broker>, task: &str) -> Result<Self> {
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
        let digest = crate::store::sha256(&encoded);
        Ok(Self {
            broker,
            routes,
            digest,
            module_count: pins.len(),
            broker_id,
        })
    }

    pub(super) fn digest(&self) -> &str {
        &self.digest
    }

    pub(super) fn module_count(&self) -> usize {
        self.module_count
    }

    pub(super) fn arithmetic_available(&self) -> bool {
        self.routes.contains_key(&Capability::MathEvaluate)
    }

    /// Bounded audit summary only; never includes module bodies, requests, or results.
    pub(super) fn audit_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "snapshot_sha256": self.digest,
            "module_count": self.module_count,
            "broker_id": self.broker_id,
        })
    }

    /// Evaluate through the first-turn pinned arithmetic route and independently
    /// check its result with the compiled parser before returning it.
    pub(super) async fn evaluate_arithmetic(
        &self,
        task: &str,
        expression: &str,
        deadline: Instant,
        policy: &TaskPolicy,
    ) -> Result<String> {
        policy.authorize(task, "jev-plus", "math.evaluate")?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || remaining > MAX_MODULE_DEADLINE {
            bail!("Typed arithmetic deadline is expired or exceeds the task limit")
        }
        if expression.trim().is_empty() || expression.len() > MAX_EXPRESSION_BYTES {
            bail!("Invalid typed arithmetic expression")
        }
        let expected = math::calculate(expression)?;
        let candidates = self.routes.get(&Capability::MathEvaluate).ok_or_else(|| {
            anyhow::anyhow!("No first-turn pinned arithmetic expert is available")
        })?;
        if candidates.task() != task {
            bail!("Typed arithmetic snapshot belongs to another task")
        }
        let reservation = self
            .broker
            .reserve_candidate(candidates, Capability::MathEvaluate, deadline)
            .await?;
        let grant = Grant::narrow(task, "jev-plus", [Capability::MathEvaluate], 1, policy)?;
        match self
            .broker
            .dispatch_reserved(
                reservation,
                &grant,
                policy,
                "jev-plus",
                Request::Arithmetic {
                    expression: expression.to_owned(),
                },
                deadline,
            )
            .await?
        {
            Response::Arithmetic { value } if value == expected => Ok(value),
            Response::Arithmetic { .. } => {
                bail!("Typed arithmetic expert failed independent verification")
            }
            _ => bail!("Typed arithmetic expert returned another response kind"),
        }
    }
}
