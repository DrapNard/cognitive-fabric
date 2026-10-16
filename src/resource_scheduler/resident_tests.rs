use super::*;
use crate::backend::Generation;
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Semaphore;
fn allocation(slots: u32, host: u64, device: u64) -> Allocation {
    Allocation {
        compute_slots: slots,
        host_memory_bytes: host,
        device_memory_bytes: device,
    }
}
fn settings() -> Settings {
    Settings {
        label: "gpu.resident".into(),
        capacity: allocation(2, 100, 100),
        max_pending: 4,
        max_wait_ms: 1000,
        max_execution_ms: 1000,
    }
}
fn resident() -> ResidentAllocation {
    ResidentAllocation {
        id: "backbone.small".into(),
        version: "1.0.0".into(),
        weights_sha256: "a".repeat(64),
        host_memory_bytes: 60,
        device_memory_bytes: 65,
        consumers: vec![
            ResidentConsumer {
                api_base: "http://localhost:1234/v1/".into(),
                model: "shared-solver".into(),
            },
            ResidentConsumer {
                api_base: "http://127.0.0.1:1234/v1".into(),
                model: "shared-critic".into(),
            },
        ],
    }
}
fn task(pool: &ResourcePool) -> String {
    pool.store
        .create_task("Resident accounting fixture", "fast")
        .unwrap()
}
struct Gate {
    entered: AtomicUsize,
    changed: Notify,
    finish: Semaphore,
}
impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: AtomicUsize::new(0),
            changed: Notify::new(),
            finish: Semaphore::new(0),
        })
    }
}
impl InferenceBackend for Gate {
    fn inference_target(&self, _: &str) -> Result<Option<crate::backend::InferenceTarget>> {
        Ok(Some(crate::backend::InferenceTarget::LocalHttp(
            ResidentConsumer {
                api_base: "http://localhost:1234/v1/".into(),
                model: "shared-solver".into(),
            },
        )))
    }
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            self.entered.fetch_add(1, Ordering::SeqCst);
            self.changed.notify_one();
            self.finish.acquire().await.unwrap().forget();
            Ok(Generation {
                text: "fixture answer".into(),
                model: "fixture/v1".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: None,
            })
        })
    }
}
async fn wait(pool: &ResourcePool, n: usize) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if pool.snapshot().unwrap().reservations.len() == n {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn call(
    backend: Arc<dyn InferenceBackend>,
    task: String,
) -> tokio::task::JoinHandle<Result<Generation>> {
    tokio::spawn(async move { backend.generate_for(&task, "solver", "fixture", 16).await })
}
#[test]
fn rejects_invalid_or_oversized_resident_declarations_before_installation() {
    let cfg = Configuration {
        settings: settings(),
        allocation: allocation(1, 20, 20),
        residents: vec![resident()],
    };
    assert!(cfg.validate().is_ok());
    for mutation in 0..17 {
        let mut cfg = cfg.clone();
        let r = &mut cfg.residents[0];
        match mutation {
            0 => r.id = "invalid/id".into(),
            1 => r.version = "01.0.0".into(),
            2 => r.weights_sha256 = "A".repeat(64),
            3 => r.weights_sha256 = "x".repeat(63),
            4 => r.host_memory_bytes = 101,
            5 => r.device_memory_bytes = 101,
            6 => {
                r.host_memory_bytes = 0;
                r.device_memory_bytes = 0;
            }
            7 => r.consumers.clear(),
            8 => r.consumers[0].api_base = "http://example.com/v1".into(),
            9 => r.consumers[0].model = "x\n".into(),
            10 => r.consumers[0].model = "x".repeat(257),
            11 => {
                let mut other = r.consumers[0].clone();
                other.api_base = "http://127.0.0.1:1234/v1".into();
                r.consumers.push(other);
            }
            12 => {
                cfg.residents.push(resident());
            }
            13 => {
                let mut other = resident();
                other.id = "backbone.other".into();
                cfg.residents.push(other);
            }
            14 => cfg.allocation.device_memory_bytes = 36,
            15 => r.consumers = vec![r.consumers[0].clone(); 33],
            _ => cfg.residents = vec![resident(); 129],
        }
        assert!(cfg.validate().is_err(), "mutation {mutation}");
    }
    let mut value = serde_json::to_value(resident()).unwrap();
    value["permissions_granted"] = json!(true);
    assert!(serde_json::from_value::<ResidentAllocation>(value).is_err());
    let store = Arc::new(Store::in_memory().unwrap());
    assert!(ResourcePool::with_residents("jev-plus", store, settings(), vec![resident()]).is_err());
}
#[tokio::test]
async fn shared_backbone_is_counted_once_and_reduces_actual_working_admission() {
    let pool = ResourcePool::with_residents(
        "human-cli",
        Arc::new(Store::in_memory().unwrap()),
        settings(),
        vec![resident()],
    )
    .unwrap();
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(0, 60, 65));
    let gate = Gate::new();
    let a = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 20, 20))
        .unwrap();
    let b = a.pin().unwrap().unwrap();
    assert_eq!(
        pool.snapshot().unwrap().resident_allocated,
        allocation(0, 60, 65)
    );
    let first = call(a, task(&pool));
    tokio::time::timeout(Duration::from_secs(3), gate.changed.notified())
        .await
        .unwrap();
    let second = call(b, task(&pool));
    wait(&pool, 2).await;
    assert_eq!(gate.entered.load(Ordering::SeqCst), 1);
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(1, 80, 85));
    gate.finish.add_permits(1);
    first.await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(3), gate.changed.notified())
        .await
        .unwrap();
    gate.finish.add_permits(1);
    second.await.unwrap().unwrap();
    let s = pool.snapshot().unwrap();
    assert_eq!(s.allocated, allocation(0, 60, 65));
    assert_eq!(s.working_allocated, Allocation::default());
    assert_eq!(s.residents.len(), 1);
    assert_eq!(s.residents[0].consumers.len(), 2);
    assert!(!s.residency_attested);
    assert!(pool
        .wrap_backend("human-cli", gate, allocation(1, 0, 36))
        .is_err());
    pool.shutdown("human-cli").unwrap();
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(0, 60, 65));
}
#[tokio::test]
async fn cancelled_request_release_never_releases_resident_capacity_and_audits_its_identity() {
    let pool = ResourcePool::with_residents(
        "human-cli",
        Arc::new(Store::in_memory().unwrap()),
        settings(),
        vec![resident()],
    )
    .unwrap();
    let gate = Gate::new();
    let t = task(&pool);
    let call = call(
        pool.wrap_backend("human-cli", gate.clone(), allocation(1, 20, 20))
            .unwrap(),
        t.clone(),
    );
    tokio::time::timeout(Duration::from_secs(3), gate.changed.notified())
        .await
        .unwrap();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    let s = pool.snapshot().unwrap();
    assert_eq!(s.allocated, allocation(1, 80, 85));
    assert_eq!(s.working_allocated, allocation(1, 20, 20));
    pool.acknowledge_stopped("human-cli", &s.reservations[0].reservation_id, true)
        .unwrap();
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(0, 60, 65));
    let trace = pool.store.task(&t).unwrap();
    let event = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "resource_intent")
        .unwrap();
    assert_eq!(
        event["payload"]["resident_catalog_sha256"],
        s.resident_catalog_sha256
    );
    assert_eq!(
        event["payload"]["resident_allocated"],
        serde_json::to_value(allocation(0, 60, 65)).unwrap()
    );
    assert_eq!(event["payload"]["residency_attested"], false);
}
#[test]
fn catalog_and_consumer_permutations_keep_live_identity_but_changed_contents_do_not() {
    let store = Arc::new(Store::in_memory().unwrap());
    let mut r = resident();
    r.host_memory_bytes = 10;
    r.device_memory_bytes = 10;
    let mut other = r.clone();
    other.id = "backbone.other".into();
    let cfg = Configuration {
        settings: settings(),
        allocation: allocation(1, 20, 20),
        residents: vec![r, other],
    };
    let pool = ResourcePool::configured(store.clone(), &cfg).unwrap();
    let mut reordered = cfg.clone();
    reordered.residents.reverse();
    for r in &mut reordered.residents {
        r.consumers.reverse();
    }
    let same = ResourcePool::configured(store.clone(), &reordered).unwrap();
    assert!(Arc::ptr_eq(&same, &pool));
    assert_eq!(
        same.snapshot().unwrap().resident_catalog_sha256,
        pool.snapshot().unwrap().resident_catalog_sha256
    );
    reordered.residents[0].device_memory_bytes += 1;
    assert!(ResourcePool::configured(store.clone(), &reordered).is_err());
    reordered = cfg.clone();
    reordered.residents[0].weights_sha256 = "b".repeat(64);
    assert!(ResourcePool::configured(store, &reordered).is_err());
    let legacy = Configuration {
        settings: settings(),
        allocation: allocation(1, 20, 20),
        residents: vec![],
    };
    assert!(serde_json::to_value(legacy)
        .unwrap()
        .get("residents")
        .is_none());
}

