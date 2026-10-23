use cogfabric::{
    config::Config,
    evaluation,
    evolution::{preparation, prepared},
    store::{sha256, Store},
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};
fn fixture() -> (PathBuf, Config, Store, String) {
    let root =
        std::env::temp_dir().join(format!("fabric-prepared-import-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let mut sources = Vec::new();
    for (id,split,body) in [
        ("reference","reference","Reference ownership facts.\n\n## Source validation\n\nThis embedded heading is source data, not a framing boundary."),
        ("validation","validation","Validation material stays outside the candidate artifact."),
        ("test","test","Unseen test material stays outside the candidate artifact."),
    ] {
        let path=format!("{id}.md");std::fs::write(root.join(&path),body).unwrap();
        sources.push(json!({"id":id,"path":path,"sha256":sha256(body.as_bytes()),"source":format!("authored-fixture:{id}"),"license":"GPL-3.0-only","split":split}));
    }
    let raw=serde_json::to_vec(&json!({"format_version":1,"reviewer":"test operator","sources_reviewed":true,"sources":sources})).unwrap();
    let path = root.join("sources.json");
    std::fs::write(&path, &raw).unwrap();
    let bundle = root.join("bundle");
    preparation::prepare(&root, &path, &sha256(&raw))
        .unwrap()
        .write(&bundle)
        .unwrap();
    let hash = sha256(&std::fs::read(bundle.join("report.json")).unwrap());
    let cfg = Config {
        home: root.join("state"),
        backend: "demo".into(),
        ..Default::default()
    };
    cfg.save().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    store
        .ensure_skill("rust.s1", "Prepared RAG fixture", &["rust".into()])
        .unwrap();
    (root, cfg, store, hash)
}
#[test]
fn cli_import_retains_provenance_without_evaluation_or_promotion_authority() {
    let (root, cfg, store, hash) = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&cfg.home)
        .args([
            "--json",
            "candidate",
            "create-prepared",
            "--skill",
            "rust.s1",
            "--bundle",
        ])
        .arg(root.join("bundle"))
        .arg("--report-sha256")
        .arg(&hash)
        .args(["--license", "GPL-3.0-only"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let candidate: Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = candidate["id"].as_str().unwrap();
    assert_eq!(candidate["status"], "candidate");
    assert_eq!(candidate["kind"], "rag");
    let provenance = prepared::provenance(&store, id).unwrap();
    assert_eq!(provenance["report_sha256"], hash);
    assert_eq!(provenance["evaluation_authority"], false);
    assert_eq!(
        provenance["prepared_provenance"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let bytes = std::fs::read(candidate["artifact"].as_str().unwrap()).unwrap();
    let body = std::str::from_utf8(&bytes).unwrap();
    assert!(body.contains("embedded heading"));
    assert!(!body.contains("Unseen test material"));
    assert_eq!(sha256(&bytes), candidate["checksum"]);
    let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&cfg.home)
        .args(["--json", "candidate", "provenance", id])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        provenance
    );
    assert!(evaluation::promote(&store, id, true).is_err());
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM evaluations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let anchor = provenance["provenance_task_id"].as_str().unwrap();
    db.execute("UPDATE tasks SET updated=0 WHERE id=?1", [anchor])
        .unwrap();
    assert!(!store
        .retention_preview(1, 100)
        .unwrap()
        .task_ids
        .contains(&anchor.to_owned()));
    db.execute("UPDATE events SET payload=json_set(payload,'$.report_sha256',?1) WHERE kind='candidate_preparation'",["0".repeat(64)]).unwrap();
    assert!(prepared::provenance(&store, id).is_err());
    drop(db);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn changed_or_inconsistent_prepared_bundles_never_create_candidates() {
    use std::os::unix::fs::symlink;
    for scenario in 0..15 {
        let (root, cfg, store, mut hash) = fixture();
        let bundle = root.join("bundle");
        let mut report: Value =
            serde_json::from_slice(&std::fs::read(bundle.join("report.json")).unwrap()).unwrap();
        match scenario {
            0 => hash = "0".repeat(64),
            1 => std::fs::write(bundle.join("artifact.md"), "changed artifact").unwrap(),
            2 => std::fs::write(bundle.join("held-out.json"), "{}").unwrap(),
            3 => report["format_version"] = json!(1),
            4 => report["permissions_granted"] = json!(true),
            5 => report["independent_review_proven"] = json!(true),
            6 => report["sources"][0]["normalized_bytes"] = json!(1),
            7 => report["sources"][0]["normalized_sha256"] = json!("0".repeat(64)),
            8 => report["sources"][2]["split"] = json!("reference"),
            9 => report["artifact_bytes"] = json!(1),
            10 => report["sources"][0]["duplicate_of"] = json!("missing-source"),
            11 => report["held_out_documents"] = json!(3),
            12 => report["candidate_created"] = json!(true),
            13 => report["grant_shell"] = json!(true),
            14 => {
                std::fs::rename(bundle.join("artifact.md"), bundle.join("real.md")).unwrap();
                symlink(bundle.join("real.md"), bundle.join("artifact.md")).unwrap();
            }
            _ => unreachable!(),
        }
        if (3..=13).contains(&scenario) {
            let raw = serde_json::to_vec(&report).unwrap();
            std::fs::write(bundle.join("report.json"), &raw).unwrap();
            hash = sha256(&raw);
        }
        assert!(
            prepared::create(&cfg, &store, "rust.s1", &bundle, &hash, "GPL-3.0-only").is_err(),
            "scenario {scenario}"
        );
        assert!(store.candidates().unwrap().is_empty());
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM events WHERE kind='candidate_preparation'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        drop(db);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn provenance_audit_failure_rolls_back_candidate_version_and_anchor_atomically() {
    let (root, cfg, store, hash) = fixture();
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    db.execute_batch("CREATE TRIGGER deny_prepared BEFORE INSERT ON events WHEN NEW.kind='candidate_preparation' BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
    let before: i64 = db
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert!(prepared::create(
        &cfg,
        &store,
        "rust.s1",
        &root.join("bundle"),
        &hash,
        "GPL-3.0-only"
    )
    .is_err());
    assert!(store.candidates().unwrap().is_empty());
    assert_eq!(
        db.query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        before
    );
    db.execute_batch("DROP TRIGGER deny_prepared").unwrap();
    let candidate = prepared::create(
        &cfg,
        &store,
        "rust.s1",
        &root.join("bundle"),
        &hash,
        "GPL-3.0-only",
    )
    .unwrap();
    assert_eq!(candidate.version, 1);
    prepared::provenance(&store, &candidate.id).unwrap();
    drop(db);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
