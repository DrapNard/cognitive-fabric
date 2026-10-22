use super::*;
fn allocation(host: u64) -> Allocation {
    Allocation {
        compute_slots: 1,
        host_memory_bytes: host,
        device_memory_bytes: 0,
    }
}
fn pool(slots: u32, bypasses: u16) -> Arc<ResourcePool> {
    ResourcePool::new(
        "human-cli",
        Arc::new(Store::in_memory().unwrap()),
        Settings {
            label: "gpu.priority".into(),
            capacity: Allocation {
                compute_slots: slots,
                host_memory_bytes: 100,
                device_memory_bytes: 0,
            },
            max_pending: 8,
            max_wait_ms: 5000,
            max_execution_ms: 5000,
            scheduling: Some(PriorityScheduling {
                role_priorities: BTreeMap::from([("solver".into(), 15), ("critic".into(), 1)]),
                max_bypasses: bypasses,
            }),
        },
    )
    .unwrap()
}
fn task(pool: &ResourcePool) -> String {
    pool.store.create_task("Priority fixture", "fast").unwrap()
}
fn waiting(
    pool: Arc<ResourcePool>,
    task: String,
    host: u64,
    priority: u8,
) -> tokio::task::JoinHandle<Result<Lease>> {
    tokio::spawn(async move {
        pool.acquire_priority(
            &task,
            allocation(host),
            uuid::Uuid::new_v4().to_string(),
            priority,
        )
        .await
    })
}
async fn pending(pool: &ResourcePool, n: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while pool.lock().unwrap().queue.len() != n {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn high_priority_runs_first_equal_priority_is_fifo_and_bypass_ceiling_protects_old_work() {
    let p = pool(1, 2);
    let initial = p
        .acquire_priority(
            &task(&p),
            allocation(10),
            uuid::Uuid::new_v4().to_string(),
            0,
        )
        .await
        .unwrap();
    let low_task = task(&p);
    let low = waiting(p.clone(), low_task.clone(), 10, 1);
    pending(&p, 1).await;
    let hi1 = waiting(p.clone(), task(&p), 10, 15);
    pending(&p, 2).await;
    let hi2 = waiting(p.clone(), task(&p), 10, 15);
    pending(&p, 3).await;
    let hi3 = waiting(p.clone(), task(&p), 10, 15);
    pending(&p, 4).await;
    initial.release().unwrap();
    let first = hi1.await.unwrap().unwrap();
    assert!(!low.is_finished());
    assert!(!hi2.is_finished());
    assert_eq!(
        p.snapshot()
            .unwrap()
            .reservations
            .iter()
            .find(|r| r.task_id == low_task)
            .unwrap()
            .queue_bypasses,
        Some(1)
    );
    first.release().unwrap();
    let second = hi2.await.unwrap().unwrap();
    assert!(!low.is_finished());
    assert_eq!(
        p.snapshot()
            .unwrap()
            .reservations
            .iter()
            .find(|r| r.task_id == low_task)
            .unwrap()
            .queue_bypasses,
        Some(2)
    );
    second.release().unwrap();
    let protected = low.await.unwrap().unwrap();
    assert!(!hi3.is_finished());
    protected.release().unwrap();
    hi3.await.unwrap().unwrap().release().unwrap();
    assert!(p.snapshot().unwrap().reservations.is_empty());
}
#[tokio::test]
async fn protected_large_request_blocks_fitting_newcomers_until_capacity_returns() {
    let p = pool(2, 1);
    let initial = p
        .acquire_priority(
            &task(&p),
            allocation(80),
            uuid::Uuid::new_v4().to_string(),
            0,
        )
        .await
        .unwrap();
    let low_task = task(&p);
    let low = waiting(p.clone(), low_task.clone(), 80, 1);
    pending(&p, 1).await;
    let high = waiting(p.clone(), task(&p), 10, 15);
    let high = high.await.unwrap().unwrap();
    assert_eq!(p.snapshot().unwrap().allocated.host_memory_bytes, 90);
    assert_eq!(
        p.snapshot()
            .unwrap()
            .reservations
            .iter()
            .find(|r| r.task_id == low_task)
            .unwrap()
            .queue_bypasses,
        Some(1)
    );
    let next = waiting(p.clone(), task(&p), 10, 15);
    pending(&p, 2).await;
    high.release().unwrap();
    assert_eq!(p.snapshot().unwrap().allocated.host_memory_bytes, 80);
    assert!(!next.is_finished());
    initial.release().unwrap();
    let low = low.await.unwrap().unwrap();
    let next = next.await.unwrap().unwrap();
    assert_eq!(p.snapshot().unwrap().allocated.host_memory_bytes, 90);
    low.release().unwrap();
    next.release().unwrap();
}
#[tokio::test]
async fn cancelled_waiter_does_not_count_as_a_bypass_or_leak_capacity() {
    let p = pool(1, 1);
    let initial = p
        .acquire_priority(
            &task(&p),
            allocation(10),
            uuid::Uuid::new_v4().to_string(),
            0,
        )
        .await
        .unwrap();
    let low_task = task(&p);
    let low = waiting(p.clone(), low_task.clone(), 10, 1);
    pending(&p, 1).await;
    let high = waiting(p.clone(), task(&p), 10, 15);
    pending(&p, 2).await;
    high.abort();
    assert!(high.await.is_err());
    pending(&p, 1).await;
    assert_eq!(
        p.snapshot()
            .unwrap()
            .reservations
            .iter()
            .find(|r| r.task_id == low_task)
            .unwrap()
            .queue_bypasses,
        Some(0)
    );
    initial.release().unwrap();
    low.await.unwrap().unwrap().release().unwrap();
    assert_eq!(p.snapshot().unwrap().allocated, Allocation::default());
}
#[test]
fn validates_operator_policy_preserves_fifo_bytes_and_binds_recovery_audits() {
    let p = pool(1, 2);
    let settings = p.settings.clone();
    for variant in 0..5 {
        let mut s = settings.clone();
        let q = s.scheduling.as_mut().unwrap();
        match variant {
            0 => q.max_bypasses = 0,
            1 => q.max_bypasses = 65,
            2 => {
                q.role_priorities.insert("shell".into(), 1);
            }
            3 => {
                q.role_priorities.insert("solver".into(), 16);
            }
            _ => q.role_priorities.clear(),
        };
        assert!(s.validate().is_err());
    }
    let mut fifo = settings.clone();
    fifo.scheduling = None;
    let bytes = serde_json::to_value(&fifo).unwrap();
    assert!(bytes.get("scheduling").is_none());
    assert_eq!(serde_json::from_value::<Settings>(bytes).unwrap(), fifo);
    assert!(settings
        .validate_queue_audit("solver", &json!({"queue_priority":15}), false)
        .is_ok());
    assert!(settings
        .validate_queue_audit(
            "solver",
            &json!({"queue_priority":15,"queue_bypasses":2}),
            true
        )
        .is_ok());
    for bad in [
        json!({}),
        json!({"queue_priority":1,"queue_bypasses":0}),
        json!({"queue_priority":15,"queue_bypasses":3}),
        json!({"queue_priority":15,"queue_bypasses":null}),
    ] {
        assert!(settings.validate_queue_audit("solver", &bad, true).is_err());
    }
    assert!(fifo
        .validate_queue_audit("solver", &json!({}), true)
        .is_ok());
    assert!(fifo
        .validate_queue_audit("solver", &json!({"queue_priority":null}), true)
        .is_err());
    assert!(ResourcePool::new("jev-plus", p.store.clone(), settings).is_err());
}
#[tokio::test]
async fn actual_role_dispatch_records_priority_before_provider_and_validates_live_policy_identity()
{
    let p = pool(1, 2);
    let task = task(&p);
    let observed = p.store.clone();
    let id = task.clone();
    p.execute(
        &task,
        "solver",
        allocation(10),
        || Ok(None),
        || async move {
            let trace = observed.task(&id)?;
            let events = trace["events"].as_array().unwrap();
            assert!(events.iter().any(|e| e["kind"] == "resource_admitted"
                && e["payload"]["queue_priority"] == 15
                && e["payload"]["queue_bypasses"] == 0));
            Ok(())
        },
    )
    .await
    .unwrap();
    let trace = p.store.task(&task).unwrap();
    let intent = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "resource_intent")
        .unwrap();
    assert_eq!(intent["payload"]["queue_priority"], 15);
    assert!(p.snapshot().unwrap().reservations.is_empty());
    let mut configuration = Configuration {
        settings: p.settings.clone(),
        allocation: allocation(10),
        residents: Vec::new(),
        working_profiles: Vec::new(),
    };
    assert!(p.matches(&p.store, &configuration));
    configuration
        .settings
        .scheduling
        .as_mut()
        .unwrap()
        .role_priorities
        .insert("solver".into(), 14);
    assert!(!p.matches(&p.store, &configuration));
}

#[tokio::test]
async fn real_role_classification_changes_waiting_dispatch_order() {
    let p = pool(1, 2);
    let initial_task = task(&p);
    let p0 = p.clone();
    let entered = Arc::new(Notify::new());
    let finish = Arc::new(tokio::sync::Semaphore::new(0));
    let entered0 = entered.clone();
    let finish0 = finish.clone();
    let running = tokio::spawn(async move {
        p0.execute(
            &initial_task,
            "explorer",
            allocation(10),
            || Ok(None),
            || async move {
                entered0.notify_one();
                finish0.acquire().await.unwrap().forget();
                Ok(())
            },
        )
        .await
    });
    entered.notified().await;
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for (index, role) in ["critic", "solver"].into_iter().enumerate() {
        let worker = p.clone();
        let id = task(&worker);
        let order = order.clone();
        handles.push(tokio::spawn(async move {
            worker
                .execute(
                    &id,
                    role,
                    allocation(10),
                    || Ok(None),
                    || async move {
                        order.lock().unwrap().push(role);
                        Ok(())
                    },
                )
                .await
        }));
        pending(&p, index + 1).await;
    }
    finish.add_permits(1);
    running.await.unwrap().unwrap();
    for handle in handles {
        handle.await.unwrap().unwrap();
    }
    assert_eq!(*order.lock().unwrap(), vec!["solver", "critic"]);
}
#[tokio::test]
async fn priority_audit_denial_never_invokes_provider_or_retains_unstarted_allocation() {
    for kind in ["resource_intent", "resource_admitted", "resource_dispatch"] {
        let p = pool(1, 2);
        let id = task(&p);
        p.store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER deny_priority BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let invoked = std::sync::atomic::AtomicBool::new(false);
        let result = p
            .execute(
                &id,
                "solver",
                allocation(10),
                || Ok(None),
                || async {
                    invoked.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;
        assert!(!invoked.load(std::sync::atomic::Ordering::SeqCst));
        assert!(result.is_err());
        assert!(p.snapshot().unwrap().reservations.is_empty());
        assert_eq!(p.snapshot().unwrap().allocated, Allocation::default());
    }
}
