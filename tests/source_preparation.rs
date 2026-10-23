use cogfabric::{evolution::preparation, store::sha256};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn fixture() -> (PathBuf, Value) {
    let root = std::env::temp_dir().join(format!("fabric-source-prepare-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let mut sources = Vec::new();
    for (id, split, body) in [
        (
            "reference",
            "reference",
            "Reference facts about ownership and bounded lifetimes.  \r\n",
        ),
        (
            "validation",
            "validation",
            "Validation question withheld from the reference document.",
        ),
        (
            "test",
            "test",
            "Independent test fixture content withheld from the reference document.",
        ),
    ] {
        let path = format!("{id}.md");
        std::fs::write(root.join(&path), body).unwrap();
        sources.push(json!({"id":id,"path":path,"sha256":sha256(body.as_bytes()),"source":format!("authored-fixture:{id}"),"license":"GPL-3.0-only","split":split}));
    }
    (
        root,
        json!({"format_version":1,"reviewer":"test operator","sources_reviewed":true,"sources":sources}),
    )
}
fn manifest(root: &std::path::Path, value: &Value) -> (PathBuf, String) {
    let path = root.join("manifest.json");
    let bytes = serde_json::to_vec(value).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    (path, sha256(&bytes))
}

#[test]
fn cli_prepares_private_split_bundle_without_initializing_runtime_or_promoting() {
    use std::os::unix::fs::PermissionsExt;
    let (root, mut value) = fixture();
    let duplicate = value["sources"][0].clone();
    let mut duplicate = duplicate;
    duplicate["id"] = json!("reference.duplicate");
    value["sources"].as_array_mut().unwrap().push(duplicate);
    let (path, hash) = manifest(&root, &value);
    let output = root.join("prepared");
    let home = root.join("unused-home");
    let result = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("--json")
        .args(["candidate", "prepare-sources", "--root"])
        .arg(&root)
        .arg("--manifest")
        .arg(&path)
        .arg("--sha256")
        .arg(&hash)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["candidate_created"], false);
    assert_eq!(report["model_promoted"], false);
    assert_eq!(report["permissions_granted"], false);
    assert_eq!(report["independent_review_proven"], false);
    assert_eq!(report["sources"][3]["duplicate_of"], "reference");
    assert!(!home.exists());
    let artifact = std::fs::read_to_string(output.join("artifact.md")).unwrap();
    assert_eq!(artifact.matches("Reference facts").count(), 1);
    assert!(!artifact.contains("Validation question"));
    assert!(!artifact.contains("Independent test"));
    assert!(!artifact.contains('\r'));
    assert_eq!(sha256(artifact.as_bytes()), report["artifact_sha256"]);
    assert_eq!(
        std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(output.join("artifact.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let prepared = preparation::prepare(&root, &path, &hash).unwrap();
    assert!(prepared.write(&output).is_err());
    assert_eq!(
        std::fs::read_to_string(output.join("artifact.md")).unwrap(),
        artifact
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_hash_review_split_overlap_traversal_and_symlink_failures_before_output() {
    use std::os::unix::fs::symlink;
    for scenario in 0..16 {
        let (root, mut value) = fixture();
        match scenario {
            0 => value["sources_reviewed"] = json!(false),
            1 => value["sources"][0]["sha256"] = json!("0".repeat(64)),
            2 => value["sources"][0]["path"] = json!("../reference.md"),
            3 => {
                std::fs::rename(root.join("reference.md"), root.join("real.md")).unwrap();
                symlink(root.join("real.md"), root.join("reference.md")).unwrap();
            }
            4 => {
                symlink(&root, root.join("linked")).unwrap();
                value["sources"][0]["path"] = json!("linked/reference.md");
            }
            5 => value["sources"][2] = value["sources"][0].clone(),
            6 => {
                let bytes = std::fs::read(root.join("reference.md")).unwrap();
                std::fs::write(root.join("test.md"), &bytes).unwrap();
                value["sources"][2]["sha256"] = json!(sha256(&bytes));
            }
            7 => value["sources"][0]["license"] = json!("unknown"),
            8 => value["sources"][2]["split"] = json!("reference"),
            9 => value["permissions_granted"] = json!(true),
            10 => {
                let text="Reference facts about ownership and bounded lifetimes.\n\nA separate test question.";
                std::fs::write(root.join("test.md"), text).unwrap();
                value["sources"][2]["sha256"] = json!(sha256(text.as_bytes()));
            }
            11 => {
                let bytes = vec![b'x'; 64_001];
                std::fs::write(root.join("reference.md"), &bytes).unwrap();
                value["sources"][0]["sha256"] = json!(sha256(&bytes));
            }
            12 => {
                value["sources"][0]["path"] = json!(root.join("reference.md").to_str().unwrap());
            }
            13 => {
                use std::{ffi::CString, os::unix::ffi::OsStrExt};
                std::fs::remove_file(root.join("reference.md")).unwrap();
                let path = CString::new(root.join("reference.md").as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            14 => {
                let bytes = b"unsafe\x1bcontrol";
                std::fs::write(root.join("reference.md"), bytes).unwrap();
                value["sources"][0]["sha256"] = json!(sha256(bytes));
            }
            15 => {
                let bytes = [0xff, 0xfe];
                std::fs::write(root.join("reference.md"), bytes).unwrap();
                value["sources"][0]["sha256"] = json!(sha256(&bytes));
            }
            _ => unreachable!(),
        }
        let (path, hash) = manifest(&root, &value);
        assert!(
            preparation::prepare(&root, &path, &hash).is_err(),
            "scenario {scenario}"
        );
        assert!(!root.join("prepared").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn source_changes_after_manifest_and_manifest_hash_changes_are_refused() {
    let (root, value) = fixture();
    let (path, hash) = manifest(&root, &value);
    assert!(preparation::prepare(&root, &path, &"0".repeat(64)).is_err());
    std::fs::write(root.join("reference.md"), "Changed reference bytes").unwrap();
    assert!(preparation::prepare(&root, &path, &hash).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
