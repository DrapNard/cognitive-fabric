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
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot, watch, OnceCell},
    task::JoinSet,
};
pub mod modules;
pub(crate) mod protocol;
pub mod tools;
pub mod unix;
pub mod worker_cleanup;
use protocol::{actor, decode, encode, now_ms, Header, Packet, Payload, ROLES};
#[cfg(test)]
mod parallel_tests;
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
    pub queued_requests: Option<u32>,
    pub processing_request: Option<bool>,
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
    backend: Option<Arc<dyn InferenceBackend>>,
    reply: oneshot::Sender<Result<Vec<u8>>>,
}
struct Module {
    descriptor: ModuleDescriptor,
    backend: Arc<dyn InferenceBackend>,
    queue: OnceCell<mpsc::Sender<Work>>,
    processing: Arc<AtomicUsize>,
    max_concurrency: usize,
    shutdown: Arc<watch::Sender<bool>>,
    routing_rank: Option<u32>,
}

// Counts concurrently active requests. Drop clears each count on cancellation,
// shutdown, malformed input, provider error or worker unwinding.
struct Processing(Arc<AtomicUsize>);
impl Processing {
    fn new(flag: Arc<AtomicUsize>) -> Self {
        flag.fetch_add(1, Ordering::AcqRel);
        Self(flag)
    }
}
impl Drop for Processing {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

// Routing samples available queue capacity once. This is not a slot reservation:
// submissions can change it immediately after a new task chooses its route.
fn routing_pressure<T>(queue: Option<&mpsc::Sender<T>>, processing: usize) -> (bool, usize) {
    let (full, queued) = queue.map_or((false, 0), |queue| {
        let available = queue.capacity();
        (available == 0, queue.max_capacity() - available)
    });
    (full, queued + processing)
}

fn same_header(left: &Header, right: &Header) -> bool {
    left.correlation == right.correlation
        && left.task == right.task
        && left.sender == right.sender
        && left.destination == right.destination
        && left.module_version == right.module_version
        && left.expires == right.expires
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stopped| *stopped).await;
}

impl Module {
    fn availability(&self) -> Availability {
        if self.queue.get().is_some_and(mpsc::Sender::is_closed) {
            Availability::Closed
        } else {
            self.backend.availability()
        }
    }
    fn queued_requests(&self) -> usize {
        self.queue
            .get()
            .map_or(0, |queue| queue.max_capacity() - queue.capacity())
    }
    async fn queue(&self) -> &mpsc::Sender<Work> {
        self.queue
            .get_or_init(|| async {
                let (sender, mut receiver) = mpsc::channel::<Work>(4);
                let backend = self.backend.clone();
                let descriptor = self.descriptor.clone();
                let processing = self.processing.clone();
                let max_concurrency = self.max_concurrency;
                let mut shutdown = self.shutdown.subscribe();
                tokio::spawn(async move {
                    let mut tasks = JoinSet::new();
                    let mut receiver_closed = false;
                    loop {
                        if receiver_closed && tasks.is_empty() {
                            break;
                        }
                        if receiver_closed || tasks.len() >= max_concurrency {
                            tokio::select! {
                                biased;
                                _ = wait_for_shutdown(&mut shutdown) => {
                                    tasks.abort_all();
                                    while tasks.join_next().await.is_some() {}
                                    break;
                                }
                                result = tasks.join_next(), if !tasks.is_empty() => {
                                    if result.is_some_and(|joined| joined.is_err()) {
                                        tasks.abort_all();
                                        while tasks.join_next().await.is_some() {}
                                        break;
                                    }
                                }
                            }
                            continue;
                        }
                        tokio::select! {
                            biased;
                            _ = wait_for_shutdown(&mut shutdown) => {
                                tasks.abort_all();
                                while tasks.join_next().await.is_some() {}
                                break;
                            }
                            work = receiver.recv(), if !receiver_closed => {
                                match work {
                                    Some(work) if !work.reply.is_closed() => {
                                        let task_backend = backend.clone();
                                        let task_descriptor = descriptor.clone();
                                        let task_processing = processing.clone();
                                        let task_shutdown = shutdown.clone();
                                        tasks.spawn(process_work(
                                            work,
                                            task_backend,
                                            task_descriptor,
                                            task_processing,
                                            task_shutdown,
                                        ));
                                    }
                                    Some(_) => {}
                                    None => receiver_closed = true,
                                }
                            }
                            result = tasks.join_next(), if !tasks.is_empty() => {
                                if result.is_some_and(|joined| joined.is_err()) {
                                    tasks.abort_all();
                                    while tasks.join_next().await.is_some() {}
                                    break;
                                }
                            }
                        }
                    }
                });
                sender
            })
            .await
    }
}