#[tokio::test]
async fn unknown_unlisted_and_invalid_targets_deny_before_inference_without_reservations() {
    use crate::backend::{InferenceTarget, ModelTarget};
    struct Classified {
        target: Option<InferenceTarget>,
        queries: AtomicUsize,
        store: Arc<Store>,
        pool: std::sync::Weak<ResourcePool>,
    }
    impl InferenceBackend for Classified {
        fn inference_target(&self, _: &str) -> Result<Option<InferenceTarget>> {
            self.queries.fetch_add(1, Ordering::SeqCst);
            self.store.search("fixture", None, 1)?;
            self.pool.upgrade().unwrap().snapshot()?;
            Ok(self.target.clone())
        }
        fn generate<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async { panic!("Uncovered target reached inference") })
        }
    }
    for target in [
        None,
        Some(InferenceTarget::LocalHttp(ModelTarget {
            api_base: "http://localhost:1234/v1".into(),
            model: "unlisted".into(),
        })),
        Some(InferenceTarget::LocalHttp(ModelTarget {
            api_base: "http://user:secret@example.com/v1".into(),
            model: "shared-solver".into(),
        })),
    ] {
        let pool = ResourcePool::with_residents(
            "human-cli",
            Arc::new(Store::in_memory().unwrap()),
            settings(),
            vec![resident()],
        )
        .unwrap();
        let t = task(&pool);
        let backend = Arc::new(Classified {
            target,
            queries: AtomicUsize::new(0),
            store: pool.store.clone(),
            pool: Arc::downgrade(&pool),
        });
        let wrapped = pool
            .wrap_backend("human-cli", backend.clone(), allocation(1, 20, 20))
            .unwrap();
        assert!(wrapped
            .generate_for(&t, "solver", "fixture", 16)
            .await
            .is_err());
        assert_eq!(backend.queries.load(Ordering::SeqCst), 1);
        let snapshot = pool.snapshot().unwrap();
        assert!(snapshot.reservations.is_empty());
        assert_eq!(snapshot.allocated, allocation(0, 60, 65));
        let trace = pool.store.task(&t).unwrap();
        let events = trace["events"].as_array().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e["kind"] == "resource_target")
                .count(),
            1
        );
        assert!(events
            .iter()
            .any(|e| e["kind"] == "resource_target" && e["payload"]["allowed"] == false));
        assert!(!events
            .iter()
            .any(|e| e["kind"] == "resource_dispatch" || e["kind"] == "resource_intent"));
        assert!(!trace.to_string().contains("secret"));
    }
}
#[tokio::test]
async fn target_audit_failure_blocks_queries_or_execution_and_empty_catalog_preserves_legacy_callbacks(
) {
    use crate::backend::InferenceTarget;
    struct Classified(AtomicUsize);
    impl InferenceBackend for Classified {
        fn inference_target(&self, _: &str) -> Result<Option<InferenceTarget>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Some(InferenceTarget::Deterministic))
        }
        fn generate<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async {
                Ok(Generation {
                    text: "fixture".into(),
                    model: "fixture/v1".into(),
                    input_tokens: None,
                    output_tokens: None,
                    elapsed_ms: 0.0,
                    queue_wait_ms: None,
                })
            })
        }
    }
    for (kind, expected) in [("resource_target_intent", 0), ("resource_target", 1)] {
        let pool = ResourcePool::with_residents(
            "human-cli",
            Arc::new(Store::in_memory().unwrap()),
            settings(),
            vec![resident()],
        )
        .unwrap();
        let t = task(&pool);
        pool.store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER refuse_target BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let backend = Arc::new(Classified(AtomicUsize::new(0)));
        let wrapped = pool
            .wrap_backend("human-cli", backend.clone(), allocation(1, 20, 20))
            .unwrap();
        assert!(wrapped
            .generate_for(&t, "solver", "fixture", 16)
            .await
            .is_err());
        assert_eq!(backend.0.load(Ordering::SeqCst), expected);
        assert!(pool.snapshot().unwrap().reservations.is_empty());
        assert!(!pool.store.task(&t).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "resource_dispatch"));
    }
    let pool = ResourcePool::new(
        "human-cli",
        Arc::new(Store::in_memory().unwrap()),
        settings(),
    )
    .unwrap();
    let backend = Arc::new(Classified(AtomicUsize::new(0)));
    pool.wrap_backend("human-cli", backend.clone(), allocation(1, 20, 20))
        .unwrap()
        .generate_for(&task(&pool), "solver", "fixture", 16)
        .await
        .unwrap();
    assert_eq!(backend.0.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn replacing_routes_during_target_observation_cannot_substitute_the_checked_backend() {
    use crate::backend::{InferenceTarget, ModelTarget};
    use crate::fabric::{Broker, ModuleDescriptor};
    struct Replace {
        broker: Mutex<Option<std::sync::Weak<Broker>>>,
        generated: AtomicUsize,
    }
    struct Unlisted(AtomicUsize);
    impl InferenceBackend for Unlisted {
        fn inference_target(&self, _: &str) -> Result<Option<InferenceTarget>> {
            Ok(Some(InferenceTarget::LocalHttp(ModelTarget {
                api_base: "http://127.0.0.1:1234/v1".into(),
                model: "unlisted".into(),
            })))
        }
        fn generate<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { panic!("Replaced unlisted backend ran") })
        }
    }
    impl InferenceBackend for Replace {
        fn inference_target(&self, _: &str) -> Result<Option<InferenceTarget>> {
            let broker = self
                .broker
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .upgrade()
                .unwrap();
            broker.register(
                ModuleDescriptor {
                    id: "inference.local".into(),
                    version: "2.0.0".into(),
                    roles: crate::fabric::protocol::ROLES
                        .iter()
                        .map(|r| r.to_string())
                        .collect(),
                },
                Arc::new(Unlisted(AtomicUsize::new(0))),
            )?;
            Ok(Some(InferenceTarget::LocalHttp(ModelTarget {
                api_base: "http://localhost:1234/v1".into(),
                model: "shared-solver".into(),
            })))
        }
        fn generate<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            self.generated.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(Generation {
                    text: "original backend".into(),
                    model: "fixture/v1".into(),
                    input_tokens: None,
                    output_tokens: None,
                    elapsed_ms: 0.0,
                    queue_wait_ms: None,
                })
            })
        }
    }
    let pool = ResourcePool::with_residents(
        "human-cli",
        Arc::new(Store::in_memory().unwrap()),
        settings(),
        vec![resident()],
    )
    .unwrap();
    let backend = Arc::new(Replace {
        broker: Mutex::new(None),
        generated: AtomicUsize::new(0),
    });
    let broker = Arc::new(Broker::with_inference(backend.clone()));
    *backend.broker.lock().unwrap() = Some(Arc::downgrade(&broker));
    let wrapped = pool
        .wrap_backend("human-cli", broker.clone(), allocation(1, 20, 20))
        .unwrap();
    let result = wrapped
        .generate_for(&task(&pool), "solver", "fixture", 16)
        .await
        .unwrap();
    assert_eq!(result.text, "original backend");
    assert_eq!(backend.generated.load(Ordering::SeqCst), 1);
    assert_eq!(broker.modules().unwrap()[0].version, "2.0.0");
    assert!(wrapped
        .generate_for(&task(&pool), "solver", "fixture", 16)
        .await
        .is_err());
}
