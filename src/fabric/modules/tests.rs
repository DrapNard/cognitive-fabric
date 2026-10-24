use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture {
    descriptor: Descriptor,
    calls: Arc<AtomicUsize>,
    panic: bool,
}

impl Fixture {
    fn new(version: &str, calls: Arc<AtomicUsize>) -> Self {
        Self::named("fixture.math", version, calls)
    }

    fn named(id: &str, version: &str, calls: Arc<AtomicUsize>) -> Self {
        Self {
            descriptor: Descriptor {
                id: id.into(),
                version: version.into(),
                capabilities: [Capability::MathEvaluate].into_iter().collect(),
            },
            calls,
            panic: false,
        }
    }

    fn panicking(calls: Arc<AtomicUsize>) -> Self {
        let mut module = Self::new("1.0.0", calls);
        module.panic = true;
        module
    }
}

impl Module for Fixture {
    fn descriptor(&self) -> Descriptor {
        self.descriptor.clone()
    }

    fn execute<'a>(
        &'a self,
        _task: &'a str,
        _deadline: Instant,
        _request: Request,
    ) -> ModuleFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.panic {
            Box::pin(async { panic!("fixture module panic") })
        } else {
            Box::pin(async move {
                match _request {
                    Request::Arithmetic { .. } => Ok(Response::Arithmetic { value: "42".into() }),
                    Request::FiniteDomain { criterion } => Ok(Response::FiniteAssignment {
                        values: criterion
                            .variables
                            .into_iter()
                            .map(|variable| (variable.id, variable.domain[0]))
                            .collect(),
                    }),
                    _ => bail!("unexpected fixture request"),
                }
            })
        }
    }
}

fn setup() -> (Broker, String, TaskPolicy, Grant) {
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store.create_task("module fixture", "standard").unwrap();
    let policy = TaskPolicy::reasoning(&task).unwrap();
    let grant = Grant::narrow(&task, "jev-plus", [Capability::MathEvaluate], 4, &policy).unwrap();
    (Broker::new(store), task, policy, grant)
}

fn arithmetic() -> Request {
    Request::Arithmetic {
        expression: "6 * 7".into(),
    }
}

