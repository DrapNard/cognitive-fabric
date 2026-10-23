use super::*;

fn manifest(id: &str, version: &str) -> Manifest {
    Manifest {
        id: id.into(),
        version: version.into(),
        kind: "reference".into(),
        checker: None,
        description: "Reviewed dependency graph expert".into(),
        tags: vec!["test".into()],
        capabilities: Vec::new(),
        dependencies: Vec::new(),
        source: "fixture".into(),
        license: "CC0-1.0".into(),
        workflow: None,
        workflow_steps: None,
        model: None,
        provider: None,
    }
}

fn active_version(store: &Store, id: &str) -> Option<String> {
    store
        .skills()
        .unwrap()
        .into_iter()
        .find(|skill| skill.id == id && skill.enabled)
        .and_then(|skill| skill.manifest_version)
}

#[test]
fn coordinated_version_swap_validates_the_whole_dependency_graph_atomically() {
    let store = Store::in_memory().unwrap();
    let base_v1 = manifest("expert.base.s1", "1.0.0");
    let mut child_v1 = manifest("expert.child.s1", "1.0.0");
    child_v1.dependencies.push(Dependency {
        id: base_v1.id.clone(),
        requirement: "^1.0".into(),
    });
    for candidate in [&base_v1, &child_v1] {
        register(&store, candidate).unwrap();
        activate(&store, &candidate.id, &candidate.version).unwrap();
    }

    let mut base_v2 = manifest(&base_v1.id, "2.0.0");
    base_v2.description = "Reviewed base version two".into();
    let mut child_v2 = manifest(&child_v1.id, "2.0.0");
    child_v2.description = "Reviewed dependent version two".into();
    child_v2.dependencies.push(Dependency {
        id: base_v2.id.clone(),
        requirement: "^2.0".into(),
    });
    register(&store, &base_v2).unwrap();
    register(&store, &child_v2).unwrap();

    // The base cannot move alone while the enabled dependent still requires v1.
    assert!(activate(&store, &base_v2.id, &base_v2.version).is_err());
    assert_eq!(
        active_version(&store, &base_v1.id).as_deref(),
        Some("1.0.0")
    );
    assert_eq!(
        active_version(&store, &child_v1.id).as_deref(),
        Some("1.0.0")
    );
    assert!(disable(&store, &base_v1.id).is_err());

    let changes = [
        ActivationChange {
            id: child_v2.id.clone(),
            version: child_v2.version.clone(),
        },
        ActivationChange {
            id: base_v2.id.clone(),
            version: base_v2.version.clone(),
        },
    ];
    let plan = activation_plan(&store, &changes).unwrap();
    assert_eq!(plan["valid"], true);
    assert_eq!(plan["permissions_granted"], false);
    assert_eq!(plan["code_executed"], false);
    assert_eq!(plan["proposed_active"].as_array().unwrap().len(), 2);
    assert!(plan["proposed_active"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["manifest_sha256"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64)));

    let digest = plan["registry_sha256"].as_str().unwrap().to_owned();
    let applied = apply_activation_plan(&store, &changes, &digest).unwrap();
    assert_eq!(applied["active"], true);
    assert_eq!(applied["changed"], true);
    assert_eq!(
        active_version(&store, &base_v1.id).as_deref(),
        Some("2.0.0")
    );
    assert_eq!(
        active_version(&store, &child_v1.id).as_deref(),
        Some("2.0.0")
    );
    assert_eq!(
        health(&store).unwrap()["active"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn activation_rejects_stale_plans_and_tampered_candidates_without_partial_changes() {
    let store = Store::in_memory().unwrap();
    let first_v1 = manifest("expert.first.s1", "1.0.0");
    let mut first_v2 = manifest(&first_v1.id, "2.0.0");
    first_v2.description = "Staged version two".into();
    let second = manifest("expert.second.s1", "1.0.0");
    register(&store, &first_v1).unwrap();
    activate(&store, &first_v1.id, &first_v1.version).unwrap();
    register(&store, &first_v2).unwrap();
    register(&store, &second).unwrap();

    let change = [ActivationChange {
        id: first_v2.id.clone(),
        version: first_v2.version.clone(),
    }];
    let stale = activation_plan(&store, &change).unwrap();
    activate(&store, &second.id, &second.version).unwrap();
    assert!(
        apply_activation_plan(&store, &change, stale["registry_sha256"].as_str().unwrap()).is_err()
    );
    assert_eq!(
        active_version(&store, &first_v1.id).as_deref(),
        Some("1.0.0")
    );

    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE expert_manifests SET checksum='0000000000000000000000000000000000000000000000000000000000000000' WHERE skill_id=?1 AND version=?2",
            params![first_v2.id, first_v2.version],
        )
        .unwrap();
    assert!(activation_plan(&store, &change).is_err());
    assert_eq!(
        active_version(&store, &first_v1.id).as_deref(),
        Some("1.0.0")
    );
}

#[test]
fn activation_audit_failure_rolls_back_every_version_in_a_graph_swap() {
    let store = Store::in_memory().unwrap();
    let base_v1 = manifest("expert.atomic.base", "1.0.0");
    let mut child_v1 = manifest("expert.atomic.child", "1.0.0");
    child_v1.dependencies.push(Dependency {
        id: base_v1.id.clone(),
        requirement: "^1".into(),
    });
    for candidate in [&base_v1, &child_v1] {
        register(&store, candidate).unwrap();
        activate(&store, &candidate.id, &candidate.version).unwrap();
    }
    let base_v2 = manifest(&base_v1.id, "2.0.0");
    let mut child_v2 = manifest(&child_v1.id, "2.0.0");
    child_v2.dependencies.push(Dependency {
        id: base_v2.id.clone(),
        requirement: "^2".into(),
    });
    register(&store, &base_v2).unwrap();
    register(&store, &child_v2).unwrap();
    let changes = [
        ActivationChange {
            id: base_v2.id.clone(),
            version: base_v2.version.clone(),
        },
        ActivationChange {
            id: child_v2.id.clone(),
            version: child_v2.version.clone(),
        },
    ];
    let plan = activation_plan(&store, &changes).unwrap();
    store.lock().unwrap().execute_batch(
        "CREATE TRIGGER reject_child_activation BEFORE INSERT ON expert_audit WHEN NEW.action='activate' AND NEW.skill_id='expert.atomic.child' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;",
    ).unwrap();

    assert!(
        apply_activation_plan(&store, &changes, plan["registry_sha256"].as_str().unwrap()).is_err()
    );
    assert_eq!(
        active_version(&store, &base_v1.id).as_deref(),
        Some("1.0.0")
    );
    assert_eq!(
        active_version(&store, &child_v1.id).as_deref(),
        Some("1.0.0")
    );
}

#[test]
fn activation_bounds_the_requested_set_and_preserves_legacy_manifest_bytes() {
    let changes = (0..257)
        .map(|index| ActivationChange {
            id: format!("expert.{index}"),
            version: "1.0.0".into(),
        })
        .collect::<Vec<_>>();
    let store = Store::in_memory().unwrap();
    assert!(activation_plan(&store, &changes).is_err());

    let legacy: Manifest = serde_json::from_str(include_str!(
        "../../tests/fixtures/legacy_checker_manifest.json"
    ))
    .unwrap();
    assert_eq!(
        sha256(&serde_json::to_vec(&legacy).unwrap()),
        "e43b6ec09af5f9be056f0f6b27c971439b42a0857eaa0bee812d107c874ecc91"
    );
    assert!(!serde_json::to_string(&legacy).unwrap().contains("provider"));
}
