//! Runtime-owned module discovery and bounded, validated FlatBuffers inference RPC.
use crate::{
    backend::{Generation, InferenceBackend},
    security::TaskPolicy,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch, OnceCell};
pub(crate) mod protocol;
pub mod tools;
pub mod unix;
use protocol::{actor, decode, encode, now_ms, Header, Packet, Payload, ROLES};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModuleDescriptor {
    pub id: String,
    pub version: String,
    pub roles: Vec<String>,
}

/// Observed local adapter state, not a remote health or inference guarantee.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Unknown,
    Connected,
    InUse,
    Closed,
    Expired,
    Exhausted,
}
impl Availability {
    pub(crate) fn unavailable(self) -> bool {
        matches!(self, Self::Closed | Self::Expired | Self::Exhausted)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ModuleStatus {
    pub descriptor: ModuleDescriptor,
    pub active: bool,
    pub availability: Availability,
    pub routing_rank: Option<u32>,
}

impl ModuleDescriptor {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self.id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
            || self.roles.is_empty()
            || self.roles.len() > ROLES.len()
        {
            bail!("Invalid inference module descriptor")
        }
        if self.version.len() > 128 {
            bail!("Module version exceeds bounds")
        }
        semver::Version::parse(&self.version)?;
        let mut seen = BTreeSet::new();
        if self
            .roles
            .iter()
            .any(|role| !ROLES.contains(&role.as_str()) || !seen.insert(role))
        {
            bail!("Invalid or duplicate module role")
        }
        Ok(())
    }
}

struct Work {
    enqueued: std::time::Instant,
    bytes: Vec<u8>,
    identity: Header,
    reply: oneshot::Sender<Result<Vec<u8>>>,
}
struct Module {
    descriptor: ModuleDescriptor,
    backend: Arc<dyn InferenceBackend>,
    queue: OnceCell<mpsc::Sender<Work>>,
    shutdown: Arc<watch::Sender<bool>>,
    routing_rank: Option<u32>,
}

fn same_header(left: &Header, right: &Header) -> bool {
    left.correlation == right.correlation
        && left.task == right.task
        && left.sender == right.sender
        && left.destination == right.destination
        && left.module_version == right.module_version
        && left.expires == right.expires
}

impl Module {
    async fn queue(&self) -> &mpsc::Sender<Work> {
        self.queue.get_or_init(|| async {
            let (sender, mut receiver) = mpsc::channel::<Work>(4);
            let backend = self.backend.clone();
            let descriptor = self.descriptor.clone();
            let mut shutdown = self.shutdown.subscribe();
            tokio::spawn(async move {
                loop {
                    let mut work = tokio::select! {
                        biased;
                        _ = shutdown.wait_for(|stopped| *stopped) => break,
                        work = receiver.recv() => match work { Some(work) => work, None => break },
                    };
                    if work.reply.is_closed() { continue; }
                    let queue_wait_ms = work.enqueued.elapsed().as_secs_f64() * 1000.0;
                    let result = async {
                        let packet = decode(&work.bytes)?;
                        if !same_header(&packet.header, &work.identity) || packet.header.destination != descriptor.id || packet.header.module_version != descriptor.version || packet.header.expires <= now_ms() { bail!("Module identity or deadline mismatch") }
                        let Payload::Request { role, prompt, tokens } = packet.payload else { bail!("Module expects an inference request") };
                        if !descriptor.roles.contains(&role) || packet.header.sender != actor(&role)? { bail!("Module role is not registered for this actor") }
                        TaskPolicy::reasoning(&packet.header.task)?.authorize(&packet.header.task, &packet.header.sender, "llm.generate")?;
                        let remaining = Duration::from_millis(packet.header.expires.saturating_sub(now_ms()));
                        let result = tokio::select! {
                            biased;
                            _ = shutdown.wait_for(|stopped| *stopped) => return Err(anyhow!("Module broker shut down")),
                            _ = work.reply.closed() => return Err(anyhow!("Module caller cancelled")),
                            result = tokio::time::timeout(remaining, backend.generate_for(&packet.header.task, &role, &prompt, tokens)) => result.map_err(|_| anyhow!("Module deadline exhausted"))?,
                        };
                        let payload = match result {
                            Ok(report) if report.output_tokens.is_some_and(|count| count > tokens as u64) => Payload::Error("Module exceeded output token budget".into()),
                            Ok(mut report) => { report.queue_wait_ms = Some(queue_wait_ms); Payload::Response(report) },
                            Err(error) => Payload::Error(error.to_string().chars().take(250).collect()),
                        };
                        encode(&Packet { header: packet.header, payload })
                    }.await;
                    let _ = work.reply.send(result);
                }
            });
            sender
        }).await
    }
}

#[derive(Default)]
struct Registry {
    active: BTreeMap<String, Arc<Module>>,
    seen: BTreeSet<(String, String)>,
    versions: BTreeMap<(String, String), Arc<Module>>,
}

pub struct Broker {
    registry: RwLock<Registry>,
    shutdown: Arc<watch::Sender<bool>>,
}
impl Broker {
    pub fn with_inference(backend: Arc<dyn InferenceBackend>) -> Self {
        Self::try_with_inference(backend)
            .expect("Trusted initial inference descriptor must be valid")
    }

