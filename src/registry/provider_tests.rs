use super::*;
use crate::{
    fabric::modules::{
        Broker, Capability as ModuleCapability, Descriptor, Grant, Limits, Module, ModuleFuture,
        Request, Response,
    },
    security::TaskPolicy,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn provider_manifest(expert_version: &str, module_version: &str) -> Manifest {
    Manifest {
        id: "expert.math.s1".into(),
        version: expert_version.into(),
        kind: "workflow".into(),
        checker: None,
        description: format!("Reviewed math workflow {expert_version}"),
        tags: vec!["math".into()],
        capabilities: vec![Capability {
            id: "math.evaluate".into(),
            version: "1.0.0".into(),
        }],
        dependencies: Vec::new(),
        source: "test fixture".into(),
        license: "CC0-1.0".into(),
        workflow: Some("Use the typed arithmetic provider".into()),
        workflow_steps: None,
        model: None,
        provider: Some(ProviderBinding {
            module_id: "typed.math".into(),
            module_version: module_version.into(),
            capabilities: vec![ProviderCapability::MathEvaluate],
        }),
    }
}

fn descriptor(
    version: &str,
    capabilities: impl IntoIterator<Item = ModuleCapability>,
) -> Descriptor {
    Descriptor {
        id: "typed.math".into(),
        version: version.into(),
        capabilities: capabilities.into_iter().collect(),
    }
}

struct FixedModule {
    descriptor: Descriptor,
    value: String,
}

impl Module for FixedModule {
    fn descriptor(&self) -> Descriptor {
        self.descriptor.clone()
    }

    fn execute<'a>(
        &'a self,
        _task: &'a str,
        _deadline: Instant,
        request: Request,
    ) -> ModuleFuture<'a> {
        Box::pin(async move {
            match request {
                Request::Arithmetic { .. } => Ok(Response::Arithmetic {
                    value: self.value.clone(),
                }),
                _ => bail!("Unexpected fixture request"),
            }
        })
    }
}

fn register_module(broker: &Broker, version: &str, value: &str) -> Result<()> {
    register_module_with_capabilities(broker, version, value, [ModuleCapability::MathEvaluate])
}

fn register_module_with_capabilities(
    broker: &Broker,
    version: &str,
    value: &str,
    capabilities: impl IntoIterator<Item = ModuleCapability>,
) -> Result<()> {
    broker.register(
        Arc::new(FixedModule {
            descriptor: descriptor(version, capabilities),
            value: value.into(),
        }),
        Limits::default(),
    )
}

fn add_task_snapshot(
    store: &Store,
    task: &str,
    manifest: &Manifest,
    broker: &Broker,
    provider_descriptor: &Descriptor,
) -> Result<()> {
    store.event(
        task,
        "typed_module_snapshot",
        "fabric",
        json!({
            "format_version":1,
            "broker_id":broker.identity_id().to_string(),
            "modules":[{
                "id":provider_descriptor.id,
                "version":provider_descriptor.version,
                "capabilities":provider_descriptor.capabilities.iter().map(|capability| capability.as_str()).collect::<Vec<_>>()
            }],
            "permissions_granted":false
        }),
    )?;
    add_route_snapshot(store, task, manifest)
}

fn add_route_snapshot(store: &Store, task: &str, manifest: &Manifest) -> Result<()> {
    let skill = store
        .skills()?
        .into_iter()
        .find(|skill| skill.id == manifest.id)
        .context("Fixture expert skill is missing")?;
    store.event(
        task,
        "route",
        "jev-plus",
        json!({"skills":[manifest.id],"versions":[skill],"manifests":[manifest]}),
    )?;
    Ok(())
}

