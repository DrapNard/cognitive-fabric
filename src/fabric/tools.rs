//! Replaceable deterministic arithmetic modules; the runtime verifies their output.
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
pub mod unix;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Descriptor {
    pub id: String,
    pub version: String,
    pub capability: String,
}
impl Descriptor {
    fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
            || self.version.len() > 64
            || self.capability != "math.evaluate"
        {
            bail!("Invalid arithmetic module descriptor")
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
struct Module {
    descriptor: Descriptor,
    backend: Arc<dyn ArithmeticBackend>,
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
}
pub struct Broker {
    registry: RwLock<Registry>,
    stop: Arc<watch::Sender<bool>>,
}
impl Default for Broker {
    fn default() -> Self {
        let (stop, _) = watch::channel(false);
        let broker = Self {
            registry: RwLock::new(Registry::default()),
            stop: Arc::new(stop),
        };
        broker
            .register(
                Descriptor {
                    id: "arithmetic.local".into(),
                    version: "1.0.0".into(),
                    capability: "math.evaluate".into(),
                },
                Arc::new(LocalMath),
            )
            .expect("Built-in arithmetic descriptor is valid");
        broker
    }
}
impl Broker {
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
        if registry.active.values().any(|m| {
            m.descriptor.id != descriptor.id && m.descriptor.capability == descriptor.capability
        }) {
            bail!("Conflicting active capability module")
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
                stop: self.stop.clone(),
            }),
        );
        Ok(())
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
        Ok(self
            .registry
            .read()
            .map_err(|_| anyhow::anyhow!("Capability registry poisoned"))?
            .active
            .values()
            .next()
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
        if *self.module.stop.borrow() {
            bail!("Capability broker shut down")
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
            let response = decode(&receive.await.context("Capability worker stopped")??)?;
            if response.identity != identity
                || response.identity.expires <= super::protocol::now_ms()
            {
                bail!("Capability response identity or deadline mismatch")
            }
            if response.failed {
                bail!("Arithmetic module rejected the request")
            }
            response.answer.context("Capability response has no answer")
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
