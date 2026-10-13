use super::*;
fn fixture() -> Manifest {
    serde_json::from_value(json!({"id":"fixture.s1","version":"1.0.0","kind":"reference","description":"Reviewed fixture","tags":["test"],"dependencies":[],"source":"test fixture","license":"CC0-1.0"})).unwrap()
}
#[test]
fn idempotent_registration_verifies_the_stored_body_without_repair_or_audit_success() {
    let store = Store::in_memory().unwrap();
    let manifest = fixture();
    register(&store, &manifest).unwrap();
    let mut altered = manifest.clone();
    altered.description = "Tampered body".into();
    let raw = serde_json::to_string(&altered).unwrap();
    store
        .lock()
        .unwrap()
        .execute("UPDATE expert_manifests SET manifest=?1", [&raw])
        .unwrap();
    assert!(register(&store, &manifest).is_err());
    assert!(activate(&store, &manifest.id, &manifest.version).is_err());
    let db = store.lock().unwrap();
    assert_eq!(
        db.query_row("SELECT manifest FROM expert_manifests", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        raw
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM expert_audit", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT active FROM expert_manifests", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn sql_manifest_field_bounds_protect_snapshot_activation_and_idempotent_reads() {
    for scenario in 0..4 {
        let store = Store::in_memory().unwrap();
        let manifest = fixture();
        register(&store, &manifest).unwrap();
        activate(&store, &manifest.id, &manifest.version).unwrap();
        {
            let db = store.lock().unwrap();
            match scenario {
                0 => {
                    let mut raw = serde_json::to_string(&manifest).unwrap();
                    raw.push_str(&" ".repeat(64001 - raw.len()));
                    db.execute(
                        "UPDATE expert_manifests SET manifest=?1,checksum=?2",
                        params![raw, sha256(raw.as_bytes())],
                    )
                    .unwrap();
                }
                1 => {
                    db.execute("UPDATE expert_manifests SET checksum=?1", ["x".repeat(65)])
                        .unwrap();
                }
                2 => {
                    db.pragma_update(None, "foreign_keys", false).unwrap();
                    let id = "a".repeat(102);
                    db.execute("UPDATE skills SET id=?1", [&id]).unwrap();
                    db.execute("UPDATE expert_manifests SET skill_id=?1", [id])
                        .unwrap();
                }
                _ => {
                    db.execute("UPDATE expert_manifests SET version=?1", ["1".repeat(65)])
                        .unwrap();
                }
            }
            assert!(snapshot_from(&db).is_err());
        }
        assert!(health(&store).is_err());
        assert!(activate(&store, &manifest.id, &manifest.version).is_err());
        if scenario < 2 {
            assert!(register(&store, &manifest).is_err());
        }
        let db = store.lock().unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM expert_audit", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            db.query_row("SELECT sum(active) FROM expert_manifests", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            1
        );
    }
}
#[test]
fn ambiguous_or_excessive_active_registries_fail_without_implicit_promotion_or_truncation() {
    let store = Store::in_memory().unwrap();
    let first = fixture();
    let mut second = first.clone();
    second.version = "2.0.0".into();
    register(&store, &first).unwrap();
    register(&store, &second).unwrap();
    store.lock().unwrap().execute_batch("DROP INDEX expert_active_version; UPDATE expert_manifests SET active=1; UPDATE skills SET enabled=1;").unwrap();
    assert!(snapshot_from(&store.lock().unwrap()).is_err());
    assert!(health(&store).is_err());
    assert!(activate(&store, &first.id, &first.version).is_err());
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row("SELECT sum(active) FROM expert_manifests", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        2
    );
    let store = Store::in_memory().unwrap();
    for index in 0..257 {
        let mut candidate = fixture();
        candidate.id = format!("fixture.{index}");
        register(&store, &candidate).unwrap();
    }
    store
        .lock()
        .unwrap()
        .execute_batch("UPDATE expert_manifests SET active=1; UPDATE skills SET enabled=1;")
        .unwrap();
    assert!(snapshot_from(&store.lock().unwrap()).is_err());
    assert!(health(&store).is_err());
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row("SELECT sum(active) FROM expert_manifests", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        257
    );
}

#[test]
fn active_routing_projection_drift_is_rejected_without_rewriting_data() {
    for tags in [false, true] {
        let store = Store::in_memory().unwrap();
        let value = fixture();
        register(&store, &value).unwrap();
        activate(&store, &value.id, &value.version).unwrap();
        if tags {
            store
                .lock()
                .unwrap()
                .execute("UPDATE skills SET tags_json='[\"other\"]'", [])
                .unwrap();
        } else {
            store
                .lock()
                .unwrap()
                .execute(
                    "UPDATE skills SET description='Different valid description'",
                    [],
                )
                .unwrap();
        }
        assert!(snapshot_from(&store.lock().unwrap()).is_err());
        assert!(store.task_context("test").is_err());
        assert!(health(&store).is_err());
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM expert_audit", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            2
        );
        let skill = store.skills().unwrap().remove(0);
        if tags {
            assert_eq!(skill.tags, vec!["other"]);
        } else {
            assert_eq!(skill.description, "Different valid description");
        }
    }
}
#[test]
fn reviewed_version_activation_updates_projection_and_keeps_previous_snapshots() {
    let store = Store::in_memory().unwrap();
    let first = fixture();
    register(&store, &first).unwrap();
    activate(&store, &first.id, &first.version).unwrap();
    let old = store.task_context("test").unwrap();
    let mut next = first.clone();
    next.version = "2.0.0".into();
    next.description = "Reviewed second version".into();
    next.tags = vec!["other".into()];
    register(&store, &next).unwrap();
    activate(&store, &next.id, &next.version).unwrap();
    let current = store.task_context("other").unwrap();
    assert_eq!(old.skills[0].description, first.description);
    assert_eq!(old.manifests[0].version, "1.0.0");
    assert_eq!(current.skills[0].tags, next.tags);
    assert_eq!(current.manifests[0].version, "2.0.0");
    assert!(health(&store).is_ok());
}
