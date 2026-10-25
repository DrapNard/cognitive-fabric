use super::*;
use crate::{backend::Generation, config::Config, engine::Engine};
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
fn pool(capacity: Allocation) -> Arc<ResourcePool> {
    ResourcePool::new(
        "human-cli",
        Arc::new(Store::in_memory().unwrap()),
        Settings {
            label: "gpu.main".into(),
            capacity,
            max_pending: 4,
            max_wait_ms: 1000,
            max_execution_ms: 1000,
            scheduling: None,
        },
    )
    .unwrap()
}
fn task(pool: &ResourcePool) -> String {
    pool.store.create_task("fixture objective", "fast").unwrap()
}
struct Gate {
    entered: AtomicUsize,
    changed: Notify,
    complete: Semaphore,
    fail: bool,
}
impl Gate {
    fn new(fail: bool) -> Arc<Self> {
        Arc::new(Self {
            entered: AtomicUsize::new(0),
            changed: Notify::new(),
            complete: Semaphore::new(0),
            fail,
        })
    }
    async fn entered(&self, n: usize) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let changed = self.changed.notified();
                if self.entered.load(Ordering::SeqCst) >= n {
                    return;
                }
                changed.await;
            }
        })
        .await
        .expect("Fixture backend entry deadline");
    }
}
impl InferenceBackend for Gate {
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            self.entered.fetch_add(1, Ordering::SeqCst);
            self.changed.notify_one();
            self.complete.acquire().await.unwrap().forget();
            if self.fail {
                bail!("Fixture provider error")
            }
            Ok(Generation {
                text: "fixture answer".into(),
                model: "fixture/v1".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 1.0,
                queue_wait_ms: Some(99999.0),
                provider_metrics: None,
            })
        })
    }
}
fn call(
    backend: Arc<dyn InferenceBackend>,
    task: String,
) -> tokio::task::JoinHandle<Result<Generation>> {
    tokio::spawn(async move { backend.generate_for(&task, "solver", "fixture", 16).await })
}
async fn pending(pool: &ResourcePool, n: usize) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if pool.lock().unwrap().queue.len() == n {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Fixture queue deadline");
}
#[test]
fn configuration_is_bounded_and_never_model_authority() {
    let store = Arc::new(Store::in_memory().unwrap());
    let settings = Settings {
        label: "gpu.main".into(),
        capacity: allocation(2, 100, 100),
        max_pending: 4,
        max_wait_ms: 1000,
        max_execution_ms: 1000,
        scheduling: None,
    };
    for actor in ["jev-plus", "jev-r", "interface", "unknown"] {
        assert!(ResourcePool::new(actor, store.clone(), settings.clone()).is_err());
        assert!(security::authorize(actor, "resources.resolve").is_err());
    }
    for variant in 0..9 {
        let mut invalid = settings.clone();
        match variant {
            0 => invalid.capacity.compute_slots = 0,
            1 => invalid.capacity.compute_slots = 257,
            2 => invalid.capacity.host_memory_bytes = u64::MAX,
            3 => invalid.capacity.device_memory_bytes = u64::MAX,
            4 => invalid.max_pending = 1025,
            5 => invalid.max_wait_ms = 60001,
            6 => invalid.max_execution_ms = 0,
            7 => invalid.max_execution_ms = 600001,
            _ => invalid.label = "bad/id".into(),
        }
        assert!(ResourcePool::new("human-cli", store.clone(), invalid).is_err());
    }
    let pool = pool(allocation(2, 100, 100));
    for a in [
        allocation(0, 0, 0),
        allocation(3, 0, 0),
        allocation(1, 101, 0),
        allocation(1, 0, 101),
    ] {
        assert!(pool.wrap_backend("human-cli", Gate::new(false), a).is_err());
    }
    assert!(pool
        .wrap_backend("jev-plus", Gate::new(false), allocation(1, 0, 0))
        .is_err());
}

