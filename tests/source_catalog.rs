//! Discovery selects declared catalog metadata, never network or permissions.
use cogfabric::{
    config::Config,
    evolution::{acquisition, catalog},
    store::{sha256, Store},
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

struct Fixture {
    root: PathBuf,
    cfg: Config,
    store: Store,
    task: String,
    path: PathBuf,
    bytes: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("fabric-source-catalog-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let cfg = Config {
            home: root.join("state"),
            backend: "demo".into(),
            ..Default::default()
        };
        cfg.save().unwrap();
        let store = Store::open(&cfg.db()).unwrap();
        let task = store
            .create_task("Missing async reference capability", "standard")
            .unwrap();
        store
            .event(
                &task,
                "capability_gap",
                "jev-plus",
                json!({"missing":["rust.async.s1"]}),
            )
            .unwrap();
        store
            .finish_task(&task, "failed", "Missing capability")
            .unwrap();
        let entries=[("doc.alpha",10,"a","rust.async.s1"),("doc.beta",5,"b","rust.async.s1"),
            ("doc.gamma",6,"b","rust.async.s1"),("doc.delta",0,"d","other.s1"),("doc.epsilon",10,"e","rust.async.s1")]
            .map(|(id,priority,hash,capability)|json!({"id":id,"url":format!("https://docs.network-source.org/{id}.txt"),
                "sha256":hash.repeat(64),"source":format!("reviewed fixture {id}"),"license":"CC0-1.0","capabilities":[capability],"priority":priority}));
        let bytes=serde_json::to_vec(&json!({"format_version":1,"reviewer":"catalog operator","catalog_reviewed":true,"sources":entries})).unwrap();
        let path = root.join("catalog.json");
        std::fs::write(&path, &bytes).unwrap();
        Self {
            root,
            cfg,
            store,
            task,
            path,
            bytes,
        }
    }
    fn hash(&self) -> String {
        sha256(&self.bytes)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn catalog_discovery_is_deterministic_gap_bound_and_requires_separate_acquisition_approval() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let draft =
        catalog::discover(&f.store, &f.task, "rust.async.s1", &f.path, &f.hash(), 2).unwrap();
    let output = f.root.join("draft");
    let report = draft.write(&output).unwrap();
    assert_eq!(report["matched_source_count"], 4);
    assert_eq!(report["duplicate_content_count"], 1);
    assert_eq!(report["selected_ids"], json!(["doc.beta", "doc.alpha"]));
    for flag in [
        "network_used",
        "permissions_granted",
        "candidate_created",
        "evaluation_authority",
        "model_promoted",
    ] {
        assert_eq!(report[flag], false);
    }
    let bytes = std::fs::read(output.join("acquisition-plan.json")).unwrap();
    assert_eq!(report["manifest_sha256"], sha256(&bytes));
    let plan: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(plan["acquisition_approved"], false);
    assert_eq!(plan["reviewer"], "");
    assert!(
        acquisition::acquire(&output.join("acquisition-plan.json"), &sha256(&bytes))
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(output.join("acquisition-plan.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(draft.write(&output).is_err());
    let cli_output = f.root.join("cli-draft");
    let result = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&f.cfg.home)
        .args([
            "--json",
            "candidate",
            "discover-remote-sources",
            &f.task,
            "--capability",
            "rust.async.s1",
            "--catalog",
        ])
        .arg(&f.path)
        .args(["--sha256", &f.hash(), "--max-sources", "2", "--output"])
        .arg(&cli_output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        std::fs::read(cli_output.join("acquisition-plan.json")).unwrap(),
        bytes
    );
    assert!(f.store.candidates().unwrap().is_empty());
    let db = rusqlite::Connection::open(f.cfg.db()).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM events", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn changed_gap_or_newly_available_skill_refuses_publication() {
    for scenario in 0..4 {
        let f = Fixture::new();
        let draft =
            catalog::discover(&f.store, &f.task, "rust.async.s1", &f.path, &f.hash(), 3).unwrap();
        match scenario {
            0 => f
                .store
                .event(
                    &f.task,
                    "capability_gap",
                    "jev-plus",
                    json!({"missing":["rust.async.s1"],"revision":1}),
                )
                .unwrap(),
            1 => f
                .store
                .ensure_skill(
                    "rust.async.s1",
                    "Reuse existing reference",
                    &["rust".into()],
                )
                .unwrap(),
            2 => {
                let db = rusqlite::Connection::open(f.cfg.db()).unwrap();
                db.execute(
                    "UPDATE events SET payload=?1 WHERE task_id=?2",
                    [
                        "{\"missing\":[\"rust.async.s1\"],\"changed\":true}",
                        &f.task,
                    ],
                )
                .unwrap();
            }
            _ => {
                // Even an orphan event left by an external FK-disabled writer
                // cannot keep a deleted task's draft alive.
                let db = rusqlite::Connection::open(f.cfg.db()).unwrap();
                db.execute("DELETE FROM tasks WHERE id=?1", [&f.task])
                    .unwrap();
            }
        }
        let output = f.root.join("refused");
        assert!(draft.write(&output).is_err());
        assert!(!output.exists());
        assert!(f.store.candidates().unwrap().is_empty());
    }
}

#[test]
fn invalid_catalogs_and_unrecorded_capabilities_never_publish() {
    let f = Fixture::new();
    assert!(catalog::discover(&f.store, &f.task, "shell.execute", &f.path, &f.hash(), 2).is_err());
    assert!(catalog::discover(
        &f.store,
        &f.task,
        "rust.async.s1",
        &f.path,
        &"0".repeat(64),
        2
    )
    .is_err());
    for limit in [0, 33] {
        assert!(catalog::discover(
            &f.store,
            &f.task,
            "rust.async.s1",
            &f.path,
            &f.hash(),
            limit
        )
        .is_err());
    }
    let original: Value = serde_json::from_slice(&f.bytes).unwrap();
    for scenario in 0..11 {
        let mut value = original.clone();
        match scenario {
            0 => value["catalog_reviewed"] = false.into(),
            1 => value["sources"][1]["id"] = value["sources"][0]["id"].clone(),
            2 => value["sources"][1]["url"] = value["sources"][0]["url"].clone(),
            3 => value["sources"][0]["capabilities"] = json!(["rust.async.s1", "rust.async.s1"]),
            4 => value["sources"][0]["priority"] = 1001.into(),
            5 => value["sources"][0]["url"] = "https://127.0.0.1/private".into(),
            6 => value["sources"][0]["sha256"] = "A".repeat(64).into(),
            7 => value["grant_tools"] = true.into(),
            8 => value["sources"][3]["url"] = "http://localhost/unmatched-entry".into(),
            9 => value["sources"][0]["source"] = format!("{}label", " ".repeat(100)).into(),
            _ => value["sources"] = json!([]),
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(&f.path, &bytes).unwrap();
        assert!(
            catalog::discover(
                &f.store,
                &f.task,
                "rust.async.s1",
                &f.path,
                &sha256(&bytes),
                2
            )
            .is_err(),
            "accepted invalid case {scenario}"
        );
    }
    assert!(f.store.candidates().unwrap().is_empty());
}

#[test]
fn maximum_catalog_and_selection_are_bounded_and_ignore_input_order() {
    let f = Fixture::new();
    let mut entries=(0..128).map(|index|{
        let id=format!("docs.source{index:03}");
        json!({"id":id,"url":format!("https://docs.network-source.org/{index}.txt"),
            "sha256":sha256(format!("document-{index}").as_bytes()),"source":"reviewed fixture","license":"CC0-1.0",
            "capabilities":["rust.async.s1"],"priority":10})
    }).collect::<Vec<_>>();
    let make = |sources: &[Value]| {
        serde_json::to_vec(&json!({"format_version":1,"reviewer":"catalog operator","catalog_reviewed":true,"sources":sources})).unwrap()
    };
    let first = make(&entries);
    std::fs::write(&f.path, &first).unwrap();
    let output = f.root.join("max-first");
    let report = catalog::discover(
        &f.store,
        &f.task,
        "rust.async.s1",
        &f.path,
        &sha256(&first),
        32,
    )
    .unwrap()
    .write(&output)
    .unwrap();
    assert_eq!(report["catalog_source_count"], 128);
    assert_eq!(report["selected_source_count"], 32);
    assert_eq!(report["selected_ids"][0], "docs.source000");
    assert_eq!(report["selected_ids"][31], "docs.source031");
    entries.reverse();
    let reversed = make(&entries);
    std::fs::write(&f.path, &reversed).unwrap();
    let second = f.root.join("max-reversed");
    catalog::discover(
        &f.store,
        &f.task,
        "rust.async.s1",
        &f.path,
        &sha256(&reversed),
        32,
    )
    .unwrap()
    .write(&second)
    .unwrap();
    assert_eq!(
        std::fs::read(output.join("acquisition-plan.json")).unwrap(),
        std::fs::read(second.join("acquisition-plan.json")).unwrap()
    );
    let mut extra = entries[0].clone();
    extra["id"] = "docs.extra".into();
    extra["url"] = "https://docs.network-source.org/extra.txt".into();
    entries.push(extra);
    let excess = make(&entries);
    std::fs::write(&f.path, &excess).unwrap();
    assert!(catalog::discover(
        &f.store,
        &f.task,
        "rust.async.s1",
        &f.path,
        &sha256(&excess),
        32
    )
    .is_err());
}

#[test]
fn symlink_catalog_and_output_are_refused_without_writes() {
    let f = Fixture::new();
    let link = f.root.join("catalog-link.json");
    std::os::unix::fs::symlink(&f.path, &link).unwrap();
    assert!(catalog::discover(&f.store, &f.task, "rust.async.s1", &link, &f.hash(), 2).is_err());
    let draft =
        catalog::discover(&f.store, &f.task, "rust.async.s1", &f.path, &f.hash(), 2).unwrap();
    let target = f.root.join("target");
    std::fs::create_dir(&target).unwrap();
    let output = f.root.join("output-link");
    std::os::unix::fs::symlink(&target, &output).unwrap();
    assert!(draft.write(&output).is_err());
    assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
}
