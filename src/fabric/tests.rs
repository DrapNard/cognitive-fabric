//! Broker, protocol and lifecycle regressions.

use super::protocol::MAX_FRAME;
use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Fixed(&'static str, AtomicUsize);
struct Switchable {
    backend: Fixed,
    closed: AtomicBool,
}
impl InferenceBackend for Switchable {
    fn availability(&self) -> Availability {
        if self.closed.load(Ordering::Acquire) {
            Availability::Closed
        } else {
            Availability::Unknown
        }
    }
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.backend.generate(role, prompt, tokens)
    }
}

#[tokio::test]
async fn ranked_candidates_select_stably_and_require_explicit_overlap_approval() {
    for reverse in [false, true] {
        let broker = Broker::with_inference(Arc::new(Fixed("initial", AtomicUsize::new(0))));
        broker.unregister("inference.local").unwrap();
        let descriptor = |id: &str| ModuleDescriptor {
            id: id.into(),
            version: "1.0.0".into(),
            roles: vec!["solver".into()],
        };
        let names = if reverse {
            ["candidate.b", "candidate.a"]
        } else {
            ["candidate.a", "candidate.b"]
        };
        for name in names {
            broker
                .register_candidate(
                    descriptor(name),
                    Arc::new(Fixed(name, AtomicUsize::new(0))),
                    10,
                )
                .unwrap();
        }
        assert_eq!(
            broker.generate("solver", "goal", 128).await.unwrap().text,
            "candidate.a"
        );
        assert!(broker.generate("planner", "goal", 128).await.is_err());
        assert!(broker
            .register(
                descriptor("exclusive"),
                Arc::new(Fixed("never", AtomicUsize::new(0)))
            )
            .is_err());
        assert!(broker
            .register_candidate(
                descriptor("too.high"),
                Arc::new(Fixed("never", AtomicUsize::new(0))),
                1_000_001
            )
            .is_err());
        assert_eq!(broker.versions().unwrap().len(), 3);
        broker.shutdown().unwrap();
    }
}

#[tokio::test]
async fn ranked_replacement_rollback_and_unavailability_preserve_task_pins() {
    let broker = Broker::with_inference(Arc::new(Fixed("initial", AtomicUsize::new(0))));
    broker.unregister("inference.local").unwrap();
    let descriptor = |id: &str, version: &str| ModuleDescriptor {
        id: id.into(),
        version: version.into(),
        roles: vec!["solver".into()],
    };
    let first = Arc::new(Switchable {
        backend: Fixed("first", AtomicUsize::new(0)),
        closed: AtomicBool::new(false),
    });
    let fallback = Arc::new(Fixed("fallback", AtomicUsize::new(0)));
    broker
        .register_candidate(descriptor("candidate.a", "1.0.0"), first.clone(), 10)
        .unwrap();
    broker
        .register_candidate(descriptor("candidate.b", "1.0.0"), fallback.clone(), 20)
        .unwrap();
    let pinned = broker.pin().unwrap().unwrap();
    broker
        .register_candidate(
            descriptor("candidate.a", "2.0.0"),
            Arc::new(Fixed("replacement", AtomicUsize::new(0))),
            30,
        )
        .unwrap();
    assert_eq!(
        broker.generate("solver", "goal", 128).await.unwrap().text,
        "fallback"
    );
    assert_eq!(
        pinned.generate("solver", "goal", 128).await.unwrap().text,
        "first"
    );
    broker.activate("candidate.a", "1.0.0").unwrap();
    assert_eq!(
        broker.generate("solver", "goal", 128).await.unwrap().text,
        "first"
    );
    first.closed.store(true, Ordering::Release);
    assert!(pinned.generate("solver", "goal", 128).await.is_err());
    assert_eq!(fallback.1.load(Ordering::SeqCst), 1);
    assert_eq!(
        broker.generate("solver", "goal", 128).await.unwrap().text,
        "fallback"
    );
    assert!(broker.activate("candidate.a", "1.0.0").is_err());
    assert!(broker
        .statuses()
        .unwrap()
        .iter()
        .any(|status| status.descriptor.id == "candidate.a"
            && status.descriptor.version == "1.0.0"
            && status.routing_rank == Some(10)
            && status.availability == Availability::Closed));
    broker.shutdown().unwrap();
}
struct Bound {
    descriptor: ModuleDescriptor,
    backend: Fixed,
}
impl InferenceBackend for Bound {
    fn module_descriptor(&self) -> Option<&ModuleDescriptor> {
        Some(&self.descriptor)
    }
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.backend.generate(role, prompt, tokens)
    }
}

