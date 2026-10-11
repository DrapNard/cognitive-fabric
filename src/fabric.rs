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
use tokio::sync::{mpsc, oneshot, OnceCell};
pub(crate) mod protocol;
pub mod unix;
use protocol::{actor, decode, encode, now_ms, Header, Packet, Payload, ROLES};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Serialize)]
pub struct ModuleDescriptor {
    pub id: String,
    pub version: String,
    pub roles: Vec<String>,
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
            tokio::spawn(async move {
                while let Some(mut work) = receiver.recv().await {
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
}

pub struct Broker {
    registry: RwLock<Registry>,
}
impl Broker {
    pub fn with_inference(backend: Arc<dyn InferenceBackend>) -> Self {
        let descriptor = ModuleDescriptor {
            id: "inference.local".into(),
            version: "1.0.0".into(),
            roles: ROLES.iter().map(|role| role.to_string()).collect(),
        };
        let module = Arc::new(Module {
            descriptor: descriptor.clone(),
            backend,
            queue: OnceCell::new(),
        });
        let mut registry = Registry::default();
        registry
            .seen
            .insert((descriptor.id.clone(), descriptor.version));
        registry.active.insert(descriptor.id, module);
        Self {
            registry: RwLock::new(registry),
        }
    }

    /// Trusted host registration, never interpreted from a model or socket actor field.
    pub fn register(
        &self,
        descriptor: ModuleDescriptor,
        backend: Arc<dyn InferenceBackend>,
    ) -> Result<()> {
        descriptor.validate()?;
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Module registry poisoned"))?;
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
            {
                bail!("Conflicting active inference role")
            }
        }
        registry
            .seen
            .insert((descriptor.id.clone(), descriptor.version.clone()));
        registry.active.insert(
            descriptor.id.clone(),
            Arc::new(Module {
                descriptor,
                backend,
                queue: OnceCell::new(),
            }),
        );
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
        let mut routes = BTreeMap::new();
        for module in registry.active.values() {
            for role in &module.descriptor.roles {
                routes.insert(role.clone(), module.clone());
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