#[test]
fn native_targets_keep_instance_identity_while_resources_match_only_canonical_base_and_model() {
    let resource_target = crate::backend::ModelTarget {
        api_base: "http://127.0.0.1:1234/api/v1".into(),
        model: "native-model-key".into(),
    };
    let configuration = Configuration {
        settings: Settings {
            label: "native_pool".into(),
            capacity: allocation(2, 100, 100),
            max_pending: 4,
            max_wait_ms: 1000,
            max_execution_ms: 1000,
            scheduling: None,
        },
        allocation: allocation(1, 8, 8),
        residents: vec![resident::ResidentAllocation {
            id: "native.shared".into(),
            version: "1.0.0".into(),
            weights_sha256: "a".repeat(64),
            host_memory_bytes: 20,
            device_memory_bytes: 25,
            consumers: vec![resource_target.clone()],
        }],
        working_profiles: vec![WorkingProfile {
            id: "native.solver".into(),
            target: resource_target,
            roles: vec!["solver".into()],
            allocation: allocation(1, 12, 8),
        }],
    };
    configuration.validate().unwrap();
    let store = Arc::new(Store::in_memory().unwrap());
    let pool = ResourcePool::configured(store, &configuration).unwrap();
    let target = crate::backend::NativeModelTarget {
        api_base: "http://127.0.0.1:1234/api/v1".into(),
        model: "native-model-key".into(),
        model_instance_id: "model-instance-a".into(),
    };
    let selected = pool
        .routing_candidate(
            "solver",
            Some(crate::backend::InferenceTarget::Native(target.clone())),
        )
        .unwrap();
    assert_eq!(
        selected.target,
        crate::backend::InferenceTarget::Native(target.clone())
    );
    assert_eq!(
        selected.metadata["target"],
        serde_json::to_value(crate::backend::InferenceTarget::Native(target.clone())).unwrap()
    );
    assert_eq!(selected.metadata["profile_id"], "native.solver");
    assert_eq!(
        selected.metadata["resident_coverage"]["target"],
        serde_json::to_value(crate::backend::InferenceTarget::Native(target.clone())).unwrap()
    );

    let managed = crate::backend::ManagedNativeTarget {
        api_base: target.api_base.clone(),
        model: target.model.clone(),
        policy_sha256: "b".repeat(64),
    };
    let managed_selected = pool
        .routing_candidate(
            "solver",
            Some(crate::backend::InferenceTarget::ManagedNative(
                managed.clone(),
            )),
        )
        .unwrap();
    assert_eq!(
        managed_selected.target,
        crate::backend::InferenceTarget::ManagedNative(managed.clone())
    );
    assert_eq!(managed_selected.metadata["profile_id"], "native.solver");
    assert_eq!(
        managed_selected.metadata["resident_coverage"]["target"],
        serde_json::to_value(crate::backend::InferenceTarget::ManagedNative(
            managed.clone()
        ))
        .unwrap()
    );
    let invalid_policy = crate::backend::ManagedNativeTarget {
        policy_sha256: "B".repeat(64),
        ..managed
    };
    assert!(matches!(
        pool.routing_candidate(
            "solver",
            Some(crate::backend::InferenceTarget::ManagedNative(
                invalid_policy
            ))
        ),
        Err(RoutingRejection::MetadataError)
    ));

    let other_instance = crate::backend::NativeModelTarget {
        model_instance_id: "model-instance-b".into(),
        ..target.clone()
    };
    let other = pool
        .routing_candidate(
            "solver",
            Some(crate::backend::InferenceTarget::Native(
                other_instance.clone(),
            )),
        )
        .unwrap();
    assert_eq!(
        other.target,
        crate::backend::InferenceTarget::Native(other_instance)
    );
    assert_ne!(other.metadata["target"], selected.metadata["target"]);

    let wrong_model = crate::backend::NativeModelTarget {
        model: "unlisted-native-model".into(),
        ..target.clone()
    };
    assert!(matches!(
        pool.routing_candidate(
            "solver",
            Some(crate::backend::InferenceTarget::Native(wrong_model))
        ),
        Err(RoutingRejection::UnlistedProfile)
    ));
    let openai_protocol_base = crate::backend::NativeModelTarget {
        api_base: "http://127.0.0.1:1234/v1".into(),
        ..target
    };
    assert!(matches!(
        pool.routing_candidate(
            "solver",
            Some(crate::backend::InferenceTarget::Native(
                openai_protocol_base
            ))
        ),
        Err(RoutingRejection::MetadataError)
    ));
}