#[tokio::test]
async fn transport_registration_preserves_identity_and_refuses_relabelling_before_dispatch() {
    let bound = Arc::new(Bound {
        descriptor: ModuleDescriptor {
            id: "inference.remote".into(),
            version: "3.0.0".into(),
            roles: vec!["solver".into()],
        },
        backend: Fixed("bound", AtomicUsize::new(0)),
    });
    let broker = Broker::try_with_inference(bound.clone()).unwrap();
    assert_eq!(broker.discover().unwrap(), vec![bound.descriptor.clone()]);
    assert!(broker.generate("planner", "goal", 128).await.is_err());
    for field in 0..3 {
        let mut forged = bound.descriptor.clone();
        match field {
            0 => forged.id = "renamed".into(),
            1 => forged.version = "4.0.0".into(),
            _ => forged.roles.push("planner".into()),
        }
        assert!(broker.register(forged, bound.clone()).is_err());
        let mut forged_candidate = bound.descriptor.clone();
        forged_candidate.id = "forged.candidate".into();
        assert!(broker
            .register_candidate(forged_candidate, bound.clone(), 0)
            .is_err());
    }
    assert_eq!(bound.backend.1.load(Ordering::SeqCst), 0);
    assert_eq!(broker.versions().unwrap().len(), 1);
    assert_eq!(
        broker.generate("solver", "goal", 128).await.unwrap().text,
        "bound"
    );
    let invalid = Arc::new(Bound {
        descriptor: ModuleDescriptor {
            id: "invalid".into(),
            version: "1.0.0".into(),
            roles: vec!["shell".into()],
        },
        backend: Fixed("never", AtomicUsize::new(0)),
    });
    assert!(Broker::try_with_inference(invalid).is_err());
    broker.shutdown().unwrap();
}
impl InferenceBackend for Fixed {
    fn generate<'a>(
        &'a self,
        _role: &'a str,
        _prompt: &'a str,
        _max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.1.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(Generation {
                text: self.0.into(),
                model: self.0.into(),
                input_tokens: Some(2),
                output_tokens: Some(1),
                elapsed_ms: 1.0,
                queue_wait_ms: None,
                provider_metrics: None,
            })
        })
    }
}

fn header() -> Header {
    Header {
        correlation: uuid::Uuid::new_v4().to_string(),
        task: uuid::Uuid::new_v4().to_string(),
        sender: "jev-r".into(),
        destination: "inference.local".into(),
        module_version: "1.0.0".into(),
        expires: now_ms() + 10_000,
    }
}
fn request(header: Header) -> Packet {
    Packet {
        header,
        payload: Payload::Request {
            role: "solver".into(),
            prompt: "Solve the original goal".into(),
            tokens: 128,
        },
    }
}

struct TaskEcho;
impl InferenceBackend for TaskEcho {
    fn generate<'a>(
        &'a self,
        _role: &'a str,
        _prompt: &'a str,
        _tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async { bail!("Task identity was lost") })
    }
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        _role: &'a str,
        _prompt: &'a str,
        _tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            Ok(Generation {
                text: task.into(),
                model: "task-identity-fixture".into(),
                input_tokens: None,
                output_tokens: None,
                elapsed_ms: 0.0,
                queue_wait_ms: None,
                provider_metrics: None,
            })
        })
    }
}

#[tokio::test]
async fn direct_and_pinned_dispatch_preserve_task_identity_and_reject_invalid_ids() {
    let broker = Broker::with_inference(Arc::new(TaskEcho));
    let pinned = broker.pin().unwrap().unwrap();
    let first = uuid::Uuid::new_v4().to_string();
    let second = uuid::Uuid::new_v4().to_string();
    for backend in [&broker as &dyn InferenceBackend, pinned.as_ref()] {
        let (a, b) = tokio::join!(
            backend.generate_for(&first, "solver", "first goal", 128),
            backend.generate_for(&second, "solver", "second goal", 128)
        );
        assert_eq!(a.unwrap().text, first);
        assert_eq!(b.unwrap().text, second);
        assert!(backend
            .generate_for("forged-task", "solver", "goal", 128)
            .await
            .is_err());
    }
}

#[test]
fn queue_metrics_preserve_absence_and_reject_invalid_values() {
    for wait in [
        None,
        Some(0.0),
        Some(12.5),
        Some(300_001.0),
        Some(-1.0),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ] {
        let packet = Packet {
            header: header(),
            payload: Payload::Response(Generation {
                text: "answer".into(),
                model: "fixture".into(),
                input_tokens: None,
                output_tokens: None,
                elapsed_ms: 1.0,
                queue_wait_ms: wait,
                provider_metrics: None,
            }),
        };
        let decoded = decode(&encode(&packet).unwrap());
        if wait.is_some_and(|v| !v.is_finite() || !(0.0..=300_000.0).contains(&v)) {
            assert!(decoded.is_err());
        } else {
            let Payload::Response(report) = decoded.unwrap().payload else {
                panic!("Missing response")
            };
            assert_eq!(report.queue_wait_ms, wait);
        }
    }
}

struct Gated {
    calls: AtomicUsize,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
impl InferenceBackend for Gated {
    fn generate<'a>(
        &'a self,
        _role: &'a str,
        _prompt: &'a str,
        _tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.entered.notify_one();
                self.release.notified().await;
            }
            Ok(Generation {
                text: "answer".into(),
                model: "fixture".into(),
                input_tokens: None,
                output_tokens: None,
                elapsed_ms: 1.0,
                queue_wait_ms: Some(999_999.0),
                provider_metrics: None,
            })
        })
    }
}

