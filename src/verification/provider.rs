//! Optional operator-trusted second checks for existing compiled criteria.
//!
//! A provider can veto a compiled result, but it cannot certify an answer by
//! itself. The compiled criterion remains the success gate.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierDescriptor {
    pub id: String,
    pub version: String,
    /// Operator-declared source/build identity; runtime does not attest binaries.
    pub implementation_sha256: String,
}

impl VerifierDescriptor {
    pub(crate) fn validate(&self) -> Result<()> {
        if !crate::registry::identifier(&self.id)
            || self.id.len() > 64
            || self.version.len() > 64
            || semver::Version::parse(&self.version)?.to_string() != self.version
            || self.implementation_sha256.len() != 64
            || !self
                .implementation_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            bail!("Invalid verifier provider identity")
        }
        Ok(())
    }
}

/// Bounded input from the original user objective and exact candidate bytes.
/// `deadline` is the original task deadline, not a renewable provider allowance.
#[derive(Clone, Debug)]
pub struct VerificationRequest {
    pub criterion: String,
    pub objective: String,
    pub objective_sha256: String,
    pub answer: String,
    pub answer_sha256: String,
    pub deadline: Instant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationClaim {
    pub accepted: bool,
    /// Stable machine code only. Human-readable provider text is intentionally
    /// excluded from audits and solver prompts to avoid secret leakage/injection.
    pub reason_code: Option<String>,
}

impl VerificationClaim {
    pub fn accepted() -> Self {
        Self {
            accepted: true,
            reason_code: None,
        }
    }

    pub fn rejected(reason_code: impl Into<String>) -> Self {
        Self {
            accepted: false,
            reason_code: Some(reason_code.into()),
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        match (self.accepted, self.reason_code.as_deref()) {
            (true, None) => Ok(()),
            (false, Some(code)) if code.len() <= 64 && crate::registry::identifier(code) => Ok(()),
            _ => bail!("Invalid verifier claim"),
        }
    }
}

pub type VerificationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<VerificationClaim>> + Send + 'a>>;

/// Operator-installed native code. This is not a sandboxed plugin interface.
/// Implementations must be cancellation-safe when their returned future is
/// dropped, must honor the supplied deadline, and must not access tools, files,
/// networks, secrets, or permissions through this interface.
pub trait VerifierProvider: Send + Sync {
    fn descriptor(&self) -> VerifierDescriptor;
    fn verify(&self, request: VerificationRequest) -> VerificationFuture<'_>;
}

#[derive(Clone)]
pub(crate) struct Handle {
    provider: Arc<dyn VerifierProvider>,
    descriptor: VerifierDescriptor,
}

impl Handle {
    pub(crate) fn new(provider: Arc<dyn VerifierProvider>) -> Result<Self> {
        let descriptor = provider.descriptor();
        descriptor.validate()?;
        Ok(Self {
            provider,
            descriptor,
        })
    }

    pub(crate) fn descriptor(&self) -> &VerifierDescriptor {
        &self.descriptor
    }

    pub(crate) async fn verify(&self, request: VerificationRequest) -> Result<VerificationClaim> {
        if request.objective.len() > 12_000
            || request.answer.len() > 8_192
            || request.criterion.len() > 64
            || Instant::now() >= request.deadline
        {
            bail!("Verifier request is expired or exceeds bounds")
        }
        let deadline = tokio::time::Instant::from_std(request.deadline);
        let claim = tokio::time::timeout_at(deadline, self.provider.verify(request))
            .await
            .map_err(|_| anyhow::anyhow!("Verifier provider deadline exceeded"))??;
        claim.validate()?;
        Ok(claim)
    }
}