    /// Fallible host construction for transport-bound adapters. Fixed identities
    /// are preserved instead of being relabelled with the default local module.
    pub fn try_with_inference(backend: Arc<dyn InferenceBackend>) -> Result<Self> {
        let descriptor = backend
            .module_descriptor()
            .cloned()
            .unwrap_or_else(|| ModuleDescriptor {
                id: "inference.local".into(),
                version: "1.0.0".into(),
                roles: ROLES.iter().map(|role| role.to_string()).collect(),
            });
        descriptor.validate()?;
        let shutdown = Arc::new(watch::channel(false).0);
        let module = Arc::new(Module {
            descriptor: descriptor.clone(),
            backend,
            queue: OnceCell::new(),
            shutdown: shutdown.clone(),
            routing_rank: None,
        });
        let mut registry = Registry::default();
        registry
            .seen
            .insert((descriptor.id.clone(), descriptor.version.clone()));
        registry
            .versions
            .insert((descriptor.id.clone(), descriptor.version), module.clone());
        registry.active.insert(descriptor.id, module);
        Ok(Self {
            registry: RwLock::new(registry),
            shutdown,
        })
    }

    /// Trusted host registration, never interpreted from a model or socket actor field.
    pub fn register(
        &self,
        descriptor: ModuleDescriptor,
        backend: Arc<dyn InferenceBackend>,
    ) -> Result<()> {
        self.register_with_rank(descriptor, backend, None)
    }

    /// Explicit trusted-host approval to compete with other ranked modules.
    /// Lower rank wins; rank is policy metadata, not measured inference cost.
    pub fn register_candidate(
        &self,
        descriptor: ModuleDescriptor,
        backend: Arc<dyn InferenceBackend>,
        routing_rank: u32,
    ) -> Result<()> {
        if routing_rank > 1_000_000 {
            bail!("Inference routing rank exceeds bounds")
        }
        self.register_with_rank(descriptor, backend, Some(routing_rank))
    }

