use super::*;
fn fixture() -> Manifest {
    serde_json::from_value(json!({"id":"reviewed.sort.s1","version":"1.0.0","kind":"reference","description":"Reviewed sorting references","tags":["test"],"source":"fixture","license":"CC0","capabilities":[{"id":"domain.sort","version":"2.1.0"}]})).unwrap()
}
#[test]
fn capability_validation_preserves_legacy_canonical_hashes() {
    let legacy: Manifest = serde_json::from_str(include_str!(
        "../../tests/fixtures/legacy_checker_manifest.json"
    ))
    .unwrap();
    assert_eq!(legacy.license, "Apache-2.0");
    // Current project examples may evolve; compatibility evidence must stay frozen.
    let current: Manifest =
        serde_json::from_str(include_str!("../../examples/checker_manifest.json")).unwrap();
    current.validate().unwrap();
    assert_eq!(current.license, env!("CARGO_PKG_LICENSE"));
    assert!(legacy.capabilities.is_empty());
    assert!(serde_json::to_value(&legacy)
        .unwrap()
        .get("capabilities")
        .is_none());
    assert_eq!(
        sha256(&serde_json::to_vec(&legacy).unwrap()),
        "e43b6ec09af5f9be056f0f6b27c971439b42a0857eaa0bee812d107c874ecc91"
    );
    for (id, version) in [
        ("reviewed.sort.s1", "1.0.0"),
        ("invalid/id", "1.0.0"),
        ("domain.sort", "01.0.0"),
        ("domain.sort", ""),
    ] {
        let mut bad = fixture();
        bad.capabilities = vec![Capability {
            id: id.into(),
            version: version.into(),
        }];
        assert!(bad.validate().is_err());
    }
    let mut bad = fixture();
    bad.capabilities.push(bad.capabilities[0].clone());
    assert!(bad.validate().is_err());
    bad.capabilities = (0..17)
        .map(|i| Capability {
            id: format!("domain.cap{i}"),
            version: "1.0.0".into(),
        })
        .collect();
    assert!(bad.validate().is_err());
}
#[test]
fn versioned_lookup_follows_active_versions_without_mutating_pinned_declarations() {
    let store = Store::in_memory().unwrap();
    let first = fixture();
    store
        .ensure_skill("domain.sort", "Legacy category", &["test".into()])
        .unwrap();
    register(&store, &first).unwrap();
    activate(&store, &first.id, &first.version).unwrap();
    let old = store.task_context("test").unwrap();
    assert_eq!(
        lookup(&store, "domain.sort", "^2").unwrap()["matches"][0]["expert_id"],
        first.id
    );
    assert_eq!(
        lookup(&store, &first.id, "^1").unwrap()["matches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        lookup(&store, "domain.sort", "^3").unwrap()["matches"],
        json!([])
    );
    let mut next = first.clone();
    next.version = "2.0.0".into();
    next.capabilities[0].version = "3.0.0".into();
    register(&store, &next).unwrap();
    activate(&store, &next.id, &next.version).unwrap();
    assert_eq!(old.manifests[0].capabilities[0].version, "2.1.0");
    assert_eq!(
        provider(&old.skills, &old.manifests, "domain.sort"),
        Some(first.id.clone())
    );
    assert_eq!(
        lookup(&store, "domain.sort", "^2").unwrap()["matches"],
        json!([])
    );
    assert_eq!(
        lookup(&store, "domain.sort", "^3").unwrap()["matches"][0]["expert_version"],
        "2.0.0"
    );
    disable(&store, &next.id).unwrap();
    assert_eq!(
        lookup(&store, "domain.sort", "*").unwrap()["matches"],
        json!([])
    );
    assert!(lookup(&store, "domain.sort", &"x".repeat(65)).is_err());
    assert!(security::authorize("jev-plus", "skill.inspect").is_err());
    assert!(security::authorize("jev-plus", "shell.execute").is_err());
}
#[test]
fn recorded_gap_preparation_observes_aliases_without_creating_permissions_or_candidates() {
    let store = Store::in_memory().unwrap();
    let task = store
        .create_task("Missing declaration", "standard")
        .unwrap();
    store
        .event(
            &task,
            "capability_gap",
            "jev-plus",
            json!({"missing":["domain.sort"]}),
        )
        .unwrap();
    assert!(
        !crate::gaps::prepare(&store, &task, "domain.sort")
            .unwrap()
            .available_now
    );
    let manifest = fixture();
    register(&store, &manifest).unwrap();
    activate(&store, &manifest.id, &manifest.version).unwrap();
    let prepared = crate::gaps::prepare(&store, &task, "domain.sort").unwrap();
    assert!(prepared.available_now);
    assert!(!prepared.permissions_granted);
    assert!(!prepared.candidate_created);
    disable(&store, &manifest.id).unwrap();
    assert!(
        !crate::gaps::prepare(&store, &task, "domain.sort")
            .unwrap()
            .available_now
    );
}
