//! Version-pinned deterministic tool modules with independently checked results.
use anyhow::{bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch, OnceCell};
#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/tool_generated.rs"));
}
use generated::tool as fb;
pub mod sql;
pub mod unix;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Descriptor {
    pub id: String,
    pub version: String,
    pub capability: String,
}
impl Descriptor {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
            || self.version.len() > 64
            || !matches!(self.capability.as_str(), "math.evaluate" | "sql.query")
        {
            bail!("Invalid tool module descriptor")
        }
        semver::Version::parse(&self.version)?;
        Ok(())
    }
}
pub trait ArithmeticBackend: Send + Sync {
    fn availability(&self) -> super::Availability {
        super::Availability::Unknown
    }
    /// Fixed transport identity, when the backend is bound to a remote grant.
    fn module_descriptor(&self) -> Option<&Descriptor> {
        None
    }

    fn evaluate<'a>(
        &'a self,
        expression: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;

    fn evaluate_for<'a>(
        &'a self,
        _task: &'a str,
        expression: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        self.evaluate(expression)
    }
}
struct LocalMath;
impl ArithmeticBackend for LocalMath {
    fn evaluate<'a>(
        &'a self,
        expression: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move { crate::math::calculate(expression) })
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Identity {
    correlation: String,
    task: String,
    actor: String,
    module: String,
    version: String,
    expires: u64,
}
struct Packet {
    identity: Identity,
    expression: Option<String>,
    answer: Option<String>,
    failed: bool,
}
fn encode(packet: &Packet) -> Result<Vec<u8>> {
    let i = &packet.identity;
    let mut b = flatbuffers::FlatBufferBuilder::new();
    let args = fb::PacketArgs {
        version: 1,
        correlation_id: Some(b.create_string(&i.correlation)),
        task_id: Some(b.create_string(&i.task)),
        actor: Some(b.create_string(&i.actor)),
        module_id: Some(b.create_string(&i.module)),
        module_version: Some(b.create_string(&i.version)),
        capability: Some(b.create_string("math.evaluate")),
        expires_at_ms: i.expires,
        expression: packet.expression.as_ref().map(|v| b.create_string(v)),
        answer: packet.answer.as_ref().map(|v| b.create_string(v)),
        failed: packet.failed,
    };
    let root = fb::Packet::create(&mut b, &args);
    fb::finish_packet_buffer(&mut b, root);
    let bytes = b.finished_data().to_vec();
    decode(&bytes)?;
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> Result<Packet> {
    if !(8..=8192).contains(&bytes.len()) || !fb::packet_buffer_has_identifier(bytes) {
        bail!("Invalid capability frame")
    }
    let p = fb::root_as_packet(bytes)?;
    if p.version() != 1 || p.actor() != "jev-plus" || p.capability() != "math.evaluate" {
        bail!("Invalid capability protocol or actor")
    }
    uuid::Uuid::parse_str(p.task_id())?;
    uuid::Uuid::parse_str(p.correlation_id())?;
    Descriptor {
        id: p.module_id().into(),
        version: p.module_version().into(),
        capability: p.capability().into(),
    }
    .validate()?;
    let expression = p.expression().map(String::from);
    let answer = p.answer().map(String::from);
    if expression
        .as_ref()
        .is_some_and(|s| s.is_empty() || s.len() > 120)
        || answer.as_ref().is_some_and(|s| s.len() > 256)
        || !matches!(
            (&expression, &answer, p.failed()),
            (Some(_), None, false) | (None, Some(_), false) | (None, None, true)
        )
    {
        bail!("Invalid capability packet shape or bounds")
    }
    Ok(Packet {
        identity: Identity {
            correlation: p.correlation_id().into(),
            task: p.task_id().into(),
            actor: p.actor().into(),
            module: p.module_id().into(),
            version: p.module_version().into(),
            expires: p.expires_at_ms(),
        },
        expression,
        answer,
        failed: p.failed(),
    })
}
struct Work {
    bytes: Vec<u8>,
    identity: Identity,
    reply: oneshot::Sender<Result<Vec<u8>>>,
}
#[derive(Clone)]
enum Backend {
    Arithmetic(Arc<dyn ArithmeticBackend>),
    Sql(Arc<dyn sql::SqlBackend>),
}
impl Backend {
    fn availability(&self) -> super::Availability {
        match self {
            Self::Arithmetic(backend) => backend.availability(),
            Self::Sql(backend) => backend.availability(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ModuleStatus {
    pub descriptor: Descriptor,
    pub active: bool,
    pub availability: super::Availability,
    pub routing_rank: Option<u32>,
}
struct Module {
    descriptor: Descriptor,
    backend: Backend,
    queue: OnceCell<mpsc::Sender<Work>>,
    processing: Arc<AtomicBool>,
    stop: Arc<watch::Sender<bool>>,
    routing_rank: Option<u32>,
}
impl Module {
    fn availability(&self) -> super::Availability {
        if self.queue.get().is_some_and(mpsc::Sender::is_closed) {
            super::Availability::Closed
        } else {
            self.backend.availability()
        }
    }
    async fn queue(&self) -> &mpsc::Sender<Work> {
        self.queue
            .get_or_init(|| async {
                let (send, mut recv) = mpsc::channel::<Work>(4);
                let descriptor = self.descriptor.clone();
                let backend = self.backend.clone();
                let processing = self.processing.clone();
                let mut stop = self.stop.subscribe();
                tokio::spawn(async move {
                    loop {
                        let mut work = tokio::select! {
                            biased;
                            _ = stop.wait_for(|v| *v) => break,
                            work = recv.recv() => match work {
                                Some(work) => work,
                                None => break,
                            },
                        };
                        if work.reply.is_closed() {
                            continue;
                        }
                        let _processing = super::Processing::new(processing.clone());
                        let execution = async {
                            let backend = match &backend {
                                Backend::Sql(backend) => {
                                    return sql::execute(
                                        &work.bytes,
                                        &work.identity,
                                        &descriptor,
                                        backend.as_ref(),
                                    )
                                    .await
                                }
                                Backend::Arithmetic(backend) => backend,
                            };
                            let request = decode(&work.bytes)?;
                            if request.identity != work.identity
                                || request.identity.module != descriptor.id
                                || request.identity.version != descriptor.version
                                || request.identity.expires <= super::protocol::now_ms()
                            {
                                bail!("Capability identity or deadline mismatch")
                            }
                            crate::security::TaskPolicy::reasoning(&request.identity.task)?
                                .authorize(
                                    &request.identity.task,
                                    &request.identity.actor,
                                    &descriptor.capability,
                                )?;
                            let expression = request
                                .expression
                                .context("Capability worker expects a request")?;
                            let result = backend
                                .evaluate_for(&request.identity.task, &expression)
                                .await;
                            let response = match result {
                                Ok(answer) if answer.len() <= 256 => Packet {
                                    identity: request.identity,
                                    expression: None,
                                    answer: Some(answer),
                                    failed: false,
                                },
                                _ => Packet {
                                    identity: request.identity,
                                    expression: None,
                                    answer: None,
                                    failed: true,
                                },
                            };
                            encode(&response)
                        };
                        let result = tokio::select! {
                            biased;
                            _ = stop.wait_for(|v| *v) => break,
                            _ = work.reply.closed() => continue,
                            result = execution => result,
                        };
                        let _ = work.reply.send(result);
                    }
                });
                send
            })
            .await
    }
}
#[derive(Default)]
struct Registry {
    active: BTreeMap<String, Arc<Module>>,
    seen: BTreeSet<(String, String)>,
    capabilities: BTreeMap<String, String>,
    versions: BTreeMap<(String, String), Arc<Module>>,
}
pub struct Broker {
    registry: RwLock<Registry>,
    stop: Arc<watch::Sender<bool>>,
}
impl Default for Broker {
    fn default() -> Self {
        let broker = Self::with_arithmetic(Arc::new(LocalMath))
            .expect("Built-in arithmetic descriptor is valid");
        broker
            .register_sql(
                Descriptor {
                    id: "sql.local".into(),
                    version: "1.0.0".into(),
                    capability: "sql.query".into(),
                },
                Arc::new(sql::LocalSql),
            )
            .expect("Built-in SQL descriptor is valid");
        broker
    }
}
impl Broker {
    pub(crate) fn with_arithmetic(backend: Arc<dyn ArithmeticBackend>) -> Result<Self> {
        let (stop, _) = watch::channel(false);
        let broker = Self {
            registry: RwLock::new(Registry::default()),
            stop: Arc::new(stop),
        };
        broker.register(
            Descriptor {
                id: "arithmetic.local".into(),
                version: "1.0.0".into(),
                capability: "math.evaluate".into(),
            },
            backend,
        )?;
        Ok(broker)
    }
    pub(crate) fn with_sql(backend: Arc<dyn sql::SqlBackend>) -> Result<Self> {
        let (stop, _) = watch::channel(false);
        let broker = Self {
            registry: RwLock::new(Registry::default()),
            stop: Arc::new(stop),
        };
        broker.register_sql(
            Descriptor {
                id: "sql.local".into(),
                version: "1.0.0".into(),
                capability: "sql.query".into(),
            },
            backend,
        )?;
        Ok(broker)
    }
    pub fn register(
        &self,
        descriptor: Descriptor,
        backend: Arc<dyn ArithmeticBackend>,
    ) -> Result<()> {
        self.register_arithmetic_ranked(descriptor, backend, None)
    }
    /// Explicit host approval for alternative arithmetic modules; lower rank wins.
    pub fn register_candidate(
        &self,
        descriptor: Descriptor,
        backend: Arc<dyn ArithmeticBackend>,
        rank: u32,
    ) -> Result<()> {
        self.register_arithmetic_ranked(descriptor, backend, Some(rank))
    }
    fn register_arithmetic_ranked(
        &self,
        descriptor: Descriptor,
        backend: Arc<dyn ArithmeticBackend>,
        rank: Option<u32>,
    ) -> Result<()> {
        descriptor.validate()?;
        if backend
            .module_descriptor()
            .is_some_and(|bound| bound != &descriptor)
        {
            bail!("Capability registration differs from backend transport identity")
        }
        if descriptor.capability != "math.evaluate" {
            bail!("Arithmetic registration requires math.evaluate")
        }
        self.register_backend(descriptor, Backend::Arithmetic(backend), rank)
    }
    pub fn register_sql(
        &self,
        descriptor: Descriptor,
        backend: Arc<dyn sql::SqlBackend>,
    ) -> Result<()> {
        self.register_sql_ranked(descriptor, backend, None)
    }
    /// Explicit host approval for alternative SQL modules; lower rank wins.
    pub fn register_sql_candidate(
        &self,
        descriptor: Descriptor,
        backend: Arc<dyn sql::SqlBackend>,
        rank: u32,
    ) -> Result<()> {
        self.register_sql_ranked(descriptor, backend, Some(rank))
    }
    fn register_sql_ranked(
        &self,
        descriptor: Descriptor,
        backend: Arc<dyn sql::SqlBackend>,
        rank: Option<u32>,
    ) -> Result<()> {
        descriptor.validate()?;
        if descriptor.capability != "sql.query" {
            bail!("SQL registration requires sql.query")
        }
        if backend
            .module_descriptor()
            .is_some_and(|bound| bound != &descriptor)
        {
            bail!("SQL registration differs from backend transport identity")
        }
        self.register_backend(descriptor, Backend::Sql(backend), rank)
    }
    fn register_backend(
        &self,
        descriptor: Descriptor,
        backend: Backend,
        routing_rank: Option<u32>,
    ) -> Result<()> {
        if routing_rank.is_some_and(|rank| rank > 1_000_000) {
            bail!("Capability routing rank exceeds bounds")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
        if *self.stop.borrow()
            || registry.seen.len() >= 128
            || registry
                .seen
                .contains(&(descriptor.id.clone(), descriptor.version.clone()))
        {
            bail!("Capability version is immutable or registry stopped/full")
        }
        if registry
            .capabilities
            .get(&descriptor.id)
            .is_some_and(|capability| capability != &descriptor.capability)
        {
            bail!("A module cannot change its capability during replacement")
        }
        if !registry.active.contains_key(&descriptor.id) && registry.active.len() >= 32 {
            bail!("Active capability module capacity exhausted")
        }
        if registry.active.values().any(|m| {
            m.descriptor.id != descriptor.id
                && m.descriptor.capability == descriptor.capability
                && (routing_rank.is_none() || m.routing_rank.is_none())
        }) {
            bail!("Conflicting active capability module")
        }
        registry
            .seen
            .insert((descriptor.id.clone(), descriptor.version.clone()));
        registry
            .capabilities
            .insert(descriptor.id.clone(), descriptor.capability.clone());
        let module = Arc::new(Module {
            descriptor,
            backend,
            queue: OnceCell::new(),
            processing: Arc::new(AtomicBool::new(false)),
            stop: self.stop.clone(),
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
    /// Trusted host lifecycle API; reuses an immutable registered backend.
    /// Activation changes future pins only and grants no task permission.
    pub fn activate(&self, id: &str, version: &str) -> Result<()> {
        if id.len() > 64 || version.len() > 64 {
            bail!("Invalid capability version selection")
        }
        let module = {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
            if *self.stop.borrow() {
                bail!("Capability registry stopped")
            }
            registry
                .versions
                .get(&(id.into(), version.into()))
                .cloned()
                .context("Unknown registered capability version")?
        };
        if module.availability().unavailable() {
            bail!("Registered capability transport is unavailable")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
        if *self.stop.borrow() {
            bail!("Capability registry stopped")
        }
        let retained = registry
            .versions
            .get(&(id.into(), version.into()))
            .context("Capability version retired during observation")?;
        if !Arc::ptr_eq(retained, &module) {
            bail!("Capability identity changed during observation")
        }
        if !registry.active.contains_key(id) && registry.active.len() >= 32 {
            bail!("Active capability module capacity exhausted")
        }
        let descriptor = &module.descriptor;
        if registry
            .active
            .get(id)
            .is_some_and(|m| m.descriptor.capability != descriptor.capability)
            || registry.active.values().any(|m| {
                m.descriptor.id != id
                    && m.descriptor.capability == descriptor.capability
                    && (module.routing_rank.is_none() || m.routing_rank.is_none())
            })
        {
            bail!("Conflicting active capability module")
        }
        registry.active.insert(id.into(), module);
        Ok(())
    }
    /// Release an inactive backend from future rollback, preserving task pins.
    /// Version and capability identities stay reserved for this broker's lifetime.
    pub fn retire(&self, id: &str, version: &str) -> Result<()> {
        if id.len() > 64 || version.len() > 64 {
            bail!("Invalid capability version selection")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
        if *self.stop.borrow() {
            bail!("Capability registry stopped")
        }
        if registry
            .active
            .get(id)
            .is_some_and(|module| module.descriptor.version == version)
        {
            bail!("Cannot retire an active capability version")
        }
        let retired = registry
            .versions
            .remove(&(id.into(), version.into()))
            .context("Unknown retained capability version")?;
        // Host backend destructors may inspect committed registry metadata.
        drop(registry);
        drop(retired);
        Ok(())
    }

    /// Registered metadata, including inactive versions; not a backend health probe.
    pub fn versions(&self) -> Result<Vec<Descriptor>> {
        Ok(self
            .registry
            .read()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?
            .versions
            .values()
            .map(|m| m.descriptor.clone())
            .collect())
    }
    pub fn unregister(&self, id: &str) -> Result<()> {
        self.registry
            .write()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?
            .active
            .remove(id)
            .context("Unknown capability module")?;
        Ok(())
    }
    /// Retained version observations without provider probes or renewed grants.
    pub fn statuses(&self) -> Result<Vec<ModuleStatus>> {
        // Freeze bounded membership and activation metadata, then release the
        // lock before invoking trusted backend observation callbacks.
        let snapshot = {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
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
            })
            .collect())
    }

    pub fn discover(&self) -> Result<Vec<Descriptor>> {
        Ok(self
            .registry
            .read()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?
            .active
            .values()
            .map(|m| m.descriptor.clone())
            .collect())
    }
    pub fn pin(&self) -> Result<Option<Handle>> {
        self.pin_capability("math.evaluate")
    }
    pub fn pin_capability(&self, capability: &str) -> Result<Option<Handle>> {
        if !matches!(capability, "math.evaluate" | "sql.query") {
            bail!("Unsupported tool capability")
        }
        let candidates = {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
            if *self.stop.borrow() {
                bail!("Capability registry stopped")
            }
            registry.active.values().cloned().collect::<Vec<_>>()
        };
        Ok(candidates
            .into_iter()
            .filter(|module| module.descriptor.capability == capability)
            .filter_map(|module| {
                let (full, load) = super::routing_pressure(
                    module.queue.get(),
                    module.processing.load(Ordering::Acquire),
                );
                if full || module.availability().unavailable() {
                    None
                } else {
                    Some((load, module))
                }
            })
            .min_by_key(|(load, module)| {
                (
                    module.routing_rank.unwrap_or(0),
                    *load,
                    module.descriptor.id.clone(),
                )
            })
            .map(|(_, module)| Handle { module }))
    }

    pub fn shutdown(&self) -> Result<()> {
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
        self.stop.send_replace(true);
        let retired = std::mem::take(&mut registry.versions);
        registry.active.clear();
        // Host backend destructors may inspect committed registry metadata.
        drop(registry);
        drop(retired);
        Ok(())
    }
}
#[derive(Clone)]
pub struct Handle {
    module: Arc<Module>,
}
impl Handle {
    pub fn descriptor(&self) -> &Descriptor {
        &self.module.descriptor
    }
    pub(crate) async fn evaluate(
        &self,
        task: &str,
        actor: &str,
        expression: &str,
    ) -> Result<String> {
        crate::security::TaskPolicy::reasoning(task)?.authorize(task, actor, "math.evaluate")?;
        if *self.module.stop.borrow() || self.module.descriptor.capability != "math.evaluate" {
            bail!("Arithmetic capability unavailable")
        }
        if expression.is_empty() || expression.len() > 120 {
            bail!("Arithmetic request exceeds bounds")
        }
        let identity = Identity {
            correlation: uuid::Uuid::new_v4().to_string(),
            task: task.into(),
            actor: actor.into(),
            module: self.module.descriptor.id.clone(),
            version: self.module.descriptor.version.clone(),
            expires: super::protocol::now_ms().saturating_add(30000),
        };
        let bytes = encode(&Packet {
            identity: identity.clone(),
            expression: Some(expression.into()),
            answer: None,
            failed: false,
        })?;
        let response = decode(&self.dispatch(bytes, identity.clone()).await?)?;
        if response.identity != identity || response.identity.expires <= super::protocol::now_ms() {
            bail!("Capability response identity or deadline mismatch")
        }
        if response.failed {
            bail!("Arithmetic module rejected the request")
        }
        response.answer.context("Capability response has no answer")
    }
    pub(crate) async fn query_sql(&self, task: &str, actor: &str, raw: &str) -> Result<String> {
        crate::security::TaskPolicy::reasoning(task)?.authorize(task, actor, "sql.query")?;
        if self.module.descriptor.capability != "sql.query" || *self.module.stop.borrow() {
            bail!("SQL capability unavailable")
        }
        let identity = Identity {
            correlation: uuid::Uuid::new_v4().to_string(),
            task: task.into(),
            actor: actor.into(),
            module: self.module.descriptor.id.clone(),
            version: self.module.descriptor.version.clone(),
            expires: super::protocol::now_ms().saturating_add(30000),
        };
        let bytes = sql::request(&identity, raw)?;
        let response = self.dispatch(bytes, identity.clone()).await?;
        sql::response(&response, &identity)
    }
    async fn dispatch(&self, bytes: Vec<u8>, identity: Identity) -> Result<Vec<u8>> {
        if self.module.availability().unavailable() {
            bail!("Pinned capability transport is unavailable")
        }
        tokio::time::timeout(Duration::from_secs(30), async {
            let (reply, receive) = oneshot::channel();
            self.module
                .queue()
                .await
                .send(Work {
                    bytes,
                    identity: identity.clone(),
                    reply,
                })
                .await
                .context("Capability queue closed")?;
            receive.await.context("Capability worker stopped")?
        })
        .await
        .context("Capability dispatch deadline exhausted")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn retirement_releases_tools_without_losing_pins_or_capability_identity() {
        let arithmetic = Arc::new(Fixed("42"));
        let math_weak = Arc::downgrade(&arithmetic);
        let broker = Broker::with_arithmetic(arithmetic).unwrap();
        let sql_backend = Arc::new(sql::LocalSql);
        let sql_weak = Arc::downgrade(&sql_backend);
        let sql_descriptor = Descriptor {
            id: "sql.local".into(),
            version: "1.0.0".into(),
            capability: "sql.query".into(),
        };
        broker
            .register_sql(sql_descriptor.clone(), sql_backend)
            .unwrap();
        let math_pin = broker.pin().unwrap().unwrap();
        let sql_pin = broker.pin_capability("sql.query").unwrap().unwrap();
        for id in ["arithmetic.local", "sql.local"] {
            assert!(broker.retire(id, "1.0.0").is_err());
            broker.unregister(id).unwrap();
            broker.retire(id, "1.0.0").unwrap();
            assert!(broker.activate(id, "1.0.0").is_err());
            assert!(broker.retire(id, "1.0.0").is_err());
        }
        assert!(broker.versions().unwrap().is_empty());
        assert!(broker.pin().unwrap().is_none());
        assert!(broker.pin_capability("sql.query").unwrap().is_none());
        assert!(broker
            .register(descriptor("1.0.0"), Arc::new(Fixed("forged")))
            .is_err());
        assert!(broker
            .register_sql(sql_descriptor, Arc::new(sql::LocalSql))
            .is_err());
        assert!(broker
            .register_sql(
                Descriptor {
                    id: "arithmetic.local".into(),
                    version: "2.0.0".into(),
                    capability: "sql.query".into(),
                },
                Arc::new(sql::LocalSql)
            )
            .is_err());
        assert!(broker
            .register(
                Descriptor {
                    id: "sql.local".into(),
                    version: "2.0.0".into(),
                    capability: "math.evaluate".into(),
                },
                Arc::new(Fixed("forged"))
            )
            .is_err());
        let task = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            math_pin.evaluate(&task, "jev-plus", "6*7").await.unwrap(),
            "42"
        );
        assert!(sql_pin
            .query_sql(&task, "interface", r#"{"tables":[],"query":"SELECT 1"}"#)
            .await
            .is_err());
        let answer = sql_pin
            .query_sql(
                &task,
                "jev-plus",
                r#"{"tables":[],"query":"SELECT 2 + 3 AS total"}"#,
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&answer).unwrap()["rows"],
            serde_json::json!([[5]])
        );
        drop(math_pin);
        drop(sql_pin);
        tokio::time::timeout(Duration::from_secs(1), async {
            while math_weak.upgrade().is_some() || sql_weak.upgrade().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Retired tool backends must release after their last pin closes");
        broker
            .register(descriptor("2.0.0"), Arc::new(Fixed("new version")))
            .unwrap();
        assert_eq!(broker.pin().unwrap().unwrap().descriptor().version, "2.0.0");
    }

    struct InventoryObserver(std::sync::Weak<Broker>);
    impl InventoryObserver {
        fn observe(&self) -> super::super::Availability {
            let broker = self.0.upgrade().unwrap();
            assert!(
                broker.registry.try_write().is_ok(),
                "Inventory callback must run outside registry locks"
            );
            assert!(!broker.versions().unwrap().is_empty());
            super::super::Availability::Connected
        }
    }
    impl ArithmeticBackend for InventoryObserver {
        fn availability(&self) -> super::super::Availability {
            self.observe()
        }
        fn evaluate<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async { bail!("Inventory must not execute tools") })
        }
    }
    impl sql::SqlBackend for InventoryObserver {
        fn availability(&self) -> super::super::Availability {
            self.observe()
        }
        fn query<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async { bail!("Inventory must not execute SQL") })
        }
    }
    #[test]
    fn inventory_tool_observation_can_inspect_registry_without_lock_recursion() {
        for sql in [false, true] {
            let broker = Arc::new(Broker::default());
            let capability = if sql { "sql.query" } else { "math.evaluate" };
            broker
                .unregister(if sql { "sql.local" } else { "arithmetic.local" })
                .unwrap();
            let descriptor = Descriptor {
                id: "observer".into(),
                version: "1.0.0".into(),
                capability: capability.into(),
            };
            let backend = Arc::new(InventoryObserver(Arc::downgrade(&broker)));
            if sql {
                broker.register_sql(descriptor, backend).unwrap();
            } else {
                broker.register(descriptor, backend).unwrap();
            }
            let observed = broker.statuses().unwrap();
            let observer = observed
                .iter()
                .find(|m| m.descriptor.id == "observer")
                .unwrap();
            assert!(observer.active);
            assert_eq!(observer.availability, super::super::Availability::Connected);
            assert_eq!(
                broker
                    .pin_capability(capability)
                    .unwrap()
                    .unwrap()
                    .descriptor()
                    .id,
                "observer"
            );
            broker.activate("observer", "1.0.0").unwrap();
        }
    }

    struct RegistryCleanup {
        broker: std::sync::Weak<Broker>,
        completed: Arc<std::sync::atomic::AtomicBool>,
    }
    impl Drop for RegistryCleanup {
        fn drop(&mut self) {
            let broker = self.broker.upgrade().unwrap();
            assert!(
                broker.registry.try_read().is_ok(),
                "Tool cleanup must run outside the registry lock"
            );
            assert!(!broker
                .versions()
                .unwrap()
                .iter()
                .any(|version| version.id == "cleanup"));
            self.completed
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
    impl ArithmeticBackend for RegistryCleanup {
        fn evaluate<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async { bail!("Cleanup fixture does not execute tools") })
        }
    }
    impl sql::SqlBackend for RegistryCleanup {
        fn query<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async { bail!("Cleanup fixture does not execute SQL") })
        }
    }
    #[test]
    fn retirement_and_shutdown_tool_cleanup_can_inspect_the_committed_registry() {
        for (sql, shutdown) in [(false, false), (true, false), (false, true), (true, true)] {
            let broker = Arc::new(Broker::default());
            broker
                .unregister(if sql { "sql.local" } else { "arithmetic.local" })
                .unwrap();
            let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let backend = Arc::new(RegistryCleanup {
                broker: Arc::downgrade(&broker),
                completed: completed.clone(),
            });
            let descriptor = Descriptor {
                id: "cleanup".into(),
                version: "1.0.0".into(),
                capability: if sql { "sql.query" } else { "math.evaluate" }.into(),
            };
            if sql {
                broker.register_sql(descriptor, backend).unwrap();
            } else {
                broker.register(descriptor, backend).unwrap();
            }
            if shutdown {
                broker.shutdown().unwrap();
            } else {
                broker.unregister("cleanup").unwrap();
                broker.retire("cleanup", "1.0.0").unwrap();
            }
            assert!(completed.load(std::sync::atomic::Ordering::Acquire));
        }
    }

    struct QueueGate {
        calls: std::sync::atomic::AtomicUsize,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    impl QueueGate {
        async fn wait(&self) {
            if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                self.entered.notify_one();
                self.release.notified().await;
            }
        }
    }
    impl ArithmeticBackend for QueueGate {
        fn evaluate<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async move {
                self.wait().await;
                Ok("42".into())
            })
        }
    }
    impl sql::SqlBackend for QueueGate {
        fn query<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async move {
                self.wait().await;
                Ok(r#"{"columns":["answer"],"rows":[[42]]}"#.into())
            })
        }
    }
    async fn queued_tool_call(handle: Handle, task: String, sql: bool) -> Result<String> {
        if sql {
            handle
                .query_sql(
                    &task,
                    "jev-plus",
                    r#"{"tables":[],"query":"SELECT 42 AS answer"}"#,
                )
                .await
        } else {
            handle.evaluate(&task, "jev-plus", "6*7").await
        }
    }
    #[tokio::test]
    async fn executing_tools_affect_new_routes_and_clear_after_completion_or_cancellation() {
        for sql in [false, true] {
            for cancel in [false, true] {
                let broker = Broker::default();
                let capability = if sql { "sql.query" } else { "math.evaluate" };
                broker
                    .unregister(if sql { "sql.local" } else { "arithmetic.local" })
                    .unwrap();
                let gate = Arc::new(QueueGate {
                    calls: std::sync::atomic::AtomicUsize::new(0),
                    entered: tokio::sync::Notify::new(),
                    release: tokio::sync::Notify::new(),
                });
                for id in ["candidate.a", "candidate.b"] {
                    let descriptor = Descriptor {
                        id: id.into(),
                        version: "1.0.0".into(),
                        capability: capability.into(),
                    };
                    if sql {
                        broker
                            .register_sql_candidate(descriptor, gate.clone(), 10)
                            .unwrap();
                    } else {
                        broker
                            .register_candidate(descriptor, gate.clone(), 10)
                            .unwrap();
                    }
                }
                let original = broker.pin_capability(capability).unwrap().unwrap();
                assert_eq!(original.descriptor().id, "candidate.a");
                let task = uuid::Uuid::new_v4().to_string();
                let call = tokio::spawn(queued_tool_call(original.clone(), task.clone(), sql));
                tokio::time::timeout(Duration::from_secs(1), gate.entered.notified())
                    .await
                    .unwrap();
                assert_eq!(original.module.queue().await.capacity(), 4);
                assert!(original.module.processing.load(Ordering::Acquire));
                assert_eq!(
                    broker
                        .pin_capability(capability)
                        .unwrap()
                        .unwrap()
                        .descriptor()
                        .id,
                    "candidate.b"
                );
                // Existing handles retain their original module throughout execution.
                assert_eq!(original.descriptor().id, "candidate.a");
                if cancel {
                    call.abort();
                    assert!(call.await.unwrap_err().is_cancelled());
                } else {
                    gate.release.notify_one();
                    tokio::time::timeout(Duration::from_secs(1), call)
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap();
                }
                tokio::time::timeout(Duration::from_secs(1), async {
                    while original.module.processing.load(Ordering::Acquire) {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                assert_eq!(
                    broker
                        .pin_capability(capability)
                        .unwrap()
                        .unwrap()
                        .descriptor()
                        .id,
                    "candidate.a"
                );
                queued_tool_call(original, task, sql).await.unwrap();
                assert_eq!(gate.calls.load(Ordering::SeqCst), 2);
            }
        }
    }

    #[tokio::test]
    async fn saturated_tools_skip_new_pins_without_replaying_existing_handles() {
        for sql in [false, true] {
            let broker = Broker::default();
            let capability = if sql { "sql.query" } else { "math.evaluate" };
            broker
                .unregister(if sql { "sql.local" } else { "arithmetic.local" })
                .unwrap();
            let busy = Arc::new(QueueGate {
                calls: std::sync::atomic::AtomicUsize::new(0),
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            });
            let descriptor = |id: &str| Descriptor {
                id: id.into(),
                version: "1.0.0".into(),
                capability: capability.into(),
            };
            if sql {
                broker
                    .register_sql_candidate(descriptor("preferred"), busy.clone(), 10)
                    .unwrap();
                broker
                    .register_sql_candidate(descriptor("fallback"), Arc::new(sql::LocalSql), 20)
                    .unwrap();
            } else {
                broker
                    .register_candidate(descriptor("preferred"), busy.clone(), 10)
                    .unwrap();
                broker
                    .register_candidate(descriptor("fallback"), Arc::new(LocalMath), 20)
                    .unwrap();
            }
            let original = broker.pin_capability(capability).unwrap().unwrap();
            assert_eq!(original.descriptor().id, "preferred");
            let task = uuid::Uuid::new_v4().to_string();
            let mut calls = vec![tokio::spawn(queued_tool_call(
                original.clone(),
                task.clone(),
                sql,
            ))];
            tokio::time::timeout(Duration::from_secs(1), busy.entered.notified())
                .await
                .unwrap();
            for _ in 0..4 {
                calls.push(tokio::spawn(queued_tool_call(
                    original.clone(),
                    task.clone(),
                    sql,
                )));
            }
            let queue = original.module.queue().await;
            tokio::time::timeout(Duration::from_secs(1), async {
                while queue.capacity() != 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            // Tool dispatch retains bounded waiting on its original queue.
            assert!(tokio::time::timeout(
                Duration::from_millis(20),
                queued_tool_call(original.clone(), task.clone(), sql)
            )
            .await
            .is_err());
            assert_eq!(busy.calls.load(Ordering::Acquire), 1);
            let next = broker.pin_capability(capability).unwrap().unwrap();
            assert_eq!(next.descriptor().id, "fallback");
            let expected = queued_tool_call(next, task.clone(), sql).await.unwrap();
            broker.unregister("fallback").unwrap();
            assert!(broker.pin_capability(capability).unwrap().is_none());
            broker.activate("fallback", "1.0.0").unwrap();
            assert_eq!(
                broker
                    .pin_capability(capability)
                    .unwrap()
                    .unwrap()
                    .descriptor()
                    .id,
                "fallback"
            );
            assert_eq!(original.descriptor().id, "preferred");
            busy.release.notify_one();
            for call in calls {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(
                        &tokio::time::timeout(Duration::from_secs(1), call)
                            .await
                            .unwrap()
                            .unwrap()
                            .unwrap()
                    )
                    .unwrap(),
                    serde_json::from_str::<serde_json::Value>(&expected).unwrap()
                );
            }
            assert_eq!(busy.calls.load(Ordering::Acquire), 5);
            assert_eq!(
                broker
                    .pin_capability(capability)
                    .unwrap()
                    .unwrap()
                    .descriptor()
                    .id,
                "preferred"
            );
            queued_tool_call(original, task, sql).await.unwrap();
            assert_eq!(busy.calls.load(Ordering::Acquire), 6);
        }
    }

    #[tokio::test]
    async fn equal_rank_tools_use_queue_pressure_without_changing_pins_or_rank_priority() {
        for sql in [false, true] {
            let broker = Broker::default();
            let capability = if sql { "sql.query" } else { "math.evaluate" };
            broker
                .unregister(if sql { "sql.local" } else { "arithmetic.local" })
                .unwrap();
            let backend = Arc::new(QueueGate {
                calls: std::sync::atomic::AtomicUsize::new(0),
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            });
            let descriptor = |id: &str, version: &str| Descriptor {
                id: id.into(),
                version: version.into(),
                capability: capability.into(),
            };
            if sql {
                broker
                    .register_sql_candidate(descriptor("candidate.a", "1.0.0"), backend.clone(), 10)
                    .unwrap();
                broker
                    .register_sql_candidate(
                        descriptor("candidate.b", "1.0.0"),
                        Arc::new(sql::LocalSql),
                        20,
                    )
                    .unwrap();
            } else {
                broker
                    .register_candidate(descriptor("candidate.a", "1.0.0"), backend.clone(), 10)
                    .unwrap();
                broker
                    .register_candidate(descriptor("candidate.b", "1.0.0"), Arc::new(LocalMath), 20)
                    .unwrap();
            }
            let original = broker.pin_capability(capability).unwrap().unwrap();
            assert_eq!(original.descriptor().id, "candidate.a");
            let task = uuid::Uuid::new_v4().to_string();
            let first = tokio::spawn(queued_tool_call(original.clone(), task.clone(), sql));
            tokio::time::timeout(Duration::from_secs(1), backend.entered.notified())
                .await
                .unwrap();
            let second = tokio::spawn(queued_tool_call(original.clone(), task.clone(), sql));
            let queue = original.module.queue().await;
            tokio::time::timeout(Duration::from_secs(1), async {
                while queue.capacity() != 3 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                broker
                    .pin_capability(capability)
                    .unwrap()
                    .unwrap()
                    .descriptor()
                    .id,
                "candidate.a"
            );
            if sql {
                broker
                    .register_sql_candidate(
                        descriptor("candidate.b", "2.0.0"),
                        Arc::new(sql::LocalSql),
                        10,
                    )
                    .unwrap();
            } else {
                broker
                    .register_candidate(descriptor("candidate.b", "2.0.0"), Arc::new(LocalMath), 10)
                    .unwrap();
            }
            let idle = broker.pin_capability(capability).unwrap().unwrap();
            assert_eq!(idle.descriptor().id, "candidate.b");
            let expected = queued_tool_call(idle, task.clone(), sql).await.unwrap();
            backend.release.notify_one();
            for call in [first, second] {
                let answer = tokio::time::timeout(Duration::from_secs(1), call)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&answer).unwrap(),
                    serde_json::from_str::<serde_json::Value>(&expected).unwrap()
                );
            }
            assert_eq!(original.descriptor().id, "candidate.a");
            queued_tool_call(original, task, sql).await.unwrap();
            assert_eq!(backend.calls.load(std::sync::atomic::Ordering::SeqCst), 3);
            assert_eq!(
                broker
                    .pin_capability(capability)
                    .unwrap()
                    .unwrap()
                    .descriptor()
                    .id,
                "candidate.a"
            );
        }
    }

    struct Fixed(&'static str);
    impl ArithmeticBackend for Fixed {
        fn evaluate<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async move { Ok(self.0.into()) })
        }
    }
    fn descriptor(version: &str) -> Descriptor {
        Descriptor {
            id: "arithmetic.local".into(),
            version: version.into(),
            capability: "math.evaluate".into(),
        }
    }
    #[tokio::test]
    async fn ranked_arithmetic_routes_preserve_pins_rollback_and_authorization() {
        let broker = Broker::default();
        let mut a = descriptor("2.0.0");
        a.id = "math.a".into();
        assert!(broker
            .register_candidate(a.clone(), Arc::new(Fixed("a")), 10)
            .is_err());
        broker.unregister("arithmetic.local").unwrap();
        let mut b = a.clone();
        b.id = "math.b".into();
        broker
            .register_candidate(b.clone(), Arc::new(Fixed("b")), 10)
            .unwrap();
        broker
            .register_candidate(a.clone(), Arc::new(Fixed("a")), 10)
            .unwrap();
        let pinned = broker.pin().unwrap().unwrap();
        assert_eq!(pinned.descriptor().id, "math.a");
        a.version = "3.0.0".into();
        broker
            .register_candidate(a, Arc::new(Fixed("new")), 20)
            .unwrap();
        let task = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            broker
                .pin()
                .unwrap()
                .unwrap()
                .evaluate(&task, "jev-plus", "1+1")
                .await
                .unwrap(),
            "b"
        );
        assert_eq!(
            pinned.evaluate(&task, "jev-plus", "1+1").await.unwrap(),
            "a"
        );
        assert!(pinned.evaluate(&task, "jev-r", "1+1").await.is_err());
        broker.activate("math.a", "2.0.0").unwrap();
        assert_eq!(broker.pin().unwrap().unwrap().descriptor().id, "math.a");
        broker.unregister("math.a").unwrap();
        assert_eq!(broker.pin().unwrap().unwrap().descriptor().id, "math.b");
        assert!(broker.activate("arithmetic.local", "1.0.0").is_err());
        assert!(broker.register(b, Arc::new(Fixed("exclusive"))).is_err());
    }
    #[test]
    fn ranked_tool_capacity_is_bounded_without_consuming_rejected_versions() {
        let broker = Broker::default();
        broker.unregister("arithmetic.local").unwrap();
        for n in 0..31 {
            let mut d = descriptor("2.0.0");
            d.id = format!("math.{n}");
            broker
                .register_candidate(d, Arc::new(LocalMath), n)
                .unwrap();
        }
        let mut extra = descriptor("2.0.0");
        extra.id = "math.extra".into();
        assert!(broker
            .register_candidate(extra.clone(), Arc::new(LocalMath), 1_000_001)
            .is_err());
        assert!(broker
            .register_candidate(extra.clone(), Arc::new(LocalMath), 0)
            .is_err());
        assert_eq!(broker.versions().unwrap().len(), 33);
        broker.unregister("math.0").unwrap();
        broker
            .register_candidate(extra, Arc::new(LocalMath), 0)
            .unwrap();
        assert!(broker.activate("math.0", "2.0.0").is_err());
        broker.unregister("math.extra").unwrap();
        broker.activate("math.0", "2.0.0").unwrap();
    }
    #[tokio::test]
    async fn ranked_sql_routes_keep_typed_queries_and_capability_boundaries() {
        let broker = Broker::default();
        broker.unregister("sql.local").unwrap();
        for (id, rank) in [("sql.b", 20), ("sql.a", 10)] {
            broker
                .register_sql_candidate(
                    Descriptor {
                        id: id.into(),
                        version: "2.0.0".into(),
                        capability: "sql.query".into(),
                    },
                    Arc::new(sql::LocalSql),
                    rank,
                )
                .unwrap();
        }
        let handle = broker.pin_capability("sql.query").unwrap().unwrap();
        assert_eq!(handle.descriptor().id, "sql.a");
        broker.unregister("sql.a").unwrap();
        assert_eq!(
            broker
                .pin_capability("sql.query")
                .unwrap()
                .unwrap()
                .descriptor()
                .id,
            "sql.b"
        );
        let task = uuid::Uuid::new_v4().to_string();
        let answer = handle
            .query_sql(
                &task,
                "jev-plus",
                r#"{"tables":[],"query":"SELECT 42 AS answer"}"#,
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&answer).unwrap()["rows"][0][0],
            42
        );
        assert!(handle
            .query_sql(&task, "jev-r", r#"{"tables":[],"query":"SELECT 42"}"#)
            .await
            .is_err());
        assert!(broker
            .register_sql_candidate(descriptor("9.0.0"), Arc::new(sql::LocalSql), 0)
            .is_err());
        assert!(broker
            .register_candidate(
                Descriptor {
                    id: "wrong.math".into(),
                    version: "1.0.0".into(),
                    capability: "sql.query".into()
                },
                Arc::new(LocalMath),
                0
            )
            .is_err());
    }
    #[tokio::test]
    async fn rollback_reuses_immutable_backend_and_preserves_inflight_pins() {
        let broker = Broker::default();
        let task = uuid::Uuid::new_v4().to_string();
        broker
            .register(descriptor("2.0.0"), Arc::new(Fixed("replacement")))
            .unwrap();
        let replacement = broker.pin().unwrap().unwrap();
        broker.activate("arithmetic.local", "1.0.0").unwrap();
        let rolled_back = broker.pin().unwrap().unwrap();
        assert_eq!(
            rolled_back
                .evaluate(&task, "jev-plus", "6*7")
                .await
                .unwrap(),
            "42"
        );
        assert_eq!(
            replacement
                .evaluate(&task, "jev-plus", "6*7")
                .await
                .unwrap(),
            "replacement"
        );
        assert_eq!(broker.versions().unwrap().len(), 3);
        broker.unregister("arithmetic.local").unwrap();
        broker.activate("arithmetic.local", "2.0.0").unwrap();
        assert_eq!(broker.pin().unwrap().unwrap().descriptor().version, "2.0.0");
        assert!(broker.activate("arithmetic.local", "9.0.0").is_err());
        assert!(broker
            .register(descriptor("1.0.0"), Arc::new(Fixed("forged")))
            .is_err());
        broker.shutdown().unwrap();
        assert!(broker.versions().unwrap().is_empty());
        assert!(broker.activate("arithmetic.local", "1.0.0").is_err());
        assert!(rolled_back
            .evaluate(&task, "jev-plus", "6*7")
            .await
            .is_err());
        assert!(replacement
            .evaluate(&task, "jev-plus", "6*7")
            .await
            .is_err());
    }
    #[tokio::test]
    async fn rollback_refuses_cross_module_capability_conflicts() {
        let broker = Broker::default();
        broker.unregister("arithmetic.local").unwrap();
        let mut alternate = descriptor("1.0.0");
        alternate.id = "alternate.math".into();
        broker.register(alternate, Arc::new(LocalMath)).unwrap();
        assert!(broker.activate("arithmetic.local", "1.0.0").is_err());
        assert_eq!(
            broker.pin().unwrap().unwrap().descriptor().id,
            "alternate.math"
        );
        broker.unregister("alternate.math").unwrap();
        broker.activate("arithmetic.local", "1.0.0").unwrap();
        assert_eq!(
            broker.pin().unwrap().unwrap().descriptor().id,
            "arithmetic.local"
        );
    }
    #[tokio::test]
    async fn routing_preserves_versions_and_shutdown_revokes_old_handles() {
        let broker = Broker::default();
        let old = broker.pin().unwrap().unwrap();
        let task = uuid::Uuid::new_v4().to_string();
        broker
            .register(descriptor("2.0.0"), Arc::new(Fixed("new answer")))
            .unwrap();
        let new = broker.pin().unwrap().unwrap();
        assert_eq!(old.evaluate(&task, "jev-plus", "6*7").await.unwrap(), "42");
        assert_eq!(
            new.evaluate(&task, "jev-plus", "6*7").await.unwrap(),
            "new answer"
        );
        assert!(old.evaluate(&task, "jev-r", "6*7").await.is_err());
        assert!(old
            .evaluate(&task, "jev-plus", &"1".repeat(121))
            .await
            .is_err());
        broker.unregister("arithmetic.local").unwrap();
        assert!(broker.pin().unwrap().is_none());
        assert!(broker
            .register(descriptor("1.0.0"), Arc::new(Fixed("42")))
            .is_err());
        broker.shutdown().unwrap();
        assert!(old.evaluate(&task, "jev-plus", "1+1").await.is_err());
        assert!(new.evaluate(&task, "jev-plus", "1+1").await.is_err());
    }
    #[test]
    fn full_version_history_still_allows_rollback_but_never_redefinition() {
        let broker = Broker::default();
        for version in 2..=127 {
            broker
                .register(descriptor(&format!("{version}.0.0")), Arc::new(LocalMath))
                .unwrap();
        }
        assert_eq!(broker.versions().unwrap().len(), 128);
        assert!(broker
            .register(descriptor("128.0.0"), Arc::new(LocalMath))
            .is_err());
        broker.unregister("arithmetic.local").unwrap();
        broker.activate("arithmetic.local", "1.0.0").unwrap();
        assert_eq!(broker.pin().unwrap().unwrap().descriptor().version, "1.0.0");
        assert_eq!(broker.versions().unwrap().len(), 128);
        assert!(broker
            .register(descriptor("1.0.0"), Arc::new(Fixed("forged")))
            .is_err());
    }
    #[tokio::test]
    async fn engine_independently_checks_replaceable_tool_results_and_charges_once() {
        for answer in ["42", "999"] {
            let store = Arc::new(crate::store::Store::in_memory().unwrap());
            let engine =
                crate::engine::Engine::new(crate::config::Config::default(), store.clone());
            engine
                .tools
                .register(descriptor("2.0.0"), Arc::new(Fixed(answer)))
                .unwrap();
            let outcome = engine.run("calc: 6*7", "fast").await.unwrap();
            assert_eq!(outcome.calls, 0);
            assert_eq!(outcome.resources.tool_calls, 1);
            if answer == "42" {
                assert_eq!(outcome.status, "completed");
                assert_eq!(outcome.answer, "42");
            } else {
                assert_eq!(outcome.status, "failed");
                assert!(!outcome
                    .evidence
                    .iter()
                    .any(|e| e == "verified:arithmetic_parser"));
            }
            let trace = store.task(&outcome.task_id).unwrap();
            assert_eq!(
                trace["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["kind"] == "tool_dispatch")
                    .count(),
                1
            );
        }
    }
    #[tokio::test]
    async fn exhausted_task_budget_prevents_tool_dispatch() {
        let store = Arc::new(crate::store::Store::in_memory().unwrap());
        let config = crate::config::Config {
            max_tool_calls: 0,
            ..Default::default()
        };
        let engine = crate::engine::Engine::new(config, store.clone());
        let outcome = engine.run("calc: 6*7", "fast").await.unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.resources.tool_calls, 0);
        let trace = store.task(&outcome.task_id).unwrap();
        assert!(!trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "tool_dispatch"));
    }

    #[test]
    fn protocol_rejects_malformed_identity_and_unsupported_packet_shapes() {
        assert!(decode(&[0; 16]).is_err());
        assert!(decode(&vec![0; 8193]).is_err());
        let identity = Identity {
            correlation: uuid::Uuid::new_v4().to_string(),
            task: uuid::Uuid::new_v4().to_string(),
            actor: "jev-plus".into(),
            module: "arithmetic.local".into(),
            version: "1.0.0".into(),
            expires: super::super::protocol::now_ms() + 1000,
        };
        let mut packet = Packet {
            identity,
            expression: Some("1+1".into()),
            answer: None,
            failed: false,
        };
        let bytes = encode(&packet).unwrap();
        assert!(decode(&bytes).is_ok());
        packet.answer = Some("2".into());
        assert!(encode(&packet).is_err());
        packet.answer = None;
        packet.identity.actor = "jev-r".into();
        assert!(encode(&packet).is_err());
        packet.identity.actor = "jev-plus".into();
        packet.identity.task = "forged".into();
        assert!(encode(&packet).is_err());
    }
    struct Pending(Arc<tokio::sync::Notify>);
    impl ArithmeticBackend for Pending {
        fn evaluate<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async move {
                self.0.notify_one();
                std::future::pending().await
            })
        }
    }
    #[tokio::test]
    async fn shutdown_cancels_active_and_buffered_capability_calls() {
        let broker = Broker::default();
        let entered = Arc::new(tokio::sync::Notify::new());
        broker
            .register(descriptor("2.0.0"), Arc::new(Pending(entered.clone())))
            .unwrap();
        let handle = broker.pin().unwrap().unwrap();
        let active = handle.clone();
        let task = uuid::Uuid::new_v4().to_string();
        let t = task.clone();
        let first = tokio::spawn(async move { active.evaluate(&t, "jev-plus", "1+1").await });
        entered.notified().await;
        let second = tokio::spawn(async move { handle.evaluate(&task, "jev-plus", "2+2").await });
        broker.shutdown().unwrap();
        assert!(first.await.unwrap().is_err());
        assert!(second.await.unwrap().is_err());
    }
    struct FailedWorker(std::sync::atomic::AtomicUsize);
    impl ArithmeticBackend for FailedWorker {
        fn availability(&self) -> super::super::Availability {
            super::super::Availability::Connected
        }
        fn evaluate<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async move {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                panic!("Expected tool worker failure");
            })
        }
    }
    impl sql::SqlBackend for FailedWorker {
        fn availability(&self) -> super::super::Availability {
            super::super::Availability::Connected
        }
        fn query<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async move {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                panic!("Expected SQL worker failure");
            })
        }
    }
    #[tokio::test]
    async fn failed_tool_queues_override_backend_health_without_replaying_handles() {
        for sql in [false, true] {
            let broker = Broker::default();
            let capability = if sql { "sql.query" } else { "math.evaluate" };
            broker
                .unregister(if sql { "sql.local" } else { "arithmetic.local" })
                .unwrap();
            let descriptor = |id: &str| Descriptor {
                id: id.into(),
                version: "1.0.0".into(),
                capability: capability.into(),
            };
            let failed = Arc::new(FailedWorker(std::sync::atomic::AtomicUsize::new(0)));
            if sql {
                broker
                    .register_sql_candidate(descriptor("failed"), failed.clone(), 10)
                    .unwrap();
                broker
                    .register_sql_candidate(descriptor("healthy"), Arc::new(sql::LocalSql), 20)
                    .unwrap();
            } else {
                broker
                    .register_candidate(descriptor("failed"), failed.clone(), 10)
                    .unwrap();
                broker
                    .register_candidate(descriptor("healthy"), Arc::new(LocalMath), 20)
                    .unwrap();
            }
            let task = uuid::Uuid::new_v4().to_string();
            let original = broker.pin_capability(capability).unwrap().unwrap();
            if sql {
                assert!(original
                    .query_sql(
                        &task,
                        "jev-plus",
                        r#"{"tables":[],"query":"SELECT 42 AS answer"}"#
                    )
                    .await
                    .is_err());
            } else {
                assert!(original.evaluate(&task, "jev-plus", "6*7").await.is_err());
            }
            assert_eq!(
                broker
                    .statuses()
                    .unwrap()
                    .iter()
                    .find(|s| s.descriptor.id == "failed")
                    .unwrap()
                    .availability,
                super::super::Availability::Closed
            );
            assert!(!original.module.processing.load(Ordering::Acquire));
            assert!(broker.activate("failed", "1.0.0").is_err());
            if sql {
                assert!(original
                    .query_sql(
                        &task,
                        "jev-plus",
                        r#"{"tables":[],"query":"SELECT 42 AS answer"}"#
                    )
                    .await
                    .is_err());
            } else {
                assert!(original.evaluate(&task, "jev-plus", "6*7").await.is_err());
            }
            assert_eq!(failed.0.load(std::sync::atomic::Ordering::Acquire), 1);
            let future = broker.pin_capability(capability).unwrap().unwrap();
            assert_eq!(future.descriptor().id, "healthy");
            if sql {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(
                        &future
                            .query_sql(
                                &task,
                                "jev-plus",
                                r#"{"tables":[],"query":"SELECT 42 AS answer"}"#
                            )
                            .await
                            .unwrap()
                    )
                    .unwrap()["rows"],
                    serde_json::json!([[42]])
                );
            } else {
                assert_eq!(
                    future.evaluate(&task, "jev-plus", "6*7").await.unwrap(),
                    "42"
                );
            }
        }
    }
    struct RetireObserver(std::sync::Weak<Broker>);
    impl RetireObserver {
        fn observe(&self) -> super::super::Availability {
            self.0
                .upgrade()
                .unwrap()
                .retire("retiring", "1.0.0")
                .unwrap();
            super::super::Availability::Connected
        }
    }
    impl ArithmeticBackend for RetireObserver {
        fn availability(&self) -> super::super::Availability {
            self.observe()
        }
        fn evaluate<'a>(
            &'a self,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async { bail!("Retired fixture cannot execute") })
        }
    }
    impl sql::SqlBackend for RetireObserver {
        fn availability(&self) -> super::super::Availability {
            self.observe()
        }
        fn query<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
            Box::pin(async { bail!("Retired fixture cannot execute SQL") })
        }
    }
    #[test]
    fn activation_does_not_resurrect_tools_retired_by_health_observation() {
        for sql in [false, true] {
            let broker = Arc::new(Broker::default());
            broker
                .unregister(if sql { "sql.local" } else { "arithmetic.local" })
                .unwrap();
            let descriptor = Descriptor {
                id: "retiring".into(),
                version: "1.0.0".into(),
                capability: if sql { "sql.query" } else { "math.evaluate" }.into(),
            };
            let backend = Arc::new(RetireObserver(Arc::downgrade(&broker)));
            if sql {
                broker.register_sql(descriptor.clone(), backend).unwrap();
            } else {
                broker.register(descriptor.clone(), backend).unwrap();
            }
            broker.unregister("retiring").unwrap();
            assert!(broker
                .activate("retiring", "1.0.0")
                .unwrap_err()
                .to_string()
                .contains("retired"));
            assert!(!broker
                .versions()
                .unwrap()
                .iter()
                .any(|d| d.id == "retiring"));
            assert!(!broker
                .discover()
                .unwrap()
                .iter()
                .any(|d| d.id == "retiring"));
            if sql {
                assert!(broker
                    .register_sql(descriptor, Arc::new(sql::LocalSql))
                    .is_err());
            } else {
                assert!(broker.register(descriptor, Arc::new(LocalMath)).is_err());
            }
        }
    }
}