async fn dispatch_arithmetic(broker: &Broker, pin: &crate::fabric::modules::Pin) -> Result<String> {
    let policy = TaskPolicy::reasoning(pin.task())?;
    let grant = Grant::narrow(
        pin.task(),
        "jev-plus",
        [ModuleCapability::MathEvaluate],
        1,
        &policy,
    )?;
    let response = broker
        .dispatch(
            pin,
            &grant,
            &policy,
            "jev-plus",
            Request::Arithmetic {
                expression: "1+1".into(),
            },
            Instant::now() + Duration::from_secs(3),
        )
        .await?;
    let Response::Arithmetic { value } = response else {
        bail!("Fixture module returned an unexpected result")
    };
    Ok(value)
}

#[tokio::test]
async fn provider_binding_pins_exact_version_and_does_not_create_task_authority() {
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest = provider_manifest("1.0.0", "1.0.0");
    register(&store, &manifest).unwrap();
    activate(&store, &manifest.id, &manifest.version).unwrap();
    let task = store
        .create_task("Typed provider route", "standard")
        .unwrap();
    let broker = Broker::new(store.clone());
    register_module(&broker, "1.0.0", "pinned-v1").unwrap();

    let pin = providers::pin_provider(
        &store,
        &manifest,
        &broker,
        &task,
        ProviderCapability::MathEvaluate,
    )
    .unwrap();
    assert_eq!(pin.task(), task);
    assert_eq!(pin.descriptor().id, "typed.math");
    assert_eq!(pin.descriptor().version, "1.0.0");
    assert_eq!(
        store.task(&task).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "expert_provider_snapshot")
            .count(),
        1
    );

    // Binding and pinning do not broaden the independent policy or create a grant.
    let restricted_policy = TaskPolicy::agent(&task).unwrap();
    assert!(Grant::narrow(
        &task,
        "jev-plus",
        [ModuleCapability::MathEvaluate],
        1,
        &restricted_policy
    )
    .is_err());
    assert_eq!(
        dispatch_arithmetic(&broker, &pin).await.unwrap(),
        "pinned-v1"
    );

    assert!(providers::pin_provider(
        &store,
        &manifest,
        &broker,
        &task,
        ProviderCapability::SqlQuery
    )
    .is_err());
    let dispatches: i64 = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE task_id=?1 AND kind='module_dispatch_intent'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dispatches, 1);
}

#[tokio::test]
async fn task_snapshot_provider_pin_survives_expert_and_module_hot_swap() {
    let store = Arc::new(Store::in_memory().unwrap());
    let old_manifest = provider_manifest("1.0.0", "1.0.0");
    register(&store, &old_manifest).unwrap();
    activate(&store, &old_manifest.id, &old_manifest.version).unwrap();
    let task = store
        .create_task("Keep admitted provider snapshot", "standard")
        .unwrap();
    let broker = Broker::new(store.clone());
    register_module(&broker, "1.0.0", "old-module-version").unwrap();
    let admitted_pin = broker.pin(&task, "typed.math", "1.0.0").unwrap();
    add_task_snapshot(
        &store,
        &task,
        &old_manifest,
        &broker,
        admitted_pin.descriptor(),
    )
    .unwrap();

    let new_manifest = provider_manifest("2.0.0", "2.0.0");
    register(&store, &new_manifest).unwrap();
    activate(&store, &new_manifest.id, &new_manifest.version).unwrap();
    register_module(&broker, "2.0.0", "new-module-version").unwrap();

    let task_pin = providers::pin_task_snapshot_provider(
        &store,
        &old_manifest,
        &broker,
        &admitted_pin,
        ProviderCapability::MathEvaluate,
    )
    .unwrap();
    assert_eq!(task_pin.descriptor().version, "1.0.0");
    assert_eq!(
        dispatch_arithmetic(&broker, &task_pin).await.unwrap(),
        "old-module-version"
    );
    assert!(store.task(&task).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "expert_provider_snapshot"
            && event["payload"]["expert_version"] == "1.0.0"
            && event["payload"]["module_version"] == "1.0.0"));
}