#[tokio::test]
async fn fifo_admits_all_dimensions_atomically_without_smaller_request_starvation() {
    let pool = pool(allocation(3, 100, 100));
    let gate = Gate::new(false);
    let a = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 60, 20))
        .unwrap();
    let b = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 50, 20))
        .unwrap();
    let c = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 10, 10))
        .unwrap();
    let first = call(a, task(&pool));
    gate.entered(1).await;
    let second = call(b, task(&pool));
    pending(&pool, 1).await;
    let third = call(c, task(&pool));
    pending(&pool, 2).await;
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(1, 60, 20));
    assert_eq!(gate.entered.load(Ordering::SeqCst), 1);
    gate.complete.add_permits(1);
    first.await.unwrap().unwrap();
    gate.entered(3).await;
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(2, 60, 30));
    gate.complete.add_permits(2);
    second.await.unwrap().unwrap();
    third.await.unwrap().unwrap();
    assert_eq!(pool.snapshot().unwrap().allocated, Allocation::default());
    assert!(pool.snapshot().unwrap().reservations.is_empty());
}
#[tokio::test]
async fn device_memory_alone_blocks_and_waiter_cancellation_unblocks_fifo() {
    let pool = pool(allocation(3, 100, 100));
    let gate = Gate::new(false);
    let a = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 10, 80))
        .unwrap();
    let b = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 10, 50))
        .unwrap();
    let c = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 10, 10))
        .unwrap();
    let first = call(a, task(&pool));
    gate.entered(1).await;
    let second = call(b, task(&pool));
    pending(&pool, 1).await;
    let third = call(c, task(&pool));
    pending(&pool, 2).await;
    second.abort();
    assert!(second.await.unwrap_err().is_cancelled());
    gate.entered(2).await;
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(2, 20, 90));
    gate.complete.add_permits(2);
    first.await.unwrap().unwrap();
    third.await.unwrap().unwrap();
}
#[tokio::test]
async fn dropped_dispatched_request_quarantines_until_audited_operator_resolution() {
    let pool = pool(allocation(1, 100, 100));
    let gate = Gate::new(false);
    let backend = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 20, 30))
        .unwrap();
    let first = call(backend.clone(), task(&pool));
    gate.entered(1).await;
    let id = pool.snapshot().unwrap().reservations[0]
        .reservation_id
        .clone();
    assert!(pool.acknowledge_stopped("human-cli", &id, true).is_err());
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    let snapshot = pool.snapshot().unwrap();
    assert_eq!(snapshot.reservations[0].phase, Phase::Quarantined);
    assert_eq!(snapshot.allocated, allocation(1, 20, 30));
    let next = call(backend, task(&pool));
    pending(&pool, 1).await;
    assert!(pool.acknowledge_stopped("jev-plus", &id, true).is_err());
    assert!(pool.acknowledge_stopped("human-cli", &id, false).is_err());
    pool.store.lock().unwrap().execute_batch("CREATE TRIGGER refuse_release BEFORE INSERT ON events WHEN NEW.kind='resource_release' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    assert!(pool.acknowledge_stopped("human-cli", &id, true).is_err());
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(1, 20, 30));
    pool.store
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER refuse_release")
        .unwrap();
    pool.acknowledge_stopped("human-cli", &id, true).unwrap();
    gate.entered(2).await;
    gate.complete.add_permits(1);
    next.await.unwrap().unwrap();
    assert!(pool.acknowledge_stopped("human-cli", &id, true).is_err());
}
#[tokio::test]
async fn provider_error_and_completion_audit_failure_retain_allocations() {
    for fail in [true, false] {
        let pool = pool(allocation(1, 100, 100));
        let gate = Gate::new(fail);
        if !fail {
            pool.store.lock().unwrap().execute_batch("CREATE TRIGGER refuse_completed BEFORE INSERT ON events WHEN NEW.kind='resource_completed' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        }
        let backend = pool
            .wrap_backend("human-cli", gate.clone(), allocation(1, 1, 1))
            .unwrap();
        let t = task(&pool);
        let first = call(backend, t.clone());
        gate.entered(1).await;
        gate.complete.add_permits(1);
        assert!(first.await.unwrap().is_err());
        assert_eq!(
            pool.snapshot().unwrap().reservations[0].phase,
            Phase::Quarantined
        );
        let events = pool.store.task(&t).unwrap();
        assert!(events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "resource_dispatch"));
        assert!(!events.to_string().contains("Fixture provider error"));
    }
}
#[tokio::test]
async fn admission_audit_failures_never_reach_provider_or_leak_resources() {
    for kind in ["resource_intent", "resource_admitted", "resource_dispatch"] {
        let pool = pool(allocation(1, 100, 100));
        let gate = Gate::new(false);
        pool.store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER refuse BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let backend = pool
            .wrap_backend("human-cli", gate.clone(), allocation(1, 1, 1))
            .unwrap();
        assert!(call(backend, task(&pool)).await.unwrap().is_err());
        assert_eq!(gate.entered.load(Ordering::SeqCst), 0);
        assert!(pool.snapshot().unwrap().reservations.is_empty());
    }
}
#[tokio::test(start_paused = true)]
async fn queue_limit_timeout_and_shutdown_release_only_undispatched_requests() {
    let store = Arc::new(Store::in_memory().unwrap());
    let pool = ResourcePool::new(
        "human-cli",
        store,
        Settings {
            label: "gpu.main".into(),
            capacity: allocation(1, 100, 100),
            max_pending: 1,
            max_wait_ms: 50,
            max_execution_ms: 1000,
            scheduling: None,
        },
    )
    .unwrap();
    let gate = Gate::new(false);
    let backend = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 1, 1))
        .unwrap();
    let first = call(backend.clone(), task(&pool));
    gate.entered(1).await;
    let second = call(backend.clone(), task(&pool));
    pending(&pool, 1).await;
    assert!(call(backend.clone(), task(&pool)).await.unwrap().is_err());
    tokio::time::advance(Duration::from_millis(51)).await;
    assert!(second.await.unwrap().is_err());
    pending(&pool, 0).await;
    let third = call(backend.clone(), task(&pool));
    pending(&pool, 1).await;
    assert!(pool.shutdown("jev-plus").is_err());
    pool.shutdown("human-cli").unwrap();
    assert!(third.await.unwrap().is_err());
    assert_eq!(backend.availability(), crate::fabric::Availability::Closed);
    assert!(backend
        .generate_for(&task(&pool), "solver", "fixture", 1)
        .await
        .is_err());
    assert_eq!(pool.snapshot().unwrap().allocated.compute_slots, 1);
    gate.complete.add_permits(1);
    first.await.unwrap().unwrap();
    assert!(pool.snapshot().unwrap().reservations.is_empty());
}
#[tokio::test]
async fn explicit_tasks_roles_and_request_bounds_are_checked_before_any_reservation() {
    let pool = pool(allocation(1, 0, 0));
    let gate = Gate::new(false);
    let backend = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 0, 0))
        .unwrap();
    let valid = task(&pool);
    assert!(backend.generate("solver", "fixture", 1).await.is_err());
    for (id, role, prompt, tokens) in [
        ("invalid", "solver", "fixture", 1),
        (
            "00000000-0000-0000-0000-000000000000",
            "solver",
            "fixture",
            1,
        ),
        (&valid, "unknown", "fixture", 1),
        (&valid, "solver", "fixture", 0),
        (&valid, "solver", "fixture", 16385),
    ] {
        assert!(backend
            .generate_for(id, role, prompt, tokens)
            .await
            .is_err());
    }
    assert!(backend
        .generate_for(&valid, "solver", &"x".repeat(32001), 1)
        .await
        .is_err());
    assert!(pool.snapshot().unwrap().reservations.is_empty());
    assert!(pool.store.task(&valid).unwrap()["events"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[tokio::test]
async fn shared_pool_coordinates_independent_brokers_and_preserves_task_charges() {
    let pool = pool(allocation(1, 100, 100));
    let gate = Gate::new(false);
    let cfg = Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 1,
        ..Config::default()
    };
    let a = Engine::with_backend(
        cfg.clone(),
        pool.store.clone(),
        pool.wrap_backend("human-cli", gate.clone(), allocation(1, 40, 70))
            .unwrap(),
    );
    let b = Engine::with_backend(
        cfg,
        pool.store.clone(),
        pool.wrap_backend("human-cli", gate.clone(), allocation(1, 40, 70))
            .unwrap(),
    );
    let first = tokio::spawn(async move { a.run("First generic objective", "fast").await });
    gate.entered(1).await;
    let second = tokio::spawn(async move { b.run("Second generic objective", "fast").await });
    pending(&pool, 1).await;
    gate.complete.add_permits(1);
    let a = first.await.unwrap().unwrap();
    gate.entered(2).await;
    gate.complete.add_permits(1);
    let b = second.await.unwrap().unwrap();
    for result in [a, b] {
        assert_eq!(result.calls, 1);
        assert_eq!(result.answer, "fixture answer");
        let events = pool.store.task(&result.task_id).unwrap();
        for kind in [
            "model_dispatch",
            "resource_intent",
            "resource_admitted",
            "resource_dispatch",
            "resource_completed",
        ] {
            assert_eq!(
                events["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["kind"] == kind)
                    .count(),
                1,
                "{kind}"
            );
        }
    }
    assert_eq!(pool.snapshot().unwrap().allocated, Allocation::default());
}

#[tokio::test(start_paused = true)]
async fn execution_deadline_quarantines_instead_of_refunding_or_retrying() {
    let pool = pool(allocation(1, 100, 100));
    let gate = Gate::new(false);
    let backend = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 5, 7))
        .unwrap();
    let t = task(&pool);
    let call = call(backend, t.clone());
    gate.entered(1).await;
    tokio::time::advance(Duration::from_millis(1001)).await;
    assert!(call
        .await
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("execution deadline"));
    let snapshot = pool.snapshot().unwrap();
    assert_eq!(snapshot.allocated, allocation(1, 5, 7));
    assert_eq!(snapshot.reservations[0].phase, Phase::Quarantined);
    assert_eq!(gate.entered.load(Ordering::SeqCst), 1);
    assert!(pool.store.task(&t).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "resource_outcome_unknown"));
}

