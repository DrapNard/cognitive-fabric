use super::*;
fn models() -> Vec<Model> {
    parse(br#"{"models":[{"key":"a","type":"llm","loaded_instances":[{"id":"a-existing","config":{"context_length":2048}}]},{"key":"b","type":"llm","loaded_instances":[]},{"key":"c","type":"llm","loaded_instances":[{"id":"c-old","config":{"context_length":1024}}]}]}"#).unwrap()
}
#[test]
fn planner_reuses_compatible_instances_and_never_invents_unloads() {
    let request = Request {
        ensure_loaded: vec![
            EnsureLoaded {
                model: "b".into(),
                context_length: 4096,
            },
            EnsureLoaded {
                model: "a".into(),
                context_length: 2048,
            },
        ],
        release_instances: vec![],
    };
    let models = models();
    let plan = build(
        "http://localhost:1234/api/v1",
        &request,
        &models,
        now().unwrap(),
    )
    .unwrap();
    plan.validate().unwrap();
    assert_eq!(plan.reused[0].instance_id, "a-existing");
    assert_eq!(
        plan.actions,
        vec![Mutation::Load {
            model: "b".into(),
            context_length: 4096
        }]
    );
    let mut reordered = models.clone();
    reordered.reverse();
    let mut request = request;
    request.ensure_loaded.reverse();
    assert_eq!(
        build(
            "http://127.0.0.1:1234/api/v1/",
            &request,
            &reordered,
            plan.created_unix
        )
        .unwrap(),
        plan
    );
}
#[test]
fn incompatible_context_requires_explicit_release_and_unloads_precede_loads() {
    let mut request = Request {
        ensure_loaded: vec![EnsureLoaded {
            model: "a".into(),
            context_length: 4096,
        }],
        release_instances: vec![],
    };
    assert!(build(
        "http://localhost:1234/api/v1",
        &request,
        &models(),
        now().unwrap()
    )
    .is_err());
    request.release_instances.push("a-existing".into());
    let plan = build(
        "http://localhost:1234/api/v1",
        &request,
        &models(),
        now().unwrap(),
    )
    .unwrap();
    assert_eq!(
        plan.actions,
        vec![
            Mutation::Unload {
                instance_id: "a-existing".into()
            },
            Mutation::Load {
                model: "a".into(),
                context_length: 4096
            }
        ]
    );
}
#[test]
fn malformed_duplicate_missing_and_tampered_plans_are_rejected() {
    for request in [
        Request {
            ensure_loaded: vec![],
            release_instances: vec![],
        },
        Request {
            ensure_loaded: vec![EnsureLoaded {
                model: "missing".into(),
                context_length: 2048,
            }],
            release_instances: vec![],
        },
        Request {
            ensure_loaded: vec![
                EnsureLoaded {
                    model: "a".into(),
                    context_length: 2048
                };
                2
            ],
            release_instances: vec![],
        },
        Request {
            ensure_loaded: vec![EnsureLoaded {
                model: "a".into(),
                context_length: 2048,
            }],
            release_instances: vec!["unknown".into()],
        },
    ] {
        assert!(build(
            "http://localhost:1234/api/v1",
            &request,
            &models(),
            now().unwrap()
        )
        .is_err());
    }
    let request = Request {
        ensure_loaded: vec![EnsureLoaded {
            model: "a".into(),
            context_length: 2048,
        }],
        release_instances: vec![],
    };
    let mut plan = build(
        "http://localhost:1234/api/v1",
        &request,
        &models(),
        now().unwrap(),
    )
    .unwrap();
    plan.actions.push(Mutation::Unload {
        instance_id: "c-old".into(),
    });
    assert!(plan.validate().is_err());
}

#[test]
fn plans_expire_and_reject_future_timestamps_or_unbounded_requests() {
    let request = Request {
        ensure_loaded: vec![EnsureLoaded {
            model: "a".into(),
            context_length: 2048,
        }],
        release_instances: vec![],
    };
    let plan = build(
        "http://localhost:1234/api/v1",
        &request,
        &models(),
        now().unwrap(),
    )
    .unwrap();
    for created in [0, now().unwrap() + 100] {
        let mut stale = plan.clone();
        stale.created_unix = created;
        stale.digest = stale.checksum().unwrap();
        assert!(stale.validate().is_err());
    }
    for context in [0, 32769] {
        let mut request = request.clone();
        request.ensure_loaded[0].context_length = context;
        assert!(canonical_request(&request).is_err());
    }
    let mut request = request;
    request.release_instances = vec!["instance".into(); 9];
    assert!(canonical_request(&request).is_err());
}
#[test]
fn corrupt_or_replayed_receipts_cannot_initialize_a_second_plan_task() {
    for forged in [false, true] {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("Receipt fixture", "fast").unwrap();
        let request = Request {
            ensure_loaded: vec![EnsureLoaded {
                model: "a".into(),
                context_length: 2048,
            }],
            release_instances: vec![],
        };
        let plan = build(
            "http://localhost:1234/api/v1",
            &request,
            &models(),
            now().unwrap(),
        )
        .unwrap();
        store
            .event(
                &task,
                "residency_plan",
                if forged { "forged" } else { "human-cli" },
                json!({"format_version":1,"state":"requested","digest":plan.digest}),
            )
            .unwrap();
        assert!(claim(&store, &plan).is_err());
        let count: u64 = store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
