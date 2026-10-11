//! Broker, protocol and lifecycle regressions.

use super::protocol::MAX_FRAME;
use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Fixed(&'static str, AtomicUsize);
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