#[tokio::test]
async fn backend_pinning_preserves_transport_identity_and_shared_allocation() {
    struct Bound {
        descriptor: crate::fabric::ModuleDescriptor,
        gate: Arc<Gate>,
    }
    impl InferenceBackend for Bound {
        fn module_descriptor(&self) -> Option<&crate::fabric::ModuleDescriptor> {
            Some(&self.descriptor)
        }
        fn modules(&self) -> Result<Vec<crate::fabric::ModuleDescriptor>> {
            Ok(vec![self.descriptor.clone()])
        }
        fn generate<'a>(
            &'a self,
            role: &'a str,
            prompt: &'a str,
            tokens: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            self.gate.generate(role, prompt, tokens)
        }
        fn pin(&self) -> Result<Option<Arc<dyn InferenceBackend>>> {
            Ok(Some(Arc::new(Self {
                descriptor: self.descriptor.clone(),
                gate: self.gate.clone(),
            })))
        }
    }
    let pool = pool(allocation(1, 100, 100));
    let gate = Gate::new(false);
    let descriptor = crate::fabric::ModuleDescriptor {
        id: "fixture.bound".into(),
        version: "1.0.0".into(),
        roles: vec!["solver".into()],
    };
    let backend = pool
        .wrap_backend(
            "human-cli",
            Arc::new(Bound {
                descriptor: descriptor.clone(),
                gate: gate.clone(),
            }),
            allocation(1, 30, 50),
        )
        .unwrap();
    assert_eq!(backend.module_descriptor(), Some(&descriptor));
    let pinned = backend.pin().unwrap().unwrap();
    assert_eq!(pinned.module_descriptor(), Some(&descriptor));
    assert_eq!(pinned.modules().unwrap(), vec![descriptor]);
    let first = call(pinned, task(&pool));
    gate.entered(1).await;
    let second = call(backend, task(&pool));
    pending(&pool, 1).await;
    gate.complete.add_permits(1);
    let first = first.await.unwrap().unwrap();
    assert!(first.queue_wait_ms.unwrap() < 99999.0);
    gate.entered(2).await;
    gate.complete.add_permits(1);
    second.await.unwrap().unwrap();
}