#[test]
fn task_snapshot_provider_rejects_same_descriptor_pin_from_another_broker() {
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest = provider_manifest("1.0.0", "1.0.0");
    register(&store, &manifest).unwrap();
    activate(&store, &manifest.id, &manifest.version).unwrap();
    let task = store
        .create_task("Reject broker substitution", "standard")
        .unwrap();

    let admitted_broker = Broker::new(store.clone());
    register_module(&admitted_broker, "1.0.0", "admitted implementation").unwrap();
    let admitted_pin = admitted_broker.pin(&task, "typed.math", "1.0.0").unwrap();
    add_task_snapshot(
        &store,
        &task,
        &manifest,
        &admitted_broker,
        admitted_pin.descriptor(),
    )
    .unwrap();

    let replacement_broker = Broker::new(store.clone());
    register_module(&replacement_broker, "1.0.0", "replacement implementation").unwrap();
    let replacement_pin = replacement_broker
        .pin(&task, "typed.math", "1.0.0")
        .unwrap();
    assert_eq!(
        replacement_pin.descriptor(),
        admitted_pin.descriptor(),
        "the registry descriptors intentionally collide"
    );
    assert!(!admitted_broker.owns_pin(&replacement_pin));
    assert!(providers::pin_task_snapshot_provider(
        &store,
        &manifest,
        &admitted_broker,
        &replacement_pin,
        ProviderCapability::MathEvaluate
    )
    .is_err());

    let snapshots: i64 = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE task_id=?1 AND kind='expert_provider_snapshot'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(snapshots, 0);
}

#[test]
fn provider_binding_rejects_inexact_module_version_or_capability_set() {
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest = provider_manifest("1.0.0", "1.0.0");
    register(&store, &manifest).unwrap();
    activate(&store, &manifest.id, &manifest.version).unwrap();
    let task = store
        .create_task("Reject inexact provider", "standard")
        .unwrap();

    let broker = Broker::new(store.clone());
    register_module_with_capabilities(
        &broker,
        "1.0.0",
        "wrong-capability-set",
        [ModuleCapability::MathEvaluate, ModuleCapability::SqlQuery],
    )
    .unwrap();
    // A descriptor with extra capabilities is rejected even though it can handle
    // the required request; the immutable expert binding is exact.
    assert!(providers::pin_provider(
        &store,
        &manifest,
        &broker,
        &task,
        ProviderCapability::MathEvaluate
    )
    .is_err());
    broker.deactivate("typed.math").unwrap();
    register_module(&broker, "1.1.0", "wrong-version").unwrap();
    assert!(providers::pin_provider(
        &store,
        &manifest,
        &broker,
        &task,
        ProviderCapability::MathEvaluate
    )
    .is_err());
    broker.deactivate("typed.math").unwrap();
    assert!(providers::pin_provider(
        &store,
        &manifest,
        &broker,
        &task,
        ProviderCapability::MathEvaluate
    )
    .is_err());
    let intents: i64 = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE task_id=?1 AND kind='module_dispatch_intent'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(intents, 0);
}