#[tokio::test]
async fn broker_measures_actual_queue_wait_and_overrides_backend_claims() {
    let backend = Arc::new(Gated {
        calls: AtomicUsize::new(0),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let broker = Arc::new(Broker::with_inference(backend.clone()));
    let first_broker = broker.clone();
    let first =
        tokio::spawn(async move { first_broker.generate("solver", "first", 128).await.unwrap() });
    backend.entered.notified().await;
    let second_broker = broker.clone();
    let second = tokio::spawn(async move {
        second_broker
            .generate("solver", "second", 128)
            .await
            .unwrap()
    });
    let pinned = broker.snapshot().unwrap();
    let queue = pinned.routes["solver"].queue().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while queue.capacity() != 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(25)).await;
    backend.release.notify_one();
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    assert!(first.queue_wait_ms.unwrap() < 999_999.0);
    assert!(second.queue_wait_ms.unwrap() >= 20.0);
    assert_eq!(second.elapsed_ms, 1.0);
}

#[tokio::test]
async fn module_replacement_and_unregistration_preserve_pinned_tasks() {
    let broker = Broker::with_inference(Arc::new(Fixed("old", AtomicUsize::new(0))));
    let pinned = broker.pin().unwrap().unwrap();
    let descriptor = ModuleDescriptor {
        id: "inference.local".into(),
        version: "2.0.0".into(),
        roles: vec!["solver".into()],
    };
    broker
        .register(
            descriptor.clone(),
            Arc::new(Fixed("new", AtomicUsize::new(0))),
        )
        .unwrap();
    assert!(broker
        .register(
            descriptor,
            Arc::new(Fixed("duplicate", AtomicUsize::new(0)))
        )
        .is_err());
    assert_eq!(broker.discover().unwrap()[0].version, "2.0.0");
    assert_eq!(
        pinned
            .generate_for(&uuid::Uuid::new_v4().to_string(), "solver", "original", 128)
            .await
            .unwrap()
            .text,
        "old"
    );
    assert_eq!(
        broker
            .generate("solver", "original", 128)
            .await
            .unwrap()
            .text,
        "new"
    );
    let conflict = ModuleDescriptor {
        id: "other".into(),
        version: "1.0.0".into(),
        roles: vec!["solver".into()],
    };
    assert!(broker
        .register(conflict, Arc::new(Fixed("conflict", AtomicUsize::new(0))))
        .is_err());
    broker.unregister("inference.local").unwrap();
    assert!(broker.generate("solver", "original", 128).await.is_err());
    assert_eq!(
        pinned
            .generate("solver", "original", 128)
            .await
            .unwrap()
            .text,
        "old"
    );
}

#[test]
fn malformed_module_frames_and_invalid_bounds_fail_closed() {
    let original = request(header());
    let bytes = encode(&original).unwrap();
    for length in 0..bytes.len() {
        // A shortened buffer may omit unused alignment padding, which is legal.
        if let Ok(decoded) = decode(&bytes[..length]) {
            assert!(same_header(&decoded.header, &original.header));
            assert!(
                matches!(decoded.payload, Payload::Request { prompt, tokens:128, .. } if prompt == "Solve the original goal")
            );
        }
    }
    let mut corrupted = bytes.clone();
    corrupted[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(&corrupted).is_err());
    assert!(decode(&vec![0; MAX_FRAME + 1]).is_err());
    let mut invalid = header();
    invalid.task = "forged-task".into();
    assert!(decode(&encode(&request(invalid)).unwrap()).is_err());
    let packet = Packet {
        header: header(),
        payload: Payload::Request {
            role: "solver".into(),
            prompt: "oversized".repeat(4000),
            tokens: 128,
        },
    };
    assert!(decode(&encode(&packet).unwrap()).is_err());
    let packet = Packet {
        header: header(),
        payload: Payload::Request {
            role: "solver".into(),
            prompt: "goal".into(),
            tokens: 0,
        },
    };
    assert!(decode(&encode(&packet).unwrap()).is_err());
    let descriptor = ModuleDescriptor {
        id: "expert".into(),
        version: "1.0.0".into(),
        roles: vec!["shell.execute".into()],
    };
    assert!(descriptor.validate().is_err());
}

#[tokio::test]
async fn rollback_reuses_original_backend_and_preserves_pinned_role_routes() {
    let original = Arc::new(Fixed("original", AtomicUsize::new(0)));
    let replacement = Arc::new(Fixed("replacement", AtomicUsize::new(0)));
    let broker = Broker::with_inference(original.clone());
    broker
        .register(
            ModuleDescriptor {
                id: "inference.local".into(),
                version: "2.0.0".into(),
                roles: vec!["solver".into()],
            },
            replacement.clone(),
        )
        .unwrap();
    let pinned_replacement = broker.pin().unwrap().unwrap();
    broker.activate("inference.local", "1.0.0").unwrap();
    assert_eq!(
        broker.generate("planner", "goal", 128).await.unwrap().text,
        "original"
    );
    assert_eq!(
        pinned_replacement
            .generate("solver", "goal", 128)
            .await
            .unwrap()
            .text,
        "replacement"
    );
    assert!(pinned_replacement
        .generate("planner", "goal", 128)
        .await
        .is_err());
    assert_eq!(original.1.load(Ordering::SeqCst), 1);
    assert_eq!(replacement.1.load(Ordering::SeqCst), 1);
    assert_eq!(broker.versions().unwrap().len(), 2);
    broker.unregister("inference.local").unwrap();
    broker.activate("inference.local", "2.0.0").unwrap();
    assert!(broker.generate("planner", "goal", 128).await.is_err());
    assert!(broker.activate("inference.local", "9.0.0").is_err());
    assert!(broker.activate(&"x".repeat(65), "1.0.0").is_err());
    broker.shutdown().unwrap();
    assert!(broker.versions().unwrap().is_empty());
    assert!(broker.activate("inference.local", "1.0.0").is_err());
    assert!(pinned_replacement
        .generate("solver", "goal", 128)
        .await
        .is_err());
}

#[test]
fn rollback_refuses_role_conflicts_without_changing_active_routes() {
    let broker = Broker::with_inference(Arc::new(Fixed("original", AtomicUsize::new(0))));
    broker
        .register(
            ModuleDescriptor {
                id: "inference.local".into(),
                version: "2.0.0".into(),
                roles: vec!["solver".into()],
            },
            Arc::new(Fixed("replacement", AtomicUsize::new(0))),
        )
        .unwrap();
    broker
        .register(
            ModuleDescriptor {
                id: "planner.other".into(),
                version: "1.0.0".into(),
                roles: vec!["planner".into()],
            },
            Arc::new(Fixed("planner", AtomicUsize::new(0))),
        )
        .unwrap();
    assert!(broker.activate("inference.local", "1.0.0").is_err());
    let routes = broker.snapshot().unwrap();
    assert_eq!(routes.routes["solver"].descriptor.version, "2.0.0");
    assert_eq!(routes.routes["planner"].descriptor.id, "planner.other");
    broker.unregister("planner.other").unwrap();
    broker.activate("inference.local", "1.0.0").unwrap();
    assert_eq!(
        broker.snapshot().unwrap().routes["planner"].descriptor.id,
        "inference.local"
    );
}

#[tokio::test]
async fn rollback_at_history_capacity_reuses_the_original_bounded_queue() {
    let broker = Broker::with_inference(Arc::new(Fixed("original", AtomicUsize::new(0))));
    let original = broker.snapshot().unwrap().routes["solver"].clone();
    let queue = original.queue().await.clone();
    for version in 2..=128 {
        broker
            .register(
                ModuleDescriptor {
                    id: "inference.local".into(),
                    version: format!("{version}.0.0"),
                    roles: vec!["solver".into()],
                },
                Arc::new(Fixed("replacement", AtomicUsize::new(0))),
            )
            .unwrap();
    }
    assert_eq!(broker.versions().unwrap().len(), 128);
    assert!(broker
        .register(
            ModuleDescriptor {
                id: "inference.local".into(),
                version: "129.0.0".into(),
                roles: vec!["solver".into()],
            },
            Arc::new(Fixed("overflow", AtomicUsize::new(0)))
        )
        .is_err());
    broker.activate("inference.local", "1.0.0").unwrap();
    let restored = broker.snapshot().unwrap().routes["solver"].clone();
    assert!(Arc::ptr_eq(&original, &restored));
    assert!(queue.same_channel(restored.queue().await));
    assert_eq!(
        broker.generate("solver", "goal", 128).await.unwrap().text,
        "original"
    );
    broker.shutdown().unwrap();
}

#[tokio::test]
async fn forged_identity_and_expired_work_never_reach_the_backend() {
    let backend = Arc::new(Fixed("never", AtomicUsize::new(0)));
    let broker = Broker::with_inference(backend.clone());
    let pinned = broker.snapshot().unwrap();
    let module = &pinned.routes["solver"];
    for mismatch in 0..4 {
        let mut identity = header();
        let mut packet = request(identity.clone());
        match mismatch {
            0 => packet.header.task = uuid::Uuid::new_v4().to_string(),
            1 => packet.header.sender = "human-cli".into(),
            2 => {
                identity.expires = 0;
                packet.header.expires = 0;
            }
            _ => {
                identity.sender = "human-cli".into();
                packet.header.sender = "human-cli".into();
            }
        }
        let (reply, response) = oneshot::channel();
        module
            .queue()
            .await
            .try_send(Work {
                enqueued: std::time::Instant::now(),
                bytes: encode(&packet).unwrap(),
                identity,
                backend: None,
                reply,
            })
            .unwrap();
        assert!(response.await.unwrap().is_err());
    }
    assert_eq!(backend.1.load(Ordering::SeqCst), 0);
}

struct Hanging {
    entered: tokio::sync::Notify,
    cancelled: Arc<AtomicBool>,
}
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
impl InferenceBackend for Hanging {
    fn generate<'a>(
        &'a self,
        _role: &'a str,
        _prompt: &'a str,
        _tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            let _guard = DropFlag(self.cancelled.clone());
            self.entered.notify_one();
            std::future::pending().await
        })
    }
}

