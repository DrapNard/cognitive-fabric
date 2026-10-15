//! Public authored retrieval fixtures cannot attest independent acceptance.
use cogfabric::{
    config::Config,
    evaluation, evolution,
    store::{self, Store},
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftCase {
    query: String,
    expected_excerpt: String,
}
#[test]
fn rust_reference_stays_quarantined_without_independent_quality_review() {
    let root = std::env::temp_dir().join(format!("fabric-rust-reference-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let artifact = root.join("reference.md");
    let dataset = root.join("draft.jsonl");
    let body = include_str!("../examples/rust_ownership_reference.md");
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../examples/rust_ownership_reference_manifest.json"
    ))
    .unwrap();
    let license = manifest["artifact_license"].as_str().unwrap();
    assert_eq!(license, env!("CARGO_PKG_LICENSE"));
    assert_eq!(manifest["artifact_sha256"], store::sha256(body.as_bytes()));
    assert_eq!(
        manifest["quality_cases_sha256"],
        store::sha256(include_bytes!(
            "../examples/rust_ownership_retrieval_draft.jsonl"
        ))
    );
    std::fs::write(&artifact, body).unwrap();
    std::fs::write(
        &dataset,
        include_str!("../examples/rust_ownership_retrieval_draft.jsonl"),
    )
    .unwrap();
    let cfg = Config {
        home: root.clone(),
        ..Config::default()
    };
    let store = Store::in_memory().unwrap();
    store
        .ensure_skill("rust.s1", "Rust reference fixture", &["rust".into()])
        .unwrap();
    let candidate = evolution::create(
        &cfg,
        &store,
        "rust.s1",
        "rag",
        &artifact,
        "Original Rust reference fixture",
        license,
    )
    .unwrap();
    assert_eq!(candidate.checksum, store::sha256(body.as_bytes()));
    assert_eq!(
        evaluation::evaluate(&store, &candidate.id, "integrity", None).unwrap()["checksum_matches"],
        true
    );
    let review = evaluation::DatasetReview {
        source: "Same-author public draft".into(),
        license: license.into(),
        held_out: false,
    };
    assert!(evaluation::evaluate_reviewed(
        &store,
        &candidate.id,
        "quality",
        Some(&dataset),
        Some(&review)
    )
    .is_err());
    assert!(evaluation::promote(&store, &candidate.id, false).is_err());
    assert!(evaluation::promote(&store, &candidate.id, true).is_err());
    assert_eq!(store.candidate(&candidate.id).unwrap().status, "candidate");
    assert!(store
        .search("String ownership", Some("rust.s1"), 1)
        .unwrap()
        .is_empty());
    // Separate mechanical retrieval fixture: no promotion or acceptance attestation.
    let staging = Store::in_memory().unwrap();
    staging
        .ensure_skill("rust.s1", "Isolated retrieval", &["rust".into()])
        .unwrap();
    staging
        .add_document(&store::Document {
            id: "fixture".into(),
            skill_id: "rust.s1".into(),
            title: "Rust reference".into(),
            body: body.into(),
            source: "Original linked Rust guide".into(),
            license: license.into(),
        })
        .unwrap();
    for line in include_str!("../examples/rust_ownership_retrieval_draft.jsonl").lines() {
        let case: DraftCase = serde_json::from_str(line).unwrap();
        let hits = staging.search(&case.query, Some("rust.s1"), 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].body.contains(&case.expected_excerpt));
    }
    std::fs::remove_dir_all(root).unwrap();
}