#[test]
fn task_snapshot_provider_pin_fails_closed_on_missing_duplicate_or_tampered_audits() {
    for scenario in 0..10 {
        let store = Arc::new(Store::in_memory().unwrap());
        let manifest = provider_manifest("1.0.0", "1.0.0");
        register(&store, &manifest).unwrap();
        activate(&store, &manifest.id, &manifest.version).unwrap();
        let task = store
            .create_task("Reject bad provider snapshot", "standard")
            .unwrap();
        let broker = Broker::new(store.clone());
        register_module(&broker, "1.0.0", "v1").unwrap();
        let pin = broker.pin(&task, "typed.math", "1.0.0").unwrap();
        let desc = pin.descriptor().clone();

        match scenario {
            0 => {
                store
                    .event(
                        &task,
                        "typed_module_snapshot",
                        "fabric",
                        json!({"format_version":1,"modules":[{"id":desc.id,"version":desc.version,"capabilities":["math.evaluate"]}],"permissions_granted":false}),
                    )
                    .unwrap();
            }
            6 => add_route_snapshot(&store, &task, &manifest).unwrap(),
            _ => add_task_snapshot(&store, &task, &manifest, &broker, &desc).unwrap(),
        }
        match scenario {
            1 => store
                .event(
                    &task,
                    "route",
                    "jev-plus",
                    json!({"skills":[manifest.id],"versions":[],"manifests":[]}),
                )
                .unwrap(),
            2 => store
                .event(
                    &task,
                    "route",
                    "jev-plus",
                    json!({"skills":[],"versions":[],"manifests":[]}),
                )
                .unwrap(),
            3 => store
                .event(
                    &task,
                    "typed_module_snapshot",
                    "fabric",
                    json!({"format_version":1,"broker_id":broker.identity_id().to_string(),"modules":[{"id":"typed.math","version":"1.0.0","capabilities":["sql.query"]}],"permissions_granted":false}),
                )
                .unwrap(),
            4 => {
                store
                    .lock()
                    .unwrap()
                    .execute(
                        "UPDATE events SET payload='{}' WHERE task_id=?1 AND kind='route'",
                        [&task],
                    )
                    .unwrap();
            }
            5 => {
                store
                    .lock()
                    .unwrap()
                    .execute(
                        "UPDATE expert_manifests SET checksum=?3 WHERE skill_id=?1 AND version=?2",
                        params![manifest.id, manifest.version, "0".repeat(64)],
                    )
                    .unwrap();
            }
            0 | 6 => {}
            7 => {
                store
                    .lock()
                    .unwrap()
                    .execute(
                        "UPDATE events SET payload=?2 WHERE task_id=?1 AND kind='typed_module_snapshot'",
                        params![task, json!({"format_version":1,"broker_id":broker.identity_id().to_string(),"modules":[{"id":"typed.math","version":"1.0.0","capabilities":["math.evaluate","sql.query"]}],"permissions_granted":false}).to_string()],
                    )
                    .unwrap();
            }
            8 => {
                store
                    .lock()
                    .unwrap()
                    .execute(
                        "UPDATE events SET payload=?2 WHERE task_id=?1 AND kind='typed_module_snapshot'",
                        params![task, json!({"format_version":1,"broker_id":broker.identity_id().to_string(),"modules":[{"id":"typed.math","version":"1.0.0","capabilities":["math.evaluate"]}],"permissions_granted":true}).to_string()],
                    )
                    .unwrap();
            }
            9 => {
                store.lock().unwrap().execute(
                    "UPDATE events SET payload=json_set(payload,'$.broker_id',?2) WHERE task_id=?1 AND kind='typed_module_snapshot'",
                    params![task, uuid::Uuid::new_v4().to_string()],
                ).unwrap();
            }
            _ => unreachable!("All task snapshot corruption cases are enumerated"),
        }
        assert!(providers::pin_task_snapshot_provider(
            &store,
            &manifest,
            &broker,
            &pin,
            ProviderCapability::MathEvaluate
        )
        .is_err());
        let snapshots: i64 = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM events WHERE task_id=?1 AND kind='expert_provider_snapshot'",
                [&task],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(snapshots, 0, "scenario {scenario}");
    }
}

#[test]
fn provider_snapshot_audit_failure_returns_no_pin() {
    let store = Arc::new(Store::in_memory().unwrap());
    let manifest = provider_manifest("1.0.0", "1.0.0");
    register(&store, &manifest).unwrap();
    activate(&store, &manifest.id, &manifest.version).unwrap();
    let task = store
        .create_task("Provider audit failure", "standard")
        .unwrap();
    let broker = Broker::new(store.clone());
    register_module(&broker, "1.0.0", "v1").unwrap();
    store
        .lock()
        .unwrap()
        .execute_batch("CREATE TRIGGER reject_provider_snapshot BEFORE INSERT ON events WHEN NEW.kind='expert_provider_snapshot' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")
        .unwrap();

    assert!(providers::pin_provider(
        &store,
        &manifest,
        &broker,
        &task,
        ProviderCapability::MathEvaluate
    )
    .is_err());
    let intents: i64 = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE task_id=?1 AND kind='module_dispatch_intent'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(intents, 0);
}