#[tokio::test]
async fn bounded_queue_rejects_overflow_and_drops_cancelled_inference() {
    let backend = Arc::new(Hanging {
        entered: tokio::sync::Notify::new(),
        cancelled: Arc::new(AtomicBool::new(false)),
    });
    let broker = Broker::with_inference(backend.clone());
    let pinned = broker.snapshot().unwrap();
    let queue = pinned.routes["solver"].queue().await;
    let mut receivers = Vec::new();
    let identity = header();
    let (reply, response) = oneshot::channel();
    queue
        .try_send(Work {
            enqueued: std::time::Instant::now(),
            bytes: encode(&request(identity.clone())).unwrap(),
            identity,
            backend: None,
            reply,
        })
        .unwrap();
    receivers.push(response);
    backend.entered.notified().await;
    for _ in 0..4 {
        let identity = header();
        let (reply, response) = oneshot::channel();
        queue
            .try_send(Work {
                enqueued: std::time::Instant::now(),
                bytes: encode(&request(identity.clone())).unwrap(),
                identity,
                backend: None,
                reply,
            })
            .unwrap();
        receivers.push(response);
    }
    let identity = header();
    let (reply, _response) = oneshot::channel();
    assert!(queue
        .try_send(Work {
            enqueued: std::time::Instant::now(),
            bytes: encode(&request(identity.clone())).unwrap(),
            identity,
            backend: None,
            reply
        })
        .is_err());
    drop(receivers);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !backend.cancelled.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn shutdown_cancels_retired_pinned_versions_and_queued_work() {
    let backend = Arc::new(Hanging {
        entered: tokio::sync::Notify::new(),
        cancelled: Arc::new(AtomicBool::new(false)),
    });
    let broker = Broker::with_inference(backend.clone());
    let pinned = broker.pin().unwrap().unwrap();
    let running_backend = pinned.clone();
    let running =
        tokio::spawn(async move { running_backend.generate("solver", "running", 128).await });
    backend.entered.notified().await;
    let queued_backend = pinned.clone();
    let queued =
        tokio::spawn(async move { queued_backend.generate("solver", "queued", 128).await });
    let snapshot = broker.snapshot().unwrap();
    let queue = snapshot.routes["solver"].queue().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while queue.capacity() != 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    broker
        .register(
            ModuleDescriptor {
                id: "inference.local".into(),
                version: "2.0.0".into(),
                roles: vec!["solver".into()],
            },
            Arc::new(Fixed("replacement", AtomicUsize::new(0))),
        )
        .unwrap();
    broker.unregister("inference.local").unwrap();
    broker.shutdown().unwrap();
    broker.shutdown().unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        assert!(running.await.unwrap().is_err());
        assert!(queued.await.unwrap().is_err());
    })
    .await
    .unwrap();
    assert!(backend.cancelled.load(Ordering::SeqCst));
    assert!(pinned
        .generate("solver", "after shutdown", 128)
        .await
        .is_err());
    assert!(broker.pin().is_err());
    assert!(broker.discover().unwrap().is_empty());
    assert!(broker
        .register(
            ModuleDescriptor {
                id: "other".into(),
                version: "1.0.0".into(),
                roles: vec!["solver".into()],
            },
            Arc::new(Fixed("forbidden", AtomicUsize::new(0)))
        )
        .is_err());
}