#[tokio::test]
async fn cancellation_after_fifo_grant_but_before_waiter_poll_cannot_leak_capacity() {
    let pool = pool(allocation(1, 100, 100));
    let first = pool
        .acquire(
            &task(&pool),
            allocation(1, 40, 60),
            uuid::Uuid::new_v4().to_string(),
        )
        .await
        .unwrap();
    let p = pool.clone();
    let t = task(&pool);
    let second = tokio::spawn(async move {
        p.acquire(&t, allocation(1, 40, 60), uuid::Uuid::new_v4().to_string())
            .await
    });
    pending(&pool, 1).await;
    first.release().unwrap();
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(1, 40, 60));
    second.abort();
    assert!(second.await.is_err());
    assert_eq!(pool.snapshot().unwrap().allocated, Allocation::default());
    assert!(pool.snapshot().unwrap().reservations.is_empty());
}

#[tokio::test]
async fn engine_cancellation_keeps_spent_task_charge_and_quarantines_broker_dispatch() {
    let pool = pool(allocation(1, 100, 100));
    let gate = Gate::new(false);
    let engine = Engine::with_backend(
        Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            max_calls: 1,
            ..Config::default()
        },
        pool.store.clone(),
        pool.wrap_backend("human-cli", gate.clone(), allocation(1, 20, 50))
            .unwrap(),
    );
    let (stop, cancel) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        engine
            .run_until("Generic cancellable objective", "fast", async {
                let _ = cancel.await;
            })
            .await
    });
    gate.entered(1).await;
    stop.send(()).unwrap();
    let result = running.await.unwrap().unwrap();
    assert_eq!(result.status, "cancelled");
    assert_eq!(result.calls, 1);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = pool.snapshot().unwrap();
            if snapshot.reservations.len() == 1
                && snapshot.reservations[0].phase == Phase::Quarantined
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(pool.snapshot().unwrap().allocated, allocation(1, 20, 50));
    let events = pool.store.task(&result.task_id).unwrap();
    assert_eq!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "model_dispatch")
            .count(),
        1
    );
    assert!(!events["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "resource_completed"));
}

#[tokio::test]
async fn task_terminated_while_waiting_never_enters_the_provider() {
    let pool = pool(allocation(1, 100, 100));
    let gate = Gate::new(false);
    let backend = pool
        .wrap_backend("human-cli", gate.clone(), allocation(1, 40, 50))
        .unwrap();
    let first = call(backend.clone(), task(&pool));
    gate.entered(1).await;
    let waiting = task(&pool);
    let second = call(backend, waiting.clone());
    pending(&pool, 1).await;
    pool.store.finish_task(&waiting, "cancelled", "").unwrap();
    gate.complete.add_permits(1);
    first.await.unwrap().unwrap();
    assert!(second.await.unwrap().is_err());
    assert_eq!(gate.entered.load(Ordering::SeqCst), 1);
    assert!(pool.snapshot().unwrap().reservations.is_empty());
    assert!(!pool.store.task(&waiting).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "resource_dispatch"));
}