    fn register_with_rank(
        &self,
        descriptor: ModuleDescriptor,
        backend: Arc<dyn InferenceBackend>,
        routing_rank: Option<u32>,
    ) -> Result<()> {
        descriptor.validate()?;
        if backend
            .module_descriptor()
            .is_some_and(|bound| bound != &descriptor)
        {
            bail!("Inference registration differs from backend transport identity")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Module registry poisoned"))?;
        if *self.shutdown.borrow() {
            bail!("Module broker shut down")
        }
        if registry.seen.len() >= 128
            || registry
                .seen
                .contains(&(descriptor.id.clone(), descriptor.version.clone()))
        {
            bail!("Module version is immutable or registry capacity exhausted")
        }
        if !registry.active.contains_key(&descriptor.id) && registry.active.len() >= 32 {
            bail!("Active module capacity exhausted")
        }
        for module in registry
            .active
            .values()
            .filter(|module| module.descriptor.id != descriptor.id)
        {
            if descriptor
                .roles
                .iter()
                .any(|role| module.descriptor.roles.contains(role))
                && (routing_rank.is_none() || module.routing_rank.is_none())
            {
                bail!("Conflicting active inference role")
            }
        }
        registry
            .seen
            .insert((descriptor.id.clone(), descriptor.version.clone()));
        let module = Arc::new(Module {
            descriptor,
            backend,
            queue: OnceCell::new(),
            shutdown: self.shutdown.clone(),
            routing_rank,
        });
        registry.versions.insert(
            (
                module.descriptor.id.clone(),
                module.descriptor.version.clone(),
            ),
            module.clone(),
        );
        registry.active.insert(module.descriptor.id.clone(), module);
        Ok(())
    }

    /// Trusted host rollback/reactivation of an immutable registered backend.
    /// Existing task routes remain pinned. This neither probes health nor renews
    /// transport grants or changes external model residency.
    pub fn activate(&self, id: &str, version: &str) -> Result<()> {
        if id.len() > 64 || version.len() > 128 {
            bail!("Invalid inference module version selection")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Module registry poisoned"))?;
        if *self.shutdown.borrow() {
            bail!("Module broker shut down")
        }
        let module = registry
            .versions
            .get(&(id.into(), version.into()))
            .cloned()
            .context("Unknown registered inference module version")?;
        if module.backend.availability().unavailable() {
            bail!("Registered inference transport is permanently closed")
        }
        if !registry.active.contains_key(id) && registry.active.len() >= 32 {
            bail!("Active module capacity exhausted")
        }
        if registry.active.values().any(|active| {
            active.descriptor.id != id
                && (module.routing_rank.is_none() || active.routing_rank.is_none())
                && module
                    .descriptor
                    .roles
                    .iter()
                    .any(|role| active.descriptor.roles.contains(role))
        }) {
            bail!("Conflicting active inference role")
        }
        registry.active.insert(id.into(), module);
        Ok(())
    }

    /// Release an inactive version from future rollback without disrupting pins.
    /// The immutable identity remains reserved; external model residency is not
    /// changed. Existing tasks retain their backend until their last pin drops.
    pub fn retire(&self, id: &str, version: &str) -> Result<()> {
        if id.len() > 64 || version.len() > 128 {
            bail!("Invalid inference module version selection")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Module registry poisoned"))?;
        if *self.shutdown.borrow() {
            bail!("Module broker shut down")
        }
        if registry
            .active
            .get(id)
            .is_some_and(|module| module.descriptor.version == version)
        {
            bail!("Cannot retire an active inference module version")
        }
        let retired = registry
            .versions
            .remove(&(id.into(), version.into()))
            .context("Unknown retained inference module version")?;
        // Host backend destructors may inspect committed registry metadata.
        drop(registry);
        drop(retired);
        Ok(())
    }

    /// All retained version metadata, including inactive modules; no inference.
    pub fn versions(&self) -> Result<Vec<ModuleDescriptor>> {
        Ok(self
            .registry
            .read()
            .map_err(|_| anyhow!("Module registry poisoned"))?
            .versions
            .values()
            .map(|module| module.descriptor.clone())
            .collect())
    }

    /// Bounded retained-version inventory without network calls or new grants.
    pub fn statuses(&self) -> Result<Vec<ModuleStatus>> {
        let registry = self
            .registry
            .read()
            .map_err(|_| anyhow!("Module registry poisoned"))?;
        Ok(registry
            .versions
            .values()
            .map(|module| ModuleStatus {
                descriptor: module.descriptor.clone(),
                active: registry
                    .active
                    .get(&module.descriptor.id)
                    .is_some_and(|active| Arc::ptr_eq(active, module)),
                availability: module.backend.availability(),
                routing_rank: module.routing_rank,
            })
            .collect())
    }

    /// Stop all versions, including modules retained by already pinned tasks.
    /// Dropping backend futures does not prove an external inference server stopped.
    pub fn shutdown(&self) -> Result<()> {
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Module registry poisoned"))?;
        self.shutdown.send_replace(true);
        let retired = std::mem::take(&mut registry.versions);
        registry.active.clear();
        // Host backend destructors may inspect committed registry metadata.
        drop(registry);
        drop(retired);
        Ok(())
    }

    pub fn unregister(&self, id: &str) -> Result<()> {
        self.registry
            .write()
            .map_err(|_| anyhow!("Module registry poisoned"))?
            .active
            .remove(id)
            .context("Unknown active module")?;
        Ok(())
    }

    pub fn discover(&self) -> Result<Vec<ModuleDescriptor>> {
        Ok(self
            .registry
            .read()
            .map_err(|_| anyhow!("Module registry poisoned"))?
            .active
            .values()
            .map(|module| module.descriptor.clone())
            .collect())
    }

    fn snapshot(&self) -> Result<Pinned> {
        let registry = self
            .registry
            .read()
            .map_err(|_| anyhow!("Module registry poisoned"))?;
        if *self.shutdown.borrow() {
            bail!("Module broker shut down")
        }
        let mut routes = BTreeMap::new();
        let mut candidates = registry.active.values().collect::<Vec<_>>();
        candidates.sort_by_key(|module| (module.routing_rank.unwrap_or(0), &module.descriptor.id));
        for module in candidates {
            if module.backend.availability().unavailable() {
                continue;
            }
            for role in &module.descriptor.roles {
                routes.entry(role.clone()).or_insert_with(|| module.clone());
            }
        }
        Ok(Pinned { routes })
    }
}

struct Pinned {
    routes: BTreeMap<String, Arc<Module>>,
}
impl InferenceBackend for Broker {
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            self.snapshot()?
                .generate_for(task, role, prompt, max_tokens)
                .await
        })
    }
    fn modules(&self) -> Result<Vec<ModuleDescriptor>> {
        self.discover()
    }
    fn pin(&self) -> Result<Option<Arc<dyn InferenceBackend>>> {
        Ok(Some(Arc::new(self.snapshot()?)))
    }
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move { self.snapshot()?.generate(role, prompt, max_tokens).await })
    }
}
impl InferenceBackend for Pinned {
    fn modules(&self) -> Result<Vec<ModuleDescriptor>> {
        let modules = self
            .routes
            .values()
            .map(|module| (module.descriptor.id.clone(), module.descriptor.clone()))
            .collect::<BTreeMap<_, _>>();
        Ok(modules.into_values().collect())
    }
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            self.generate_for(&uuid::Uuid::new_v4().to_string(), role, prompt, max_tokens)
                .await
        })
    }
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            let module = self
                .routes
                .get(role)
                .context("No registered module for inference role")?;
            if *module.shutdown.borrow() {
                bail!("Module broker shut down")
            }
            if module.backend.availability().unavailable() {
                bail!("Pinned inference transport is permanently closed")
            }
            let sender = actor(role)?;
            TaskPolicy::reasoning(task)?.authorize(task, sender, "llm.generate")?;
            let header = Header {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: task.into(),
                sender: sender.into(),
                destination: module.descriptor.id.clone(),
                module_version: module.descriptor.version.clone(),
                expires: now_ms().saturating_add(300_000),
            };
            let bytes = encode(&Packet {
                header: header.clone(),
                payload: Payload::Request {
                    role: role.into(),
                    prompt: prompt.into(),
                    tokens: max_tokens,
                },
            })?;
            // Validate before allocating a queue slot; workers validate again at the trust boundary.
            decode(&bytes)?;
            let (reply, response) = oneshot::channel();
            module
                .queue()
                .await
                .try_send(Work {
                    enqueued: std::time::Instant::now(),
                    bytes,
                    identity: header.clone(),
                    reply,
                })
                .map_err(|_| anyhow!("Module queue busy or unavailable"))?;
            let bytes = response.await.context("Module worker unavailable")??;
            let response = decode(&bytes)?;
            if !same_header(&header, &response.header) {
                bail!("Module response correlation mismatch")
            }
            match response.payload {
                Payload::Response(report) => Ok(report),
                Payload::Error(error) => bail!("Module failed: {error}"),
                _ => bail!("Unexpected module response"),
            }
        })
    }
}