async fn process_work(
    mut work: Work,
    backend: Arc<dyn InferenceBackend>,
    descriptor: ModuleDescriptor,
    processing: Arc<AtomicUsize>,
    mut shutdown: watch::Receiver<bool>,
) {
    if work.reply.is_closed() {
        return;
    }
    let backend = work.backend.take().unwrap_or(backend);
    let _processing = Processing::new(processing);
    let queue_wait_ms = work.enqueued.elapsed().as_secs_f64() * 1000.0;
    let result = async {
        let packet = decode(&work.bytes)?;
        if !same_header(&packet.header, &work.identity)
            || packet.header.destination != descriptor.id
            || packet.header.module_version != descriptor.version
            || packet.header.expires <= now_ms()
        {
            bail!("Module identity or deadline mismatch")
        }
        let Payload::Request { role, prompt, tokens } = packet.payload else {
            bail!("Module expects an inference request")
        };
        if !descriptor.roles.contains(&role) || packet.header.sender != actor(&role)? {
            bail!("Module role is not registered for this actor")
        }
        TaskPolicy::reasoning(&packet.header.task)?.authorize(
            &packet.header.task,
            &packet.header.sender,
            "llm.generate",
        )?;
        let remaining = Duration::from_millis(packet.header.expires.saturating_sub(now_ms()));
        let result = tokio::select! {
            biased;
            _ = wait_for_shutdown(&mut shutdown) => return Err(anyhow!("Module broker shut down")),
            _ = work.reply.closed() => return Err(anyhow!("Module caller cancelled")),
            result = tokio::time::timeout(remaining, backend.generate_for(&packet.header.task, &role, &prompt, tokens)) => result.map_err(|_| anyhow!("Module deadline exhausted"))?,
        };
        let payload = match result {
            Ok(report) if report.output_tokens.is_some_and(|count| count > tokens as u64) => {
                Payload::Error("Module exceeded output token budget".into())
            }
            Ok(mut report) => {
                report.queue_wait_ms = Some(queue_wait_ms);
                Payload::Response(report)
            }
            Err(error) => Payload::Error(error.to_string().chars().take(250).collect()),
        };
        encode(&Packet { header: packet.header, payload })
    }
    .await;
    let _ = work.reply.send(result);
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
    max_concurrency: usize,
}

/// Stable task-policy scope for recent-advice and empirical decisions. It binds
/// only the selected role/module/target/profile map and original resource
/// catalogs; queue pressure, excluded candidates and pool UUIDs are observations.
pub(crate) fn resource_route_scope(selection: &serde_json::Value) -> Result<String> {
    let binding = &selection["binding"];
    let routes = selection["routes"]
        .as_array()
        .ok_or_else(|| anyhow!("Resource route map is unavailable"))?
        .iter()
        .map(|route| {
            serde_json::json!({
                "role":route["role"],
                "module_id":route["module_id"],
                "module_version":route["module_version"],
                "target":route["resource"]["target"],
                "profile_id":route["resource"]["profile_id"],
                "allocation":route["resource"]["allocation"],
                "resident_coverage":route["resource"]["resident_coverage"],
            })
        })
        .collect::<Vec<serde_json::Value>>();
    if serde_json::to_vec(binding)?.len() > 4096 || routes.len() > ROLES.len() {
        bail!("Resource route scope exceeds bounds")
    }
    Ok(crate::store::sha256(&serde_json::to_vec(
        &serde_json::json!({
            "domain":"cognitive-fabric/resource-route-scope/v1",
            "binding":binding,
            "routes":routes,
        }),
    )?))
}

impl Broker {
    pub fn with_inference(backend: Arc<dyn InferenceBackend>) -> Self {
        Self::try_with_inference(backend)
            .expect("Trusted initial inference descriptor must be valid")
    }

    /// Trusted host construction with a small explicit per-module concurrency
    /// ceiling. Default construction remains one active request per module.
    pub(crate) fn with_inference_concurrency(
        backend: Arc<dyn InferenceBackend>,
        max_concurrency: usize,
    ) -> Self {
        Self::try_with_inference_concurrency(backend, max_concurrency)
            .expect("Trusted inference module concurrency must be bounded")
    }

