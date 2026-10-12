//! Version-pinned deterministic tool modules with independently checked results.
use anyhow::{bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
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
struct Module {
    descriptor: Descriptor,
    backend: Backend,
    queue: OnceCell<mpsc::Sender<Work>>,
    stop: Arc<watch::Sender<bool>>,
}
impl Module {
    async fn queue(&self) -> &mpsc::Sender<Work> {
        self.queue
            .get_or_init(|| async {
                let (send, mut recv) = mpsc::channel::<Work>(4);
                let descriptor = self.descriptor.clone();
                let backend = self.backend.clone();
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
        self.register_backend(descriptor, Backend::Arithmetic(backend))
    }
    pub fn register_sql(
        &self,
        descriptor: Descriptor,
        backend: Arc<dyn sql::SqlBackend>,
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
        self.register_backend(descriptor, Backend::Sql(backend))
    }
    fn register_backend(&self, descriptor: Descriptor, backend: Backend) -> Result<()> {
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
            .active
            .get(&descriptor.id)
            .is_some_and(|m| m.descriptor.capability != descriptor.capability)
        {
            bail!("A module cannot change its capability during replacement")
        }
        if registry.active.values().any(|m| {
            m.descriptor.id != descriptor.id && m.descriptor.capability == descriptor.capability
        }) {
            bail!("Conflicting active capability module")
        }
        registry
            .seen
            .insert((descriptor.id.clone(), descriptor.version.clone()));
        let module = Arc::new(Module {
            descriptor,
            backend,
            queue: OnceCell::new(),
            stop: self.stop.clone(),
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
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
        if *self.stop.borrow() {
            bail!("Capability registry stopped")
        }
        let module = registry
            .versions
            .get(&(id.into(), version.into()))
            .cloned()
            .context("Unknown registered capability version")?;
        let descriptor = &module.descriptor;
        if registry
            .active
            .get(id)
            .is_some_and(|m| m.descriptor.capability != descriptor.capability)
            || registry
                .active
                .values()
                .any(|m| m.descriptor.id != id && m.descriptor.capability == descriptor.capability)
        {
            bail!("Conflicting active capability module")
        }
        registry.active.insert(id.into(), module);
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
        Ok(self
            .registry
            .read()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?
            .active
            .values()
            .find(|m| m.descriptor.capability == capability)
            .cloned()
            .map(|module| Handle { module }))
    }
    pub fn shutdown(&self) -> Result<()> {
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?;
        self.stop.send_replace(true);
        registry.active.clear();
        registry.versions.clear();
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
            assert_eq!(outcome.resources.tool_calls, 2);
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
}