#[tokio::test]
async fn policy_grants_cannot_expand_to_finite_search() {
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store
        .create_task("finite search fixture", "research")
        .unwrap();
    let ordinary = TaskPolicy::reasoning(&task).unwrap();
    assert!(Grant::narrow(
        &task,
        "jev-plus",
        [Capability::ConstraintsSolve],
        1,
        &ordinary,
    )
    .is_err());

    let finite_policy = TaskPolicy::reasoning_with_finite_search(&task).unwrap();
    let grant = Grant::narrow(
        &task,
        "jev-plus",
        [Capability::ConstraintsSolve],
        1,
        &finite_policy,
    )
    .unwrap();
    let broker = Broker::new(store);
    let module = Arc::new(Fixture {
        descriptor: Descriptor {
            id: "fixture.finite".into(),
            version: "1.0.0".into(),
            capabilities: [Capability::ConstraintsSolve].into_iter().collect(),
        },
        calls: Arc::new(AtomicUsize::new(0)),
        panic: false,
    });
    assert_eq!(grant.task, task);
    assert_eq!(
        module.descriptor().capabilities,
        [Capability::ConstraintsSolve].into()
    );
    broker.register(module, Limits::default()).unwrap();
    let pin = broker.pin(&task, "fixture.finite", "1.0.0").unwrap();
    let request = Request::FiniteDomain {
        criterion: FiniteCriterion {
            variables: vec![FiniteVariable {
                id: "x".into(),
                domain: vec![1],
            }],
            constraints: vec![FiniteConstraint::Linear {
                terms: vec![LinearTerm {
                    variable: "x".into(),
                    coefficient: 1,
                }],
                relation: Relation::Equal,
                rhs: 1,
            }],
        },
    };
    assert!(broker
        .dispatch(
            &pin,
            &grant,
            &ordinary,
            "jev-plus",
            request.clone(),
            Instant::now() + Duration::from_secs(2)
        )
        .await
        .is_err());
    let response = broker
        .dispatch(
            &pin,
            &grant,
            &finite_policy,
            "jev-plus",
            request,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(
        response,
        Response::FiniteAssignment {
            values: [("x".into(), 1)].into()
        }
    );
}

#[tokio::test]
async fn process_module_cannot_be_called_without_broker_authorization() {
    let store = Arc::new(Store::in_memory().unwrap());
    let module = ProcessModule::builtin(store).unwrap();
    let result = module
        .execute(
            "00000000-0000-4000-8000-000000000001",
            Instant::now() + Duration::from_secs(2),
            arithmetic(),
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn grant_and_policy_intersection_controls_dispatch() {
    let (broker, task, policy, grant) = setup();
    let calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(Fixture::new("1.0.0", calls.clone())),
            Limits::default(),
        )
        .unwrap();
    let pin = broker.pin(&task, "fixture.math", "1.0.0").unwrap();
    let response = broker
        .dispatch(
            &pin,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(response, Response::Arithmetic { value: "42".into() });
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let other_task = uuid::Uuid::new_v4().to_string();
    let other_policy = TaskPolicy::reasoning(&other_task).unwrap();
    assert!(broker
        .dispatch(
            &pin,
            &grant,
            &other_policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_task_pin_keeps_its_version_across_hot_swap() {
    let (broker, task, policy, grant) = setup();
    let old_calls = Arc::new(AtomicUsize::new(0));
    let new_calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(Fixture::new("1.0.0", old_calls.clone())),
            Limits::default(),
        )
        .unwrap();
    let old_pin = broker.pin(&task, "fixture.math", "1.0.0").unwrap();
    broker
        .register(
            Arc::new(Fixture::new("2.0.0", new_calls.clone())),
            Limits::default(),
        )
        .unwrap();
    let new_pin = broker.pin(&task, "fixture.math", "2.0.0").unwrap();
    for pin in [&old_pin, &new_pin] {
        broker
            .dispatch(
                pin,
                &grant,
                &policy,
                "jev-plus",
                arithmetic(),
                Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap();
    }
    assert_eq!(old_calls.load(Ordering::SeqCst), 1);
    assert_eq!(new_calls.load(Ordering::SeqCst), 1);
    assert_eq!(old_pin.descriptor().version, "1.0.0");
}

#[tokio::test]
async fn overlapping_active_capabilities_skip_a_full_candidate_before_intent() {
    let (broker, task, policy, grant) = setup();
    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(Fixture::named(
                "fixture.alpha",
                "1.0.0",
                first_calls.clone(),
            )),
            Limits {
                queue_capacity: 1,
                concurrency: 1,
            },
        )
        .unwrap();
    broker
        .register(
            Arc::new(Fixture::named(
                "fixture.beta",
                "1.0.0",
                second_calls.clone(),
            )),
            Limits::default(),
        )
        .unwrap();
    let (snapshot_pins, routes) = broker.pin_active_routes(&task).unwrap();
    assert_eq!(snapshot_pins.len(), 2);
    let candidates = routes.get(&Capability::MathEvaluate).unwrap();
    assert_eq!(
        candidates
            .candidates()
            .iter()
            .map(|pin| pin.descriptor().id.as_str())
            .collect::<Vec<_>>(),
        ["fixture.alpha", "fixture.beta"]
    );
    broker
        .register(
            Arc::new(Fixture::named(
                "fixture.gamma",
                "1.0.0",
                Arc::new(AtomicUsize::new(0)),
            )),
            Limits::default(),
        )
        .unwrap();
    assert_eq!(snapshot_pins.len(), 2);
    assert_eq!(candidates.candidates().len(), 2);

    // Occupy alpha's only bounded queue slot without creating dispatch intent.
    let alpha = candidates.exact("fixture.alpha", "1.0.0").unwrap();
    let occupied = broker
        .reserve_exact(alpha, Instant::now() + Duration::from_secs(2))
        .await
        .unwrap();
    let selected = broker
        .reserve_candidate(
            candidates,
            Capability::MathEvaluate,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(selected.pin().descriptor().id, "fixture.beta");
    drop(occupied);

    let response = broker
        .dispatch_reserved(
            selected,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(response, Response::Arithmetic { value: "42".into() });
    assert_eq!(first_calls.load(Ordering::SeqCst), 0);
    assert_eq!(second_calls.load(Ordering::SeqCst), 1);
    let trace = broker.store.task(&task).unwrap();
    let intents = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "module_dispatch_intent")
        .collect::<Vec<_>>();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0]["payload"]["module_id"], "fixture.beta");
}

#[tokio::test]
async fn a_candidate_reservation_cannot_be_reused_for_another_capability() {
    let (broker, task, policy, grant) = setup();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut module = Fixture::new("1.0.0", calls.clone());
    module.descriptor.capabilities.insert(Capability::SqlQuery);
    broker
        .register(Arc::new(module), Limits::default())
        .unwrap();
    let candidates = broker
        .pin_candidates(&task, Capability::MathEvaluate)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let reservation = broker
        .reserve_candidate(&candidates, Capability::MathEvaluate, deadline)
        .await
        .unwrap();
    let error = broker
        .dispatch_reserved(
            reservation,
            &grant,
            &policy,
            "jev-plus",
            Request::Sql {
                query: "SELECT 1".into(),
                tables: vec![],
            },
            deadline,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("reserved capability"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!broker.store.task(&task).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "module_dispatch_intent"));
}

#[tokio::test]
async fn a_reserved_slot_cannot_extend_its_original_deadline() {
    let (broker, task, policy, grant) = setup();
    let calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(Fixture::new("1.0.0", calls.clone())),
            Limits::default(),
        )
        .unwrap();
    let pin = broker.pin(&task, "fixture.math", "1.0.0").unwrap();
    let reservation = broker
        .reserve_exact(&pin, Instant::now() + Duration::from_millis(20))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(broker
        .dispatch_reserved(
            reservation,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2)
        )
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!broker.store.task(&task).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "module_dispatch_intent"));
}

#[tokio::test]
async fn a_dispatched_provider_failure_never_retries_an_alternate() {
    let (broker, task, policy, grant) = setup();
    let failed_calls = Arc::new(AtomicUsize::new(0));
    let alternate_calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(Fixture {
                descriptor: Descriptor {
                    id: "fixture.alpha".into(),
                    version: "1.0.0".into(),
                    capabilities: [Capability::MathEvaluate].into(),
                },
                calls: failed_calls.clone(),
                panic: true,
            }),
            Limits::default(),
        )
        .unwrap();
    broker
        .register(
            Arc::new(Fixture::named(
                "fixture.beta",
                "1.0.0",
                alternate_calls.clone(),
            )),
            Limits::default(),
        )
        .unwrap();
    let candidates = broker
        .pin_candidates(&task, Capability::MathEvaluate)
        .unwrap();
    let reservation = broker
        .reserve_candidate(
            &candidates,
            Capability::MathEvaluate,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(reservation.pin().descriptor().id, "fixture.alpha");
    assert!(broker
        .dispatch_reserved(
            reservation,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .is_err());
    assert_eq!(failed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(alternate_calls.load(Ordering::SeqCst), 0);
    let trace = broker.store.task(&task).unwrap();
    let intents = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "module_dispatch_intent")
        .collect::<Vec<_>>();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0]["payload"]["module_id"], "fixture.alpha");
}

