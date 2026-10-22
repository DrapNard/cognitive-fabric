use super::*;
use crate::backend::Generation;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Semaphore;

struct Active(Arc<AtomicUsize>);
impl Active {
    fn new(active: Arc<AtomicUsize>) -> Self {
        active.fetch_add(1, Ordering::AcqRel);
        Self(active)
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct GateBackend {
    active: Arc<AtomicUsize>,
    started: Arc<Semaphore>,
    release: Arc<Semaphore>,
}
impl InferenceBackend for GateBackend {
    fn generate<'a>(
        &'a self,
        _role: &'a str,
        _prompt: &'a str,
        _tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async { bail!("Expected task-scoped broker dispatch") })
    }

    fn generate_for<'a>(
        &'a self,
        _task: &'a str,
        _role: &'a str,
        _prompt: &'a str,
        _tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let active = self.active.clone();
        let started = self.started.clone();
        let release = self.release.clone();
        Box::pin(async move {
            let _active = Active::new(active);
            started.add_permits(1);
            let _release = release.acquire().await?;
            Ok(Generation {
                text: "done".into(),
                model: "broker-parallel-test".into(),
                input_tokens: None,
                output_tokens: None,
                elapsed_ms: 0.0,
                queue_wait_ms: None,
            })
        })
    }
}

fn gated_backend() -> (Arc<GateBackend>, Arc<AtomicUsize>) {
    let active = Arc::new(AtomicUsize::new(0));
    (
        Arc::new(GateBackend {
            active: active.clone(),
            started: Arc::new(Semaphore::new(0)),
            release: Arc::new(Semaphore::new(0)),
        }),
        active,
    )
}

async fn wait_started(backend: &GateBackend, count: usize) {
    let permits = u32::try_from(count).unwrap();
    let started = backend.started.clone().acquire_many_owned(permits);
    let _permit = tokio::time::timeout(Duration::from_secs(1), started)
        .await
        .expect("backend calls should enter before timeout")
        .expect("started semaphore remains open");
}

fn call(broker: Arc<Broker>, prompt: &'static str) -> tokio::task::JoinHandle<Result<Generation>> {
    tokio::spawn(async move {
        broker
            .generate_for(&uuid::Uuid::new_v4().to_string(), "solver", prompt, 128)
            .await
    })
}

#[tokio::test]
async fn configured_module_cap_bounds_concurrent_requests_and_queues_the_next() {
    let (backend, _) = gated_backend();
    let broker = Arc::new(Broker::with_inference_concurrency(backend.clone(), 2));
    let first = call(broker.clone(), "first");
    let second = call(broker.clone(), "second");
    wait_started(&backend, 2).await;

    let third = call(broker.clone(), "third");
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if broker.statuses().unwrap()[0].queued_requests == Some(1) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("third request should wait in the bounded queue");
    assert_eq!(backend.active.load(Ordering::Acquire), 2);
    assert_eq!(broker.statuses().unwrap()[0].processing_request, Some(true));

    backend.release.add_permits(2);
    wait_started(&backend, 1).await;
    backend.release.add_permits(1);
    for request in [first, second, third] {
        tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .expect("request should complete")
            .unwrap()
            .unwrap();
    }
    assert_eq!(backend.active.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn shutdown_cancels_and_drains_every_active_module_request() {
    let (backend, active) = gated_backend();
    let broker = Arc::new(Broker::with_inference_concurrency(backend.clone(), 2));
    let module = broker.snapshot().unwrap().routes["solver"].clone();
    let first = call(broker.clone(), "first");
    let second = call(broker.clone(), "second");
    wait_started(&backend, 2).await;
    assert_eq!(active.load(Ordering::Acquire), 2);

    broker.shutdown().unwrap();
    for request in [first, second] {
        assert!(tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .expect("shutdown should finish active calls")
            .unwrap()
            .is_err());
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while module.processing.load(Ordering::Acquire) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("active request accounting should clear after abort and drain");
    assert_eq!(active.load(Ordering::Acquire), 0);
}

#[test]
fn module_concurrency_constructor_rejects_unbounded_caps() {
    let (backend, _) = gated_backend();
    assert!(Broker::try_with_inference_concurrency(backend.clone(), 0).is_err());
    assert!(Broker::try_with_inference_concurrency(backend, 5).is_err());
}
