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