#[tokio::test]
async fn broker_rejects_a_foreign_pin_but_retained_local_pin_survives_hot_swap() {
    let task = uuid::Uuid::new_v4().to_string();
    let store_a = Arc::new(Store::in_memory().unwrap());
    let store_b = Arc::new(Store::in_memory().unwrap());
    store_a
        .create_named_task(&task, "cross broker fixture", "standard")
        .unwrap();
    store_b
        .create_named_task(&task, "cross broker fixture", "standard")
        .unwrap();
    let policy = TaskPolicy::reasoning(&task).unwrap();
    let grant = Grant::narrow(&task, "jev-plus", [Capability::MathEvaluate], 3, &policy).unwrap();
    let broker_a = Broker::new(store_a);
    let broker_b = Broker::new(store_b);
    let a_v1_calls = Arc::new(AtomicUsize::new(0));
    let b_v1_calls = Arc::new(AtomicUsize::new(0));
    broker_a
        .register(
            Arc::new(Fixture::new("1.0.0", a_v1_calls.clone())),
            Limits::default(),
        )
        .unwrap();
    broker_b
        .register(
            Arc::new(Fixture::new("1.0.0", b_v1_calls.clone())),
            Limits::default(),
        )
        .unwrap();
    let local_pin = broker_a.pin(&task, "fixture.math", "1.0.0").unwrap();
    let foreign_pin = broker_b.pin(&task, "fixture.math", "1.0.0").unwrap();
    assert_eq!(local_pin.descriptor(), foreign_pin.descriptor());

    assert!(broker_a
        .dispatch(
            &foreign_pin,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .is_err());
    assert_eq!(b_v1_calls.load(Ordering::SeqCst), 0);

    let a_v2_calls = Arc::new(AtomicUsize::new(0));
    broker_a
        .register(
            Arc::new(Fixture::new("2.0.0", a_v2_calls.clone())),
            Limits::default(),
        )
        .unwrap();
    broker_a
        .dispatch(
            &local_pin,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(a_v1_calls.load(Ordering::SeqCst), 1);
    assert_eq!(a_v2_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn panicked_module_is_unavailable_and_never_retried_or_replaced() {
    let (broker, task, policy, grant) = setup();
    let calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(Fixture::panicking(calls.clone())),
            Limits::default(),
        )
        .unwrap();
    let pin = broker.pin(&task, "fixture.math", "1.0.0").unwrap();
    assert!(broker
        .dispatch(
            &pin,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .is_err());
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if broker
                .status()
                .unwrap()
                .iter()
                .all(|status| !status.available)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(broker.discover().unwrap().is_empty());
    assert!(broker.pin(&task, "fixture.math", "1.0.0").is_err());
    assert!(broker.activate("fixture.math", "1.0.0").is_err());
    assert!(broker
        .dispatch(
            &pin,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn flatbuffer_protocol_rejects_bad_and_oversized_frames() {
    let envelope = protocol::Envelope {
        kind: protocol::Kind::Request,
        correlation: uuid::Uuid::new_v4().to_string(),
        task: uuid::Uuid::new_v4().to_string(),
        actor: "jev-plus".into(),
        module: "builtin.typed".into(),
        version: "1.0.0".into(),
        capability: Capability::MathEvaluate,
        expires: 1,
        request: Some(arithmetic()),
        response: None,
        failed: false,
        error: None,
    };
    let encoded = protocol::encode(&envelope).unwrap();
    assert_eq!(
        protocol::decode(&encoded).unwrap().request,
        Some(arithmetic())
    );
    assert!(protocol::decode(b"not-flatbuffer").is_err());
    assert!(protocol::decode(&vec![0; protocol::MAX_FRAME + 1]).is_err());
}

#[tokio::test]
async fn deadline_and_audit_failure_prevent_execution() {
    let (broker, task, policy, grant) = setup();
    let calls = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(Fixture::new("1.0.0", calls.clone())),
            Limits::default(),
        )
        .unwrap();
    let pin = broker.pin(&task, "fixture.math", "1.0.0").unwrap();
    assert!(broker
        .dispatch(
            &pin,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now()
        )
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    broker
        .store
        .lock()
        .unwrap()
        .execute_batch("CREATE TRIGGER reject_module_intent BEFORE INSERT ON events WHEN NEW.kind='module_dispatch_intent' BEGIN SELECT RAISE(FAIL, 'blocked by fixture'); END;")
        .unwrap();
    assert!(broker
        .dispatch(
            &pin,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn queue_capacity_and_cancellation_are_bounded() {
    struct Slow(Descriptor, Arc<AtomicUsize>, Arc<AtomicUsize>);
    impl Module for Slow {
        fn descriptor(&self) -> Descriptor {
            self.0.clone()
        }
        fn execute<'a>(
            &'a self,
            _task: &'a str,
            _deadline: Instant,
            _request: Request,
        ) -> ModuleFuture<'a> {
            self.1.fetch_add(1, Ordering::SeqCst);
            let dropped = self.2.clone();
            Box::pin(async move {
                struct OnDrop(Arc<AtomicUsize>);
                impl Drop for OnDrop {
                    fn drop(&mut self) {
                        self.0.fetch_add(1, Ordering::SeqCst);
                    }
                }
                let _on_drop = OnDrop(dropped);
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(Response::Arithmetic {
                    value: "late".into(),
                })
            })
        }
    }
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store.create_task("slow fixture", "standard").unwrap();
    let broker = Arc::new(Broker::new(store));
    let calls = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    broker
        .register(
            Arc::new(Slow(
                Descriptor {
                    id: "slow.math".into(),
                    version: "1.0.0".into(),
                    capabilities: [Capability::MathEvaluate].into(),
                },
                calls.clone(),
                dropped.clone(),
            )),
            Limits {
                queue_capacity: 1,
                concurrency: 1,
            },
        )
        .unwrap();
    let pin = broker.pin(&task, "slow.math", "1.0.0").unwrap();
    let dispatch = |broker: Arc<Broker>, pin: Pin, task: String| {
        tokio::spawn(async move {
            let policy = TaskPolicy::reasoning(&task).unwrap();
            let grant =
                Grant::narrow(&task, "jev-plus", [Capability::MathEvaluate], 4, &policy).unwrap();
            broker
                .dispatch(
                    &pin,
                    &grant,
                    &policy,
                    "jev-plus",
                    arithmetic(),
                    Instant::now() + Duration::from_secs(10),
                )
                .await
        })
    };
    let first = dispatch(broker.clone(), pin.clone(), task.clone());
    tokio::time::timeout(Duration::from_secs(1), async {
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let second = dispatch(broker.clone(), pin.clone(), task.clone());
    tokio::time::timeout(Duration::from_secs(1), async {
        while broker.status().unwrap()[0].queued == 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let third_policy = TaskPolicy::reasoning(&task).unwrap();
    let third_grant = Grant::narrow(
        &task,
        "jev-plus",
        [Capability::MathEvaluate],
        4,
        &third_policy,
    )
    .unwrap();
    assert!(broker
        .dispatch(
            &pin,
            &third_grant,
            &third_policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_millis(50)
        )
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    first.abort();
    second.abort();
    let _ = first.await;
    let _ = second.await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while dropped.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dropping_the_broker_releases_initialized_module_workers() {
    let store = Arc::new(Store::in_memory().unwrap());
    let task = store
        .create_task("Drop module registry", "standard")
        .unwrap();
    let policy = TaskPolicy::reasoning(&task).unwrap();
    let grant = Grant::narrow(&task, "jev-plus", [Capability::MathEvaluate], 1, &policy).unwrap();
    let implementation = Arc::new(Fixture::new("1.0.0", Arc::new(AtomicUsize::new(0))));
    let weak = Arc::downgrade(&implementation);
    let broker = Broker::new(store);
    broker
        .register(implementation.clone(), Limits::default())
        .unwrap();
    let pin = broker.pin(&task, "fixture.math", "1.0.0").unwrap();
    broker
        .dispatch(
            &pin,
            &grant,
            &policy,
            "jev-plus",
            arithmetic(),
            Instant::now() + Duration::from_secs(1),
        )
        .await
        .unwrap();
    drop(implementation);
    drop(pin);
    drop(broker);
    tokio::time::timeout(Duration::from_secs(1), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Dropped Broker must stop idle workers and release their implementations");
}