    /// Fallible host construction for transport-bound adapters. Fixed identities
    /// are preserved instead of being relabelled with the default local module.
    pub fn try_with_inference(backend: Arc<dyn InferenceBackend>) -> Result<Self> {
        Self::try_with_inference_concurrency(backend, 1)
    }

    pub(crate) fn try_with_inference_concurrency(
        backend: Arc<dyn InferenceBackend>,
        max_concurrency: usize,
    ) -> Result<Self> {
        if !(1..=4).contains(&max_concurrency) {
            bail!("Inference module concurrency must be between 1 and 4")
        }
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
            processing: Arc::new(AtomicUsize::new(0)),
            max_concurrency,
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
            max_concurrency,
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
            processing: Arc::new(AtomicUsize::new(0)),
            max_concurrency: self.max_concurrency,
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
        let module = {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow!("Module registry poisoned"))?;
            if *self.shutdown.borrow() {
                bail!("Module broker shut down")
            }
            registry
                .versions
                .get(&(id.into(), version.into()))
                .cloned()
                .context("Unknown registered inference module version")?
        };
        // Trusted observation callbacks may inspect or mutate registry metadata.
        // Never invoke one while holding the registry's lock.
        if module.availability().unavailable() {
            bail!("Registered inference module is permanently closed")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Module registry poisoned"))?;
        if *self.shutdown.borrow() {
            bail!("Module broker shut down")
        }
        let retained = registry
            .versions
            .get(&(id.into(), version.into()))
            .context("Inference module was retired during availability observation")?;
        if !Arc::ptr_eq(retained, &module) {
            bail!("Inference module identity changed during availability observation")
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
        // Freeze bounded membership and activation metadata, then release the
        // lock before invoking trusted backend observation callbacks.
        let snapshot = {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow::anyhow!("Module registry poisoned"))?;
            registry
                .versions
                .values()
                .map(|module| {
                    let active = registry
                        .active
                        .get(&module.descriptor.id)
                        .is_some_and(|active| Arc::ptr_eq(active, module));
                    (module.clone(), active)
                })
                .collect::<Vec<_>>()
        };
        Ok(snapshot
            .into_iter()
            .map(|(module, active)| ModuleStatus {
                descriptor: module.descriptor.clone(),
                active,
                availability: module.availability(),
                routing_rank: module.routing_rank,
                queued_requests: Some(module.queued_requests() as u32),
                processing_request: Some(module.processing.load(Ordering::Acquire) > 0),
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
        let candidates = {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow!("Module registry poisoned"))?;
            if *self.shutdown.borrow() {
                bail!("Module broker shut down")
            }
            registry.active.values().cloned().collect::<Vec<_>>()
        };
        let mut routes = BTreeMap::new();
        // Full queues are ineligible for new pins. Operator rank still dominates
        // load among candidates observed to have room; old pins never migrate.
        let mut candidates = candidates
            .into_iter()
            .filter_map(|module| {
                let (full, load) = routing_pressure(
                    module.queue.get(),
                    module.processing.load(Ordering::Acquire),
                );
                (!full).then_some((load, module))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_cached_key(|(load, module)| {
            (
                module.routing_rank.unwrap_or(0),
                *load,
                module.descriptor.id.clone(),
            )
        });
        for (_, module) in candidates {
            if module.availability().unavailable() {
                continue;
            }
            for role in &module.descriptor.roles {
                routes.entry(role.clone()).or_insert_with(|| module.clone());
            }
        }
        Ok(Pinned {
            routes,
            resource_targets: None,
            route_backends: None,
        })
    }

    /// Select routes only from candidates that satisfy the immutable configured
    /// working-profile and resident declarations. Trusted metadata callbacks are
    /// invoked without the registry, pool, or Store lock held.
    pub(crate) fn pin_resource_aware(
        &self,
        task: &str,
        pool: &crate::resource_scheduler::ResourcePool,
    ) -> Result<(Arc<dyn InferenceBackend>, serde_json::Value)> {
        pool.record_routing_intent(task)?;
        let registry_snapshot = {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow!("Module registry poisoned"))?;
            if *self.shutdown.borrow() {
                bail!("Module broker shut down")
            }
            registry.active.values().cloned().collect::<Vec<_>>()
        };
        let mut candidates = registry_snapshot
            .iter()
            .cloned()
            .map(|module| {
                let (full, load) = routing_pressure(
                    module.queue.get(),
                    module.processing.load(Ordering::Acquire),
                );
                (full, load, module)
            })
            .collect::<Vec<_>>();
        candidates.sort_by_cached_key(|(full, load, module)| {
            (
                module.routing_rank.unwrap_or(0),
                *full,
                *load,
                module.descriptor.id.clone(),
            )
        });

        let mut routes = BTreeMap::<String, Arc<Module>>::new();
        let mut targets = BTreeMap::<String, crate::backend::InferenceTarget>::new();
        let mut route_metadata = BTreeMap::<String, serde_json::Value>::new();
        let mut route_backends = BTreeMap::<String, Arc<dyn InferenceBackend>>::new();
        let mut excluded_candidates = Vec::new();
        for (_, _, module) in &candidates {
            let role_candidates = module
                .descriptor
                .roles
                .iter()
                .filter(|role| !routes.contains_key(*role))
                .cloned()
                .collect::<Vec<_>>();
            if role_candidates.is_empty() {
                continue;
            }
            let (queue_full, _) = routing_pressure(
                module.queue.get(),
                module.processing.load(Ordering::Acquire),
            );
            if queue_full {
                excluded_candidates.extend(role_candidates.iter().map(|role| {
                    serde_json::json!({"role":role,"module_id":module.descriptor.id,
                        "module_version":module.descriptor.version,"reason_code":"queue_full"})
                }));
                continue;
            }
            if module.availability().unavailable() {
                excluded_candidates.extend(role_candidates.iter().map(|role| {
                    serde_json::json!({"role":role,"module_id":module.descriptor.id,
                        "module_version":module.descriptor.version,"reason_code":"unavailable"})
                }));
                continue;
            }
            let pinned_backend = match module.backend.pin() {
                Ok(Some(backend)) => backend,
                Ok(None) => module.backend.clone(),
                Err(_) => {
                    excluded_candidates.extend(role_candidates.iter().map(|role| {
                        serde_json::json!({"role":role,"module_id":module.descriptor.id,
                            "module_version":module.descriptor.version,"reason_code":"metadata_error"})
                    }));
                    continue;
                }
            };
            if pinned_backend
                .module_descriptor()
                .is_some_and(|descriptor| {
                    descriptor.id != module.descriptor.id
                        || descriptor.version != module.descriptor.version
                        || module
                            .descriptor
                            .roles
                            .iter()
                            .any(|role| !descriptor.roles.contains(role))
                })
            {
                excluded_candidates.extend(role_candidates.iter().map(|role| {
                    serde_json::json!({"role":role,"module_id":module.descriptor.id,
                        "module_version":module.descriptor.version,"reason_code":"metadata_error"})
                }));
                continue;
            }
            if pinned_backend.availability().unavailable() {
                excluded_candidates.extend(role_candidates.iter().map(|role| {
                    serde_json::json!({"role":role,"module_id":module.descriptor.id,
                        "module_version":module.descriptor.version,"reason_code":"unavailable"})
                }));
                continue;
            }
            for role in role_candidates {
                let target = match pinned_backend.inference_target(&role) {
                    Ok(target) => target,
                    Err(_) => {
                        excluded_candidates.push(serde_json::json!({"role":role,
                            "module_id":module.descriptor.id,"module_version":module.descriptor.version,
                            "reason_code":"metadata_error"}));
                        continue;
                    }
                };
                let candidate = match pool.routing_candidate(&role, target) {
                    Ok(candidate) => candidate,
                    Err(reason) => {
                        excluded_candidates.push(serde_json::json!({"role":role,
                            "module_id":module.descriptor.id,"module_version":module.descriptor.version,
                            "reason_code":reason.reason_code()}));
                        continue;
                    }
                };
                routes.insert(role.clone(), module.clone());
                targets.insert(role.clone(), candidate.target);
                route_backends.insert(role.clone(), pinned_backend.clone());
                route_metadata.insert(role, candidate.metadata);
            }
        }

        // A trusted callback may have changed activation or retirement. Refuse
        // the whole pin if any selected identity changed before publication.
        {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow!("Module registry poisoned"))?;
            if *self.shutdown.borrow()
                || registry.active.len() != registry_snapshot.len()
                || registry_snapshot.iter().any(|module| {
                    !registry
                        .active
                        .get(&module.descriptor.id)
                        .is_some_and(|active| Arc::ptr_eq(active, module))
                        || !registry
                            .versions
                            .get(&(
                                module.descriptor.id.clone(),
                                module.descriptor.version.clone(),
                            ))
                            .is_some_and(|version| Arc::ptr_eq(version, module))
                })
            {
                bail!("Inference route changed during resource eligibility checks")
            }
        }
        let unmatched_roles = ROLES
            .iter()
            .filter(|role| !routes.contains_key(**role))
            .copied()
            .collect::<Vec<_>>();
        let route_evidence = ROLES
            .iter()
            .filter_map(|role| {
                let module = routes.get(*role)?;
                let resource = route_metadata.get(*role)?;
                Some(serde_json::json!({
                    "role":role,
                    "module_id":module.descriptor.id,
                    "module_version":module.descriptor.version,
                    "resource":resource,
                }))
            })
            .collect::<Vec<_>>();
        if excluded_candidates.len() > 32 * ROLES.len() {
            bail!("Resource route exclusion evidence exceeds bounds")
        }
        excluded_candidates.sort_by(|left, right| {
            let rank = |value: &serde_json::Value| {
                ROLES
                    .iter()
                    .position(|role| Some(*role) == value["role"].as_str())
                    .unwrap_or(ROLES.len())
            };
            (
                rank(left),
                left["module_id"].as_str().unwrap_or_default(),
                left["module_version"].as_str().unwrap_or_default(),
                left["reason_code"].as_str().unwrap_or_default(),
            )
                .cmp(&(
                    rank(right),
                    right["module_id"].as_str().unwrap_or_default(),
                    right["module_version"].as_str().unwrap_or_default(),
                    right["reason_code"].as_str().unwrap_or_default(),
                ))
        });
        let identity = pool.routing_identity()?;
        let mut selection = serde_json::json!({
            "format_version":1,
            "pool_id":identity["pool_id"],
            "binding":identity["binding"],
            "routes":route_evidence,
            "unmatched_roles":unmatched_roles,
            "excluded_candidates":excluded_candidates,
            "eligible":!routes.is_empty(),
            "permissions_granted":false,
            "weights_attested":false,
            "physical_limits_enforced":false,
        });
        let route_scope_sha256 = resource_route_scope(&selection)?;
        selection["route_scope_sha256"] = serde_json::json!(route_scope_sha256);
        if serde_json::to_vec(&selection)?.len() > 32_768 {
            bail!("Resource route selection evidence exceeds bounds")
        }
        pool.record_routing_selection(task, &selection, !routes.is_empty())?;
        Ok((
            Arc::new(Pinned {
                routes,
                resource_targets: Some(targets),
                route_backends: Some(route_backends),
            }),
            selection,
        ))
    }
}

struct Pinned {
    routes: BTreeMap<String, Arc<Module>>,
    resource_targets: Option<BTreeMap<String, crate::backend::InferenceTarget>>,
    route_backends: Option<BTreeMap<String, Arc<dyn InferenceBackend>>>,
}

/// Restrict a restored backend to the exact roles admitted by an original
/// resource-aware route decision. Recovery validates targets and module
/// identity before constructing this adapter; it never searches alternatives.
#[derive(Clone)]
struct RestrictedRoutes {
    backend: Arc<dyn InferenceBackend>,
    routes: BTreeMap<String, crate::backend::InferenceTarget>,
}

pub(crate) fn restrict_inference_roles(
    backend: Arc<dyn InferenceBackend>,
    routes: BTreeMap<String, crate::backend::InferenceTarget>,
) -> Result<Arc<dyn InferenceBackend>> {
    if routes.is_empty()
        || routes.len() > ROLES.len()
        || routes.keys().any(|role| !ROLES.contains(&role.as_str()))
    {
        bail!("Saved inference route role map is invalid")
    }
    let pinned = backend.pin()?.unwrap_or(backend);
    for (role, expected) in &routes {
        if pinned.inference_target(role)?.as_ref() != Some(expected) {
            bail!("Saved inference route target is unavailable")
        }
    }
    Ok(Arc::new(RestrictedRoutes {
        backend: pinned,
        routes,
    }))
}

impl InferenceBackend for RestrictedRoutes {
    fn prepare_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let expected = self
                .routes
                .get(role)
                .ok_or_else(|| anyhow!("Role is absent from the saved inference route map"))?;
            if self.backend.inference_target(role)?.as_ref() != Some(expected) {
                bail!("Saved inference target changed before preparation")
            }
            self.backend.prepare_for(task, role, prompt).await
        })
    }
    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        self.backend.input_token_admission()
    }
    fn inference_target(&self, role: &str) -> Result<Option<crate::backend::InferenceTarget>> {
        Ok(self.routes.get(role).cloned())
    }

    fn availability(&self) -> Availability {
        self.backend.availability()
    }

    fn module_descriptor(&self) -> Option<&ModuleDescriptor> {
        self.backend.module_descriptor()
    }

    fn modules(&self) -> Result<Vec<ModuleDescriptor>> {
        let allowed = self.routes.keys().cloned().collect::<BTreeSet<_>>();
        Ok(self
            .backend
            .modules()?
            .into_iter()
            .filter_map(|mut module| {
                module.roles.retain(|role| allowed.contains(role));
                (!module.roles.is_empty()).then_some(module)
            })
            .collect())
    }

    fn pin(&self) -> Result<Option<Arc<dyn InferenceBackend>>> {
        Ok(Some(Arc::new(self.clone())))
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
            let expected = self
                .routes
                .get(role)
                .ok_or_else(|| anyhow!("Role is absent from the saved inference route map"))?;
            if self.backend.inference_target(role)?.as_ref() != Some(expected) {
                bail!("Saved inference target changed")
            }
            self.backend
                .generate_for(task, role, prompt, max_tokens)
                .await
        })
    }
}