fn configured() -> Configuration {
    Configuration {
        residents: Vec::new(),
        settings: Settings {
            label: "gpu.configured".into(),
            capacity: allocation(1, 100, 100),
            max_pending: 4,
            max_wait_ms: 1000,
            max_execution_ms: 1000,
            scheduling: None,
        },
        allocation: allocation(1, 40, 60),
        working_profiles: Vec::new(),
    }
}
#[tokio::test]
async fn configured_engine_clones_and_native_agent_share_one_pool_and_keep_original_budgets() {
    struct Chat(Arc<Gate>);
    impl crate::agent::ChatBackend for Chat {
        fn complete<'a>(
            &'a self,
            _: &'a crate::agent::Turn,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<crate::agent::Completion>> + Send + 'a>> {
            Box::pin(async move {
                let report = self.0.generate("solver", "fixture", 16).await?;
                Ok(crate::agent::Completion {
                    content: Some(report.text),
                    tool_calls: vec![],
                    model: report.model,
                    finish_reason: "stop".into(),
                    input_tokens: Some(1),
                    output_tokens: Some(1),
                })
            })
        }
    }
    let store = Arc::new(Store::in_memory().unwrap());
    let gate = Gate::new(false);
    let cfg = Config {
        resource_admission: Some(configured()),
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 1,
        ..Config::default()
    };
    let engine = Engine::with_backend(cfg.clone(), store.clone(), gate.clone());
    let pool = engine.configured_resource_pool().unwrap().unwrap();
    let clone = engine.clone();
    assert!(Arc::ptr_eq(
        &pool,
        &clone.configured_resource_pool().unwrap().unwrap()
    ));
    let agent =
        crate::agent::AgentRuntime::with_backend(cfg, store.clone(), Arc::new(Chat(gate.clone())))
            .unwrap();
    let agent = agent
        .with_resource_pool("human-cli", Some(pool.clone()))
        .unwrap();
    let first = tokio::spawn(async move { engine.run("First collective objective", "fast").await });
    gate.entered(1).await;
    let second = tokio::spawn(async move {
        agent.run_until(serde_json::from_value(json!({"messages":[{"role":"user","content":"Native objective"}],"max_tokens":16})).unwrap(),std::future::pending()).await
    });
    pending(&pool, 1).await;
    assert_eq!(gate.entered.load(Ordering::SeqCst), 1);
    gate.complete.add_permits(1);
    let result = first.await.unwrap().unwrap();
    assert_eq!(result.calls, 1);
    gate.entered(2).await;
    gate.complete.add_permits(1);
    let native = second.await.unwrap().unwrap();
    for id in [&result.task_id, &native.task_id] {
        let trace = store.task(id).unwrap();
        let events = trace["events"].as_array().unwrap();
        let admission = events
            .iter()
            .find(|e| e["kind"] == "resource_admitted")
            .unwrap();
        assert_eq!(admission["payload"]["pool_id"], pool.id);
        assert_eq!(
            events
                .iter()
                .filter(|e| e["kind"] == "resource_dispatch")
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e["kind"] == "resource_completed")
                .count(),
            1
        );
    }
    let mut changed = clone;
    changed
        .cfg
        .resource_admission
        .as_mut()
        .unwrap()
        .settings
        .capacity
        .compute_slots = 2;
    assert!(changed.configured_resource_pool().is_err());
    changed.cfg.resource_admission = None;
    assert!(changed.configured_resource_pool().is_err());
}
#[test]
fn configured_pool_excludes_other_workspace_owners_and_releases_the_lease_on_drop() {
    let root = std::env::temp_dir().join(format!(
        "fabric-configured-resources-{}",
        uuid::Uuid::new_v4()
    ));
    let store = Arc::new(Store::open(&root.join("state.db")).unwrap());
    let pool = ResourcePool::configured(store.clone(), &configured()).unwrap();
    let other = Arc::new(Store::open(&root.join("state.db")).unwrap());
    assert!(ResourcePool::configured(other.clone(), &configured()).is_err());
    drop(pool);
    assert!(ResourcePool::configured(other, &configured()).is_ok());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn configured_startup_and_offline_reconciliation_validate_original_audit_identity() {
    for mutation in 0..6 {
        let root = std::env::temp_dir().join(format!(
            "fabric-resource-reconciliation-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(Store::open(&root.join("state.db")).unwrap());
        let task = task_for_store(&store);
        let id = uuid::Uuid::new_v4().to_string();
        let pool = uuid::Uuid::new_v4().to_string();
        let dispatch = json!({"format_version":1,"pool_id":pool,"reservation_id":id,"provider_stop_proven":false});
        store
            .event(&task, "resource_dispatch", "jev-plus", dispatch.clone())
            .unwrap();
        assert!(ResourcePool::configured(store.clone(), &configured()).is_err());
        let before = store.task(&task).unwrap();
        assert!(ResourcePool::release_abandoned("jev-plus", &store, &id, true).is_err());
        assert!(ResourcePool::release_abandoned("human-cli", &store, &id, false).is_err());
        assert_eq!(store.task(&task).unwrap(), before);
        match mutation {
            0 => {
                ResourcePool::release_abandoned("human-cli", &store, &id, true).unwrap();
                assert!(ResourcePool::configured(store.clone(), &configured()).is_ok());
                assert!(ResourcePool::release_abandoned("human-cli", &store, &id, true).is_err());
            }
            1 => {
                let mut complete = dispatch.clone();
                complete["provider_request_completed"] = json!(true);
                complete["pool_id"] = json!(uuid::Uuid::new_v4().to_string());
                store
                    .event(&task, "resource_completed", "jev-plus", complete)
                    .unwrap();
                assert!(ResourcePool::abandoned_status("human-cli", &store).is_err());
            }
            2 => {
                store
                    .event(&task, "resource_dispatch", "jev-plus", dispatch.clone())
                    .unwrap();
                assert!(ResourcePool::abandoned_status("human-cli", &store).is_err());
            }
            3 => {
                store.lock().unwrap().execute_batch("CREATE TRIGGER refuse BEFORE INSERT ON events WHEN NEW.kind='resource_release' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
                assert!(ResourcePool::release_abandoned("human-cli", &store, &id, true).is_err());
                assert_eq!(store.task(&task).unwrap(), before);
            }
            4 => {
                let held =
                    ResourcePool::new("human-cli", store.clone(), configured().settings).unwrap();
                let mut lock = store
                    .runtime_lock
                    .as_ref()
                    .unwrap()
                    .as_os_str()
                    .to_os_string();
                lock.push(".resources");
                let _lease =
                    crate::recovery::ExecutionLease::acquire(std::path::Path::new(&lock), true)
                        .unwrap();
                assert!(ResourcePool::release_abandoned("human-cli", &store, &id, true).is_err());
                drop(held);
            }
            _ => {
                store.lock().unwrap().execute("UPDATE events SET payload=?1 WHERE kind='resource_dispatch'",[format!("{{\"format_version\":1,\"pool_id\":\"{pool}\",\"reservation_id\":\"{id}\",\"provider_stop_proven\":false,\"provider_stop_proven\":true}}")]).unwrap();
                assert!(ResourcePool::abandoned_status("human-cli", &store).is_err());
            }
        }
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
fn task_for_store(store: &Store) -> String {
    store.create_task("Resource audit fixture", "fast").unwrap()
}

#[test]
fn retention_preserves_uncertain_resources_until_reviewed_release() {
    let store = Store::in_memory().unwrap();
    let task = task_for_store(&store);
    let id = uuid::Uuid::new_v4().to_string();
    let pool = uuid::Uuid::new_v4().to_string();
    store.event(&task,"resource_dispatch","jev-plus",json!({"format_version":1,"reservation_id":id,"pool_id":pool,"provider_stop_proven":false})).unwrap();
    store.finish_task(&task, "failed", "").unwrap();
    store
        .lock()
        .unwrap()
        .execute("UPDATE tasks SET updated=0 WHERE id=?1", [&task])
        .unwrap();
    assert!(store.retention_preview(1, 100).unwrap().task_ids.is_empty());
    store.event(&task,"resource_release","human-cli",json!({"format_version":1,"reservation_id":id,"pool_id":pool,"operator_acknowledged_stop":true,"provider_stop_proven":false})).unwrap();
    let plan = store.retention_preview(1, 100).unwrap();
    assert_eq!(plan.task_ids, vec![task]);
    store.apply_retention(&plan, true).unwrap();
}