#[tokio::test]
async fn shutdown_before_queue_initialization_prevents_backend_dispatch() {
    let backend = Arc::new(Fixed("never", AtomicUsize::new(0)));
    let broker = Broker::with_inference(backend.clone());
    let pinned = broker.pin().unwrap().unwrap();
    broker.shutdown().unwrap();
    assert!(pinned.generate("solver", "goal", 128).await.is_err());
    assert!(broker.generate("solver", "goal", 128).await.is_err());
    assert_eq!(backend.1.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn inactive_retirement_preserves_pins_and_releases_backend_after_last_pin() {
    let backend = Arc::new(Fixed("retired", AtomicUsize::new(0)));
    let weak = Arc::downgrade(&backend);
    let broker = Broker::with_inference(backend.clone());
    drop(backend);
    let pinned = broker.pin().unwrap().unwrap();
    assert!(broker.retire("inference.local", "1.0.0").is_err());
    broker.unregister("inference.local").unwrap();
    broker.retire("inference.local", "1.0.0").unwrap();
    assert!(broker.versions().unwrap().is_empty());
    assert!(broker.activate("inference.local", "1.0.0").is_err());
    assert!(broker.retire("inference.local", "1.0.0").is_err());
    assert!(broker
        .register(
            ModuleDescriptor {
                id: "inference.local".into(),
                version: "1.0.0".into(),
                roles: vec!["solver".into()],
            },
            Arc::new(Fixed("replacement", AtomicUsize::new(0))),
        )
        .is_err());
    assert!(broker.generate("solver", "goal", 128).await.is_err());
    assert_eq!(
        pinned.generate("solver", "goal", 128).await.unwrap().text,
        "retired"
    );
    assert!(weak.upgrade().is_some());
    drop(pinned);
    tokio::time::timeout(Duration::from_secs(1), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Retired backend must be released after its queue and last pin close");
}

#[tokio::test]
async fn retirement_preserves_active_and_queued_calls_but_not_shutdown_immunity() {
    for shutdown in [false, true] {
        let backend = Arc::new(Gated {
            calls: AtomicUsize::new(0),
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let weak = Arc::downgrade(&backend);
        let broker = Broker::with_inference(backend.clone());
        let pinned = broker.snapshot().unwrap();
        let active = Arc::new(pinned);
        let first_pin = active.clone();
        let first = tokio::spawn(async move { first_pin.generate("solver", "first", 128).await });
        tokio::time::timeout(Duration::from_secs(1), backend.entered.notified())
            .await
            .unwrap();
        let second_pin = active.clone();
        let second =
            tokio::spawn(async move { second_pin.generate("solver", "second", 128).await });
        let queue = active.routes["solver"].queue().await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while queue.capacity() != 3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        broker
            .register(
                ModuleDescriptor {
                    id: "inference.local".into(),
                    version: "2.0.0".into(),
                    roles: vec!["solver".into()],
                },
                Arc::new(Fixed("replacement", AtomicUsize::new(0))),
            )
            .unwrap();
        broker.retire("inference.local", "1.0.0").unwrap();
        assert_eq!(
            broker.generate("solver", "new", 128).await.unwrap().text,
            "replacement"
        );
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        if shutdown {
            broker.shutdown().unwrap();
        } else {
            backend.release.notify_one();
        }
        for call in [first, second] {
            let result = tokio::time::timeout(Duration::from_secs(1), call)
                .await
                .unwrap()
                .unwrap();
            if shutdown {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap().text, "answer");
            }
        }
        assert_eq!(
            backend.calls.load(Ordering::SeqCst),
            if shutdown { 1 } else { 2 }
        );
        drop(active);
        drop(backend);
        tokio::time::timeout(Duration::from_secs(1), async {
            while weak.upgrade().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Retired running backend must be released");
    }
}

struct RegistryCleanup {
    broker: std::sync::Weak<Broker>,
    completed: Arc<AtomicBool>,
}
impl Drop for RegistryCleanup {
    fn drop(&mut self) {
        let broker = self.broker.upgrade().unwrap();
        assert!(
            broker.registry.try_read().is_ok(),
            "Backend cleanup must run outside the registry lock"
        );
        assert!(!broker
            .versions()
            .unwrap()
            .iter()
            .any(|version| version.id == "cleanup"));
        self.completed.store(true, Ordering::Release);
    }
}
impl InferenceBackend for RegistryCleanup {
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async { bail!("Cleanup fixture does not perform inference") })
    }
}
#[test]
fn retirement_and_shutdown_backend_cleanup_can_inspect_the_committed_registry() {
    for shutdown in [false, true] {
        let broker = Arc::new(Broker::with_inference(Arc::new(Fixed(
            "initial",
            AtomicUsize::new(0),
        ))));
        broker.unregister("inference.local").unwrap();
        let completed = Arc::new(AtomicBool::new(false));
        broker
            .register(
                ModuleDescriptor {
                    id: "cleanup".into(),
                    version: "1.0.0".into(),
                    roles: vec!["solver".into()],
                },
                Arc::new(RegistryCleanup {
                    broker: Arc::downgrade(&broker),
                    completed: completed.clone(),
                }),
            )
            .unwrap();
        if shutdown {
            broker.shutdown().unwrap();
        } else {
            broker.unregister("cleanup").unwrap();
            broker.retire("cleanup", "1.0.0").unwrap();
        }
        assert!(completed.load(Ordering::Acquire));
    }
}

#[tokio::test]
async fn equal_rank_routing_uses_queue_pressure_without_migrating_existing_pins() {
    let backend = Arc::new(Gated {
        calls: AtomicUsize::new(0),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let broker = Broker::with_inference(Arc::new(Fixed("initial", AtomicUsize::new(0))));
    broker.unregister("inference.local").unwrap();
    let descriptor = |id: &str, version: &str| ModuleDescriptor {
        id: id.into(),
        version: version.into(),
        roles: vec!["solver".into()],
    };
    broker
        .register_candidate(descriptor("candidate.a", "1.0.0"), backend.clone(), 10)
        .unwrap();
    broker
        .register_candidate(
            descriptor("candidate.b", "1.0.0"),
            Arc::new(Fixed("fallback", AtomicUsize::new(0))),
            20,
        )
        .unwrap();
    let original = Arc::new(broker.snapshot().unwrap());
    assert_eq!(original.routes["solver"].descriptor.id, "candidate.a");
    let first_pin = original.clone();
    let first = tokio::spawn(async move { first_pin.generate("solver", "first", 128).await });
    tokio::time::timeout(Duration::from_secs(1), backend.entered.notified())
        .await
        .unwrap();
    let second_pin = original.clone();
    let second = tokio::spawn(async move { second_pin.generate("solver", "second", 128).await });
    let queue = original.routes["solver"].queue().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while queue.capacity() != 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // Lower-priority operator ranks never win merely through lower local load.
    assert_eq!(
        broker.snapshot().unwrap().routes["solver"].descriptor.id,
        "candidate.a"
    );
    broker
        .register_candidate(
            descriptor("candidate.b", "2.0.0"),
            Arc::new(Fixed("idle", AtomicUsize::new(0))),
            10,
        )
        .unwrap();
    let less_queued = broker.snapshot().unwrap();
    assert_eq!(less_queued.routes["solver"].descriptor.id, "candidate.b");
    assert_eq!(
        less_queued
            .generate("solver", "third", 128)
            .await
            .unwrap()
            .text,
        "idle"
    );
    backend.release.notify_one();
    for call in [first, second] {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), call)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .text,
            "answer"
        );
    }
    assert_eq!(
        original
            .generate("solver", "pinned", 128)
            .await
            .unwrap()
            .text,
        "answer"
    );
    assert_eq!(
        broker.snapshot().unwrap().routes["solver"].descriptor.id,
        "candidate.a"
    );
}

struct InventoryObserver(std::sync::Weak<Broker>);
impl InferenceBackend for InventoryObserver {
    fn availability(&self) -> Availability {
        let broker = self.0.upgrade().unwrap();
        assert!(
            broker.registry.try_write().is_ok(),
            "Inventory callback must run outside registry locks"
        );
        assert_eq!(broker.versions().unwrap().len(), 1);
        Availability::Connected
    }
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async { bail!("Inventory must not generate") })
    }
}
#[test]
fn inventory_backend_observation_can_inspect_registry_without_lock_recursion() {
    let broker = Arc::new(Broker::with_inference(Arc::new(Fixed(
        "initial",
        AtomicUsize::new(0),
    ))));
    broker.unregister("inference.local").unwrap();
    broker.retire("inference.local", "1.0.0").unwrap();
    broker
        .register(
            ModuleDescriptor {
                id: "observer".into(),
                version: "1.0.0".into(),
                roles: vec!["solver".into()],
            },
            Arc::new(InventoryObserver(Arc::downgrade(&broker))),
        )
        .unwrap();
    let observed = broker.statuses().unwrap();
    assert_eq!(observed.len(), 1);
    assert!(observed[0].active);
    assert_eq!(observed[0].availability, Availability::Connected);
    assert_eq!(
        broker.snapshot().unwrap().routes["solver"].descriptor.id,
        "observer"
    );
    broker.activate("observer", "1.0.0").unwrap();
}

#[tokio::test]
async fn equal_rank_routes_prefer_idle_peer_when_running_queue_is_empty() {
    let backend = Arc::new(Gated {
        calls: AtomicUsize::new(0),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let broker = Broker::with_inference(Arc::new(Fixed("initial", AtomicUsize::new(0))));
    broker.unregister("inference.local").unwrap();
    let descriptor = |id: &str| ModuleDescriptor {
        id: id.into(),
        version: "1.0.0".into(),
        roles: vec!["solver".into()],
    };
    broker
        .register_candidate(descriptor("candidate.a"), backend.clone(), 10)
        .unwrap();
    broker
        .register_candidate(
            descriptor("candidate.b"),
            Arc::new(Fixed("idle", AtomicUsize::new(0))),
            10,
        )
        .unwrap();
    let pinned = Arc::new(broker.snapshot().unwrap());
    assert_eq!(pinned.routes["solver"].descriptor.id, "candidate.a");
    let active = pinned.clone();
    let call = tokio::spawn(async move { active.generate("solver", "running", 128).await });
    tokio::time::timeout(Duration::from_secs(1), backend.entered.notified())
        .await
        .unwrap();
    let inventory = broker.statuses().unwrap();
    let busy = inventory
        .iter()
        .find(|s| s.descriptor.id == "candidate.a")
        .unwrap();
    assert_eq!(busy.queued_requests, Some(0));
    assert_eq!(busy.processing_request, Some(true));
    let next = broker.snapshot().unwrap();
    assert_eq!(next.routes["solver"].descriptor.id, "candidate.b");
    assert_eq!(
        next.generate("solver", "independent", 128)
            .await
            .unwrap()
            .text,
        "idle"
    );
    assert_eq!(pinned.routes["solver"].descriptor.id, "candidate.a");
    backend.release.notify_one();
    assert_eq!(call.await.unwrap().unwrap().text, "answer");
    assert_eq!(
        pinned
            .generate("solver", "still pinned", 128)
            .await
            .unwrap()
            .text,
        "answer"
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        while broker
            .statuses()
            .unwrap()
            .iter()
            .any(|s| s.processing_request == Some(true))
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        broker.snapshot().unwrap().routes["solver"].descriptor.id,
        "candidate.a"
    );
    assert_eq!(backend.calls.load(Ordering::Acquire), 2);
}

#[tokio::test]
async fn processing_observation_clears_after_cancellation_and_shutdown() {
    for shutdown in [false, true] {
        let cancelled = Arc::new(AtomicBool::new(false));
        let backend = Arc::new(Hanging {
            entered: tokio::sync::Notify::new(),
            cancelled: cancelled.clone(),
        });
        let broker = Arc::new(Broker::with_inference(backend.clone()));
        let pinned = broker.snapshot().unwrap();
        let module = pinned.routes["solver"].clone();
        let call = tokio::spawn(async move { pinned.generate("solver", "pending", 128).await });
        tokio::time::timeout(Duration::from_secs(1), backend.entered.notified())
            .await
            .unwrap();
        assert_eq!(broker.statuses().unwrap()[0].processing_request, Some(true));
        if shutdown {
            broker.shutdown().unwrap();
            assert!(call.await.unwrap().is_err());
        } else {
            call.abort();
            assert!(call.await.unwrap_err().is_cancelled());
        }
        tokio::time::timeout(Duration::from_secs(1), async {
            while module.processing.load(Ordering::Acquire) > 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(cancelled.load(Ordering::Acquire));
        assert_eq!(module.queued_requests(), 0);
        if shutdown {
            assert!(broker.statuses().unwrap().is_empty());
        } else {
            let status = broker.statuses().unwrap();
            assert_eq!(status[0].processing_request, Some(false));
            assert_eq!(status[0].queued_requests, Some(0));
        }
    }
}

struct PanicsButAdvertisesConnected(AtomicUsize);
impl InferenceBackend for PanicsButAdvertisesConnected {
    fn availability(&self) -> Availability {
        Availability::Connected
    }
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("Expected trusted callback failure");
        })
    }
}
#[tokio::test]
async fn closed_local_worker_overrides_backend_health_without_replaying_pins() {
    let failing = Arc::new(PanicsButAdvertisesConnected(AtomicUsize::new(0)));
    let healthy = Arc::new(Fixed("healthy", AtomicUsize::new(0)));
    let broker = Broker::with_inference(Arc::new(Fixed("initial", AtomicUsize::new(0))));
    broker.unregister("inference.local").unwrap();
    let descriptor = |id: &str| ModuleDescriptor {
        id: id.into(),
        version: "1.0.0".into(),
        roles: vec!["solver".into()],
    };
    broker
        .register_candidate(descriptor("candidate.a"), failing.clone(), 10)
        .unwrap();
    broker
        .register_candidate(descriptor("candidate.b"), healthy.clone(), 20)
        .unwrap();
    let original = broker.snapshot().unwrap();
    assert!(original.generate("solver", "first", 128).await.is_err());
    let status = broker
        .statuses()
        .unwrap()
        .into_iter()
        .find(|s| s.descriptor.id == "candidate.a")
        .unwrap();
    assert_eq!(status.availability, Availability::Closed);
    assert_eq!(status.processing_request, Some(false));
    assert!(broker.activate("candidate.a", "1.0.0").is_err());
    assert!(original
        .generate("solver", "no implicit retry", 128)
        .await
        .unwrap_err()
        .to_string()
        .contains("permanently closed"));
    assert_eq!(healthy.1.load(Ordering::Acquire), 0);
    let future = broker.snapshot().unwrap();
    assert_eq!(future.routes["solver"].descriptor.id, "candidate.b");
    assert_eq!(
        future
            .generate("solver", "new task", 128)
            .await
            .unwrap()
            .text,
        "healthy"
    );
    assert_eq!(failing.0.load(Ordering::Acquire), 1);
    assert_eq!(healthy.1.load(Ordering::Acquire), 1);
}

struct RetireOnObservation {
    broker: std::sync::Weak<Broker>,
    calls: AtomicUsize,
}
impl InferenceBackend for RetireOnObservation {
    fn availability(&self) -> Availability {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.broker
            .upgrade()
            .unwrap()
            .retire("retiring", "1.0.0")
            .unwrap();
        Availability::Connected
    }
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async { bail!("Retired fixture must never execute") })
    }
}
#[test]
fn activation_cannot_resurrect_a_version_retired_during_health_observation() {
    let broker = Arc::new(Broker::with_inference(Arc::new(Fixed(
        "initial",
        AtomicUsize::new(0),
    ))));
    broker.unregister("inference.local").unwrap();
    broker.retire("inference.local", "1.0.0").unwrap();
    let backend = Arc::new(RetireOnObservation {
        broker: Arc::downgrade(&broker),
        calls: AtomicUsize::new(0),
    });
    let descriptor = ModuleDescriptor {
        id: "retiring".into(),
        version: "1.0.0".into(),
        roles: vec!["solver".into()],
    };
    broker
        .register(descriptor.clone(), backend.clone())
        .unwrap();
    broker.unregister("retiring").unwrap();
    assert!(broker
        .activate("retiring", "1.0.0")
        .unwrap_err()
        .to_string()
        .contains("retired"));
    assert_eq!(backend.calls.load(Ordering::Acquire), 1);
    assert!(broker.versions().unwrap().is_empty());
    assert!(broker.discover().unwrap().is_empty());
    assert!(broker
        .register(
            descriptor,
            Arc::new(Fixed("replacement", AtomicUsize::new(0)))
        )
        .is_err());
}

