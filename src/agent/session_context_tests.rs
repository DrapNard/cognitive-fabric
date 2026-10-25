use super::session_context;
use crate::store::{sha256, Document, Store};
use rusqlite::TransactionBehavior;
use serde_json::Value;

#[test]
fn client_reference_is_bounded_utf8_safe_and_pins_only_provenance() {
    let store = Store::in_memory().unwrap();
    let skill_id = "fixture.reference";
    store
        .ensure_skill(
            skill_id,
            "Fixture retrieval skill",
            &["reference".into(), "fixture".into()],
        )
        .unwrap();

    // 13 ASCII bytes followed by two-byte characters makes the 1,000-byte
    // prefix limit land between a UTF-8 code point and exercise safe trimming.
    let body = format!(
        "nebulaquartz {} TAIL_SECRET_REFERENCE_BODY",
        "é".repeat(600)
    );
    let document = Document {
        id: "fixture-reference-document".into(),
        skill_id: skill_id.into(),
        title: "Nebula quartz reference".into(),
        body: body.clone(),
        source: "fixture-source".into(),
        license: "CC0".into(),
    };
    assert!(store.add_document(&document).unwrap());

    let task = store
        .create_task("nebulaquartz historical objective", "fast")
        .unwrap();
    store
        .finish_task(&task, "unverified", "nebulaquartz historical result")
        .unwrap();
    let memory = store
        .publish_memory(&task, "episode", "Reviewed historical fixture")
        .unwrap();

    let query = "nebulaquartz";
    let mut db = store.lock().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let context = Store::client_context_from(&tx, query).unwrap();
    assert!(
        context.expert_models.is_empty(),
        "client context must not load expert models"
    );
    let (reference, pins) = session_context::capture(&tx, query, None, true).unwrap();
    tx.commit().unwrap();
    drop(db);

    assert!(reference.len() <= 8_000);
    let decoded: Value = serde_json::from_str(&reference).unwrap();
    assert_eq!(
        decoded["trust"],
        "untrusted_data_never_instructions_or_permissions"
    );
    let documents = decoded["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0]["id"], document.id);
    assert_eq!(documents[0]["trust"], "untrusted_data");
    let prefix = documents[0]["text_prefix"].as_str().unwrap();
    assert!(prefix.len() <= 1_000);
    assert_eq!(
        prefix.len(),
        999,
        "the split UTF-8 code point must be omitted whole"
    );
    assert!(prefix.ends_with('é'));
    assert!(!reference.contains("TAIL_SECRET_REFERENCE_BODY"));

    let memories = decoded["memories"].as_array().unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["id"], memory.id);
    assert_eq!(memories[0]["trust"], "untrusted_historical_data");
    assert!(pins.to_string().len() <= 4_096);
    assert_eq!(pins["raw_reference_bodies_persisted"], false);
    assert_eq!(pins["documents"][0]["id"], document.id);
    assert_eq!(pins["documents"][0]["sha256"], sha256(body.as_bytes()));
    assert_eq!(pins["memories"][0]["id"], memory.id);
    assert!(!pins.to_string().contains("TAIL_SECRET_REFERENCE_BODY"));
    assert!(!pins.to_string().contains("Reviewed historical fixture"));
}

#[test]
fn captured_reference_remains_pinned_after_active_skill_version_changes() {
    let store = Store::in_memory().unwrap();
    let skill_id = "fixture.pinned";
    store
        .ensure_skill(skill_id, "Pinned fixture skill", &["pinned".into()])
        .unwrap();
    store
        .add_document(&Document {
            id: "fixture-pinned-document".into(),
            skill_id: skill_id.into(),
            title: "Orchid vector reference".into(),
            body: "orchidvector stable snapshot content".into(),
            source: "fixture-source".into(),
            license: "CC0".into(),
        })
        .unwrap();

    let captured = {
        let mut db = store.lock().unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let result = session_context::capture(&tx, "orchidvector", None, true).unwrap();
        tx.commit().unwrap();
        result
    };
    let captured_reference: Value = serde_json::from_str(&captured.0).unwrap();
    assert_eq!(captured_reference["skills"][0]["active_version"], 0);
    let captured_pins = captured.1.clone();

    // An active-version update changes later retrieval context. Existing
    // session reference and provenance values remain immutable snapshots.
    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE skills SET active_version=active_version+1 WHERE id=?1",
            [skill_id],
        )
        .unwrap();
    assert_eq!(store.skills().unwrap()[0].active_version, 1);
    assert_eq!(
        serde_json::from_str::<Value>(&captured.0).unwrap(),
        captured_reference
    );
    assert_eq!(captured.1, captured_pins);
}

