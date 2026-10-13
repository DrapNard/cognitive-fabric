use super::*;
use std::sync::{Arc, Barrier};
fn fill(store: &Store, count: usize) {
    store.lock().unwrap().execute("WITH RECURSIVE n(i) AS (VALUES(0) UNION ALL SELECT i+1 FROM n WHERE i+1<?1) INSERT INTO skills(id,description,tags_json) SELECT printf('fixture.%04d',i),'fixture','[\"test\"]' FROM n",[count as i64]).unwrap();
}
fn manifest(id: &str) -> crate::registry::Manifest {
    serde_json::from_value(json!({"id":id,"version":"1.0.0","kind":"reference","description":"Reviewed fixture","tags":["test"],"dependencies":[],"source":"test fixture","license":"CC0-1.0"})).unwrap()
}
#[test]
fn skill_reads_reject_oversized_or_malformed_metadata_without_repairing_rows() {
    for scenario in 0..13 {
        let store = Store::in_memory().unwrap();
        store
            .ensure_skill("fixture.s1", "fixture", &["test".into()])
            .unwrap();
        let db = store.lock().unwrap();
        match scenario {
            0 => {
                db.execute("UPDATE skills SET id=?1", ["a".repeat(102)])
                    .unwrap();
            }
            1 => {
                db.execute("UPDATE skills SET description=?1", ["x".repeat(501)])
                    .unwrap();
            }
            2 => {
                db.execute("UPDATE skills SET tags_json=?1", [" ".repeat(2049)])
                    .unwrap();
            }
            3 => {
                db.execute("UPDATE skills SET tags_json='not-json'", [])
                    .unwrap();
            }
            4 => {
                db.execute("UPDATE skills SET tags_json='[true]'", [])
                    .unwrap();
            }
            5 => {
                db.execute("UPDATE skills SET tags_json='[\"test\",\"test\"]'", [])
                    .unwrap();
            }
            6 => {
                db.execute("UPDATE skills SET tags_json='[\"Rust\"]'", [])
                    .unwrap();
            }
            7 => {
                db.execute(
                    "UPDATE skills SET tags_json=?1",
                    [serde_json::to_string(
                        &(0..33).map(|i| format!("tag{i}")).collect::<Vec<_>>(),
                    )
                    .unwrap()],
                )
                .unwrap();
            }
            8 => {
                db.execute(
                    "UPDATE skills SET tags_json=?1",
                    [serde_json::to_string(&["a".repeat(41)]).unwrap()],
                )
                .unwrap();
            }
            9 => {
                db.execute("UPDATE skills SET active_version=-1", [])
                    .unwrap();
            }
            10 => {
                db.execute("UPDATE skills SET enabled=2", []).unwrap();
            }
            _ => {
                let version = if scenario == 11 {
                    "1".repeat(65)
                } else {
                    "01.0.0".into()
                };
                db.execute("INSERT INTO expert_manifests(skill_id,version,manifest,checksum,active) VALUES ('fixture.s1',?1,'{}','fixture',1)",[version]).unwrap();
            }
        }
        drop(db);
        assert!(store.skills().is_err(), "scenario {scenario}");
        assert!(
            crate::registry::health(&store).is_err(),
            "Health must reject invalid retained metadata, scenario {scenario}"
        );
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM skills", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            1
        );
    }
    let store = Store::in_memory().unwrap();
    assert!(store.ensure_skill("fixture.s1", "fixture", &[]).is_err());
    store
        .ensure_skill("fixture.s1", "fixture", &["test".into()])
        .unwrap();
    store
        .lock()
        .unwrap()
        .execute("UPDATE skills SET tags_json='[]'", [])
        .unwrap();
    assert!(
        store.skills().unwrap()[0].tags.is_empty(),
        "Valid legacy empty tags remain readable"
    );
    assert_eq!(
        crate::registry::health(&store).unwrap()["retained_skill_ids"],
        1
    );
}
#[test]
fn skill_identity_capacity_is_shared_by_legacy_and_manifest_registration() {
    let store = Store::in_memory().unwrap();
    fill(&store, MAX_SKILLS);
    assert_eq!(store.skills().unwrap().len(), MAX_SKILLS);
    assert!(store
        .ensure_skill("overflow.s1", "fixture", &["test".into()])
        .is_err());
    assert!(crate::registry::register(&store, &manifest("overflow.s1")).is_err());
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM expert_manifests", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM expert_audit", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    store
        .ensure_skill("fixture.0000", "Ignored replacement", &["other".into()])
        .unwrap();
    assert_eq!(store.skills().unwrap()[0].description, "fixture");
    crate::registry::register(&store, &manifest("fixture.0000")).unwrap();
    crate::registry::activate(&store, "fixture.0000", "1.0.0").unwrap();
    assert_eq!(
        store.skills().unwrap()[0].manifest_version.as_deref(),
        Some("1.0.0")
    );
    let overflow = Store::in_memory().unwrap();
    fill(&overflow, MAX_SKILLS + 1);
    assert!(overflow.skills().is_err());
    assert_eq!(
        overflow
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM skills", [], |r| r.get::<_, usize>(0))
            .unwrap(),
        MAX_SKILLS + 1
    );
}
#[test]
fn concurrent_legacy_and_manifest_writers_cannot_exceed_identity_capacity() {
    let root = std::env::temp_dir().join(format!("fabric-skill-capacity-{}", Uuid::new_v4()));
    let cfg = crate::config::Config {
        home: root.clone(),
        ..Default::default()
    };
    cfg.prepare().unwrap();
    let first = Arc::new(Store::open(&cfg.db()).unwrap());
    fill(&first, MAX_SKILLS - 1);
    let second = Arc::new(Store::open(&cfg.db()).unwrap());
    let barrier = Arc::new(Barrier::new(3));
    let a = first.clone();
    let start = barrier.clone();
    let legacy = std::thread::spawn(move || {
        start.wait();
        a.ensure_skill("race.legacy", "fixture", &["test".into()])
            .is_ok()
    });
    let b = second.clone();
    let start = barrier.clone();
    let registered = std::thread::spawn(move || {
        start.wait();
        crate::registry::register(&b, &manifest("race.registered")).is_ok()
    });
    barrier.wait();
    assert_eq!(
        usize::from(legacy.join().unwrap()) + usize::from(registered.join().unwrap()),
        1
    );
    assert_eq!(first.skills().unwrap().len(), MAX_SKILLS);
    drop(first);
    drop(second);
    std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn malformed_skill_snapshot_prevents_model_and_routing_dispatch() {
    use crate::backend::{Generation, InferenceBackend};
    use std::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
    };
    struct Backend(AtomicUsize);
    impl InferenceBackend for Backend {
        fn generate<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { bail!("Unexpected inference") })
        }
    }
    let store = Arc::new(Store::in_memory().unwrap());
    bootstrap(&store).unwrap();
    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE skills SET tags_json='invalid' WHERE id='rust.s1'",
            [],
        )
        .unwrap();
    let backend = Arc::new(Backend(AtomicUsize::new(0)));
    let engine = crate::engine::Engine::with_backend(
        crate::config::Config {
            backend: "openai-local".into(),
            max_tool_calls: 0,
            ..Default::default()
        },
        store.clone(),
        backend.clone(),
    );
    let out = engine.run("Explain Rust borrowing", "fast").await.unwrap();
    assert_eq!(out.status, "failed");
    assert_eq!(out.calls, 0);
    assert_eq!(backend.0.load(Ordering::SeqCst), 0);
    let trace = store.task(&out.task_id).unwrap();
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["kind"] != "routing_policy"));
}