#[tokio::test]
async fn saturated_inference_routes_skip_only_new_pins_and_never_retry_old_handles() {
    let busy = Arc::new(Gated {
        calls: AtomicUsize::new(0),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let fallback = Arc::new(Fixed("fallback", AtomicUsize::new(0)));
    let broker = Broker::with_inference(Arc::new(Fixed("initial", AtomicUsize::new(0))));
    broker.unregister("inference.local").unwrap();
    let descriptor = |id: &str| ModuleDescriptor {
        id: id.into(),
        version: "1.0.0".into(),
        roles: vec!["solver".into()],
    };
    broker
        .register_candidate(descriptor("preferred"), busy.clone(), 10)
        .unwrap();
    broker
        .register_candidate(descriptor("fallback"), fallback.clone(), 20)
        .unwrap();
    let pinned = Arc::new(broker.snapshot().unwrap());
    assert_eq!(pinned.routes["solver"].descriptor.id, "preferred");
    let first = pinned.clone();
    let mut calls = vec![tokio::spawn(async move {
        first.generate("solver", "running", 128).await
    })];
    tokio::time::timeout(Duration::from_secs(1), busy.entered.notified())
        .await
        .unwrap();
    for _ in 0..4 {
        let waiting = pinned.clone();
        calls.push(tokio::spawn(async move {
            waiting.generate("solver", "buffered", 128).await
        }));
    }
    let queue = pinned.routes["solver"].queue().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while queue.capacity() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(busy.calls.load(Ordering::Acquire), 1);
    assert!(pinned
        .generate("solver", "must not retry", 128)
        .await
        .is_err());
    assert_eq!(fallback.1.load(Ordering::Acquire), 0);
    let next = broker.snapshot().unwrap();
    assert_eq!(next.routes["solver"].descriptor.id, "fallback");
    assert_eq!(
        next.generate("solver", "new task", 128).await.unwrap().text,
        "fallback"
    );
    assert_eq!(fallback.1.load(Ordering::Acquire), 1);
    broker.unregister("fallback").unwrap();
    assert!(!broker.snapshot().unwrap().routes.contains_key("solver"));
    broker.activate("fallback", "1.0.0").unwrap();
    assert_eq!(
        broker.snapshot().unwrap().routes["solver"].descriptor.id,
        "fallback"
    );
    assert_eq!(pinned.routes["solver"].descriptor.id, "preferred");
    busy.release.notify_one();
    for call in calls {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), call)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .text,
            "answer"
        );
    }
    assert_eq!(busy.calls.load(Ordering::Acquire), 5);
    assert_eq!(
        broker.snapshot().unwrap().routes["solver"].descriptor.id,
        "preferred"
    );
    assert_eq!(
        pinned
            .generate("solver", "original pin after drain", 128)
            .await
            .unwrap()
            .text,
        "answer"
    );
    assert_eq!(busy.calls.load(Ordering::Acquire), 6);
}