impl InferenceBackend for Broker {
    fn prepare_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { self.snapshot()?.prepare_for(task, role, prompt).await })
    }
    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        let registry = self.registry.read().ok()?;
        let mut settings = registry
            .active
            .values()
            .map(|module| module.backend.input_token_admission());
        let first = settings.next()?;
        settings
            .all(|setting| setting == first)
            .then_some(first)
            .flatten()
    }
    fn inference_target(&self, role: &str) -> Result<Option<crate::backend::InferenceTarget>> {
        self.snapshot()?.inference_target(role)
    }
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            let snapshot = self.snapshot()?;
            if snapshot.input_token_admission() != self.input_token_admission() {
                bail!("Inference route token-admission capability changed before dispatch")
            }
            snapshot.generate_for(task, role, prompt, max_tokens).await
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
        Box::pin(async move {
            let snapshot = self.snapshot()?;
            if snapshot.input_token_admission() != self.input_token_admission() {
                bail!("Inference route token-admission capability changed before dispatch")
            }
            snapshot.generate(role, prompt, max_tokens).await
        })
    }
}
impl InferenceBackend for Pinned {
    fn prepare_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let actor = actor(role)?;
            TaskPolicy::reasoning(task)?.authorize(task, actor, "llm.generate")?;
            if prompt.len() > 32_000 {
                bail!("Inference preparation prompt exceeds limits")
            }
            let module = self
                .routes
                .get(role)
                .context("No registered module for inference role")?;
            let backend = self
                .route_backends
                .as_ref()
                .and_then(|routes| routes.get(role))
                .unwrap_or(&module.backend);
            if backend.input_token_admission() != self.input_token_admission()
                || *module.shutdown.borrow()
                || module.availability().unavailable()
            {
                bail!("Pinned inference route is unavailable for preparation")
            }
            backend.prepare_for(task, role, prompt).await
        })
    }
    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        let settings = if let Some(backends) = &self.route_backends {
            backends
                .values()
                .map(|backend| backend.input_token_admission())
                .collect::<Vec<_>>()
        } else {
            self.routes
                .values()
                .map(|module| module.backend.input_token_admission())
                .collect::<Vec<_>>()
        };
        let first = *settings.first()?;
        settings
            .iter()
            .all(|setting| *setting == first)
            .then_some(first)
            .flatten()
    }
    fn inference_target(&self, role: &str) -> Result<Option<crate::backend::InferenceTarget>> {
        if let Some(targets) = &self.resource_targets {
            return Ok(targets.get(role).cloned());
        }
        let module = self
            .routes
            .get(role)
            .context("No registered module for inference role")?;
        module.backend.inference_target(role)
    }
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
            let selected_backend = self
                .route_backends
                .as_ref()
                .and_then(|backends| backends.get(role))
                .unwrap_or(&module.backend);
            if selected_backend.input_token_admission() != self.input_token_admission() {
                bail!("Pinned inference route token-admission capability changed before dispatch")
            }
            if *module.shutdown.borrow() {
                bail!("Module broker shut down")
            }
            if module.availability().unavailable() {
                bail!("Pinned inference module is permanently closed")
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
                    backend: self
                        .route_backends
                        .as_ref()
                        .and_then(|backends| backends.get(role).cloned()),
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