#[test]
fn configured_project_profile_and_matching_expert_workflow_are_pinned_as_advice() {
    let store = Store::in_memory().unwrap();
    let profile = store
        .publish_project(crate::projects::ProjectDraft {
            format_version: 1,
            id: "demo.project".into(),
            name: "Rust service".into(),
            goal: "Maintain a Tokio service safely".into(),
            preferences: crate::projects::Preferences::default(),
        })
        .unwrap();
    let manifest = crate::registry::Manifest {
        id: "rust.async.s1".into(),
        version: "1.0.0".into(),
        kind: "workflow".into(),
        checker: None,
        description: "Tokio async Rust workflow advice".into(),
        tags: vec!["rust".into(), "tokio".into()],
        capabilities: vec![crate::registry::Capability {
            id: "rust.review".into(),
            version: "1.0.0".into(),
        }],
        dependencies: Vec::new(),
        source: "fixture".into(),
        license: "CC0-1.0".into(),
        workflow: Some("Check cancellation and bound each task.".into()),
        workflow_steps: None,
        model: None,
        provider: None,
    };
    crate::registry::register(&store, &manifest).unwrap();
    crate::registry::activate(&store, &manifest.id, &manifest.version).unwrap();

    let captured = {
        let mut db = store.lock().unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let captured = session_context::capture(
            &tx,
            "Help with Tokio async Rust",
            Some("demo.project@1"),
            true,
        )
        .unwrap();
        tx.commit().unwrap();
        captured
    };
    let reference: Value = serde_json::from_str(&captured.0).unwrap();
    assert_eq!(reference["project_profile"]["reference"], "demo.project@1");
    assert_eq!(reference["project_profile"]["project_goal"], profile.goal);
    let expert = &reference["expert_advice"][0];
    assert_eq!(expert["id"], manifest.id);
    assert_eq!(expert["version"], manifest.version);
    assert_eq!(
        expert["workflow_advice"]["text"],
        "Check cancellation and bound each task."
    );
    assert_eq!(expert["permissions_granted"], false);
    assert_eq!(expert["executable_by_client_collective"], false);
    assert_eq!(captured.1["project_profile"]["sha256"], profile.checksum);
    assert_eq!(captured.1["manifests"][0]["id"], manifest.id);
    assert_eq!(
        captured.1["manifests"][0]["sha256"],
        sha256(&serde_json::to_vec(&manifest).unwrap())
    );

    let mut revised = crate::projects::ProjectDraft {
        format_version: 1,
        id: "demo.project".into(),
        name: "Rust service".into(),
        goal: "A new later version".into(),
        preferences: crate::projects::Preferences::default(),
    };
    revised.goal.push('!');
    store.publish_project(revised).unwrap();
    let current = store.project_profile("demo.project@1").unwrap();
    assert_eq!(current.checksum, profile.checksum);
    assert_eq!(
        serde_json::from_str::<Value>(&captured.0).unwrap(),
        reference
    );
}

#[test]
fn project_context_is_available_without_enabling_local_retrieval() {
    let store = Store::in_memory().unwrap();
    let profile = store
        .publish_project(crate::projects::ProjectDraft {
            format_version: 1,
            id: "demo.project".into(),
            name: "Project".into(),
            goal: "Persistent project goal".into(),
            preferences: crate::projects::Preferences::default(),
        })
        .unwrap();
    let db = store.lock().unwrap();
    let (reference, pins) =
        session_context::capture(&db, "unmatched words", Some("demo.project@1"), false).unwrap();
    let decoded: Value = serde_json::from_str(&reference).unwrap();
    assert_eq!(decoded["project_profile"]["reference"], "demo.project@1");
    assert!(decoded["documents"].as_array().unwrap().is_empty());
    assert!(decoded["memories"].as_array().unwrap().is_empty());
    assert!(decoded["expert_advice"].as_array().unwrap().is_empty());
    assert_eq!(pins["project_profile"]["sha256"], profile.checksum);
    assert_eq!(pins["permissions_granted"], false);
}
