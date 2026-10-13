//! Trusted replaceable Jev+ expert selection; decisions never grant permissions.
use crate::store::{self, Skill};
use anyhow::{bail, Result};
use serde::Serialize;
use std::{collections::BTreeSet, sync::Arc};
#[cfg(test)]
mod tests;
pub const MAX_ROUTES: usize = 2;
pub const MAX_ROUTING_SKILLS: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Descriptor {
    pub id: String,
    pub version: String,
}
impl Descriptor {
    fn validate(&self) -> Result<()> {
        if !crate::registry::identifier(&self.id)
            || self.id.len() > 64
            || self.version.len() > 64
            || semver::Version::parse(&self.version)?.to_string() != self.version
        {
            bail!("Invalid expert router identity/version")
        }
        Ok(())
    }
}
pub struct Request<'a> {
    pub objective: &'a str,
    pub skills: &'a [Skill],
    pub max_routes: usize,
}
/// Implementations are operator-trusted Rust code, not a sandboxed plugin API.
/// Callbacks must be short and nonblocking; the runtime cannot preempt native code.
/// Provider inference or external actions need separate budgeted integration.
pub trait ExpertRouter: Send + Sync {
    fn descriptor(&self) -> Descriptor;
    fn select(&self, request: Request<'_>) -> Result<Vec<String>>;
}
pub struct TagRouter;
impl ExpertRouter for TagRouter {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "jev-plus.tags".into(),
            version: "1.0.0".into(),
        }
    }
    fn select(&self, request: Request<'_>) -> Result<Vec<String>> {
        Ok(store::route(request.skills, request.objective))
    }
}
#[derive(Clone)]
pub(crate) struct Handle {
    backend: Arc<dyn ExpertRouter>,
    descriptor: Descriptor,
}
impl Default for Handle {
    fn default() -> Self {
        let backend = Arc::new(TagRouter);
        let descriptor = backend.descriptor();
        Self {
            backend,
            descriptor,
        }
    }
}
impl Handle {
    pub(crate) fn new(backend: Arc<dyn ExpertRouter>) -> Result<Self> {
        let descriptor = backend.descriptor();
        descriptor.validate()?;
        Ok(Self {
            backend,
            descriptor,
        })
    }
    pub(crate) fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub(crate) fn select(&self, objective: &str, skills: &[Skill]) -> Result<Vec<String>> {
        if objective.trim().is_empty()
            || objective.len() > 12000
            || skills.len() > MAX_ROUTING_SKILLS
        {
            bail!("Expert routing input exceeds bounds")
        }
        let selected = self.backend.select(Request {
            objective,
            skills,
            max_routes: MAX_ROUTES,
        })?;
        let mut unique = BTreeSet::new();
        if selected.len() > MAX_ROUTES
            || selected.iter().any(|id| {
                !unique.insert(id) || !skills.iter().any(|skill| &skill.id == id && skill.enabled)
            })
        {
            bail!("Expert router selected unknown, disabled, duplicate or excessive skills")
        }
        Ok(selected)
    }
}
