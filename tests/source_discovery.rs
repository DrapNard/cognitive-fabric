#![cfg(target_os = "linux")]
use cogfabric::{
    evolution::{discovery, preparation},
    store::sha256,
};
use serde_json::Value;
use std::{path::PathBuf, process::Command};
fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("fabric-discovery-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("input")).unwrap();
    root
}
#[test]
fn discovery_is_deterministic_private_and_requires_review_before_preparation() {
    use std::os::unix::fs::PermissionsExt;
    let root = root();
    let input = root.join("input");
    std::fs::create_dir(input.join("nested")).unwrap();
    for (path, text) in [
        (
            "z.md",
            "A separate test source about independent verification.",
        ),
        (
            "nested/b.txt",
            "A separate validation source about ownership boundaries.",
        ),
        (
            "a.md",
            "Reference documentation about bounded orchestration.",
        ),
    ] {
        std::fs::write(input.join(path), text).unwrap();
    }
    std::fs::write(
        input.join(".private.md"),
        "hidden source must not be inventoried",
    )
    .unwrap();
    std::fs::write(input.join("image.png"), [0xff, 0xfe]).unwrap();
    let output = root.join("draft");
    let home = root.join("unused-home");
    let command = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .arg("--json")
        .args(["candidate", "discover-sources", "--root"])
        .arg(&input)
        .args([
            "--source",
            "authored-fixture",
            "--license",
            "GPL-3.0-only",
            "--output",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        command.status.success(),
        "{}",
        String::from_utf8_lossy(&command.stderr)
    );
    let report: Value = serde_json::from_slice(&command.stdout).unwrap();
    assert_eq!(report["sources"], 3);
    assert_eq!(report["hidden_entries_skipped"], 1);
    assert_eq!(report["sources_reviewed"], false);
    assert_eq!(report["network_used"], false);
    assert!(!home.exists());
    let raw = std::fs::read(output.join("sources.json")).unwrap();
    assert_eq!(sha256(&raw), report["manifest_sha256"]);
    assert_eq!(
        std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let draft = discovery::discover(&input, "authored-fixture", "GPL-3.0-only").unwrap();
    let other = root.join("again");
    draft.write(&other).unwrap();
    assert_eq!(std::fs::read(other.join("sources.json")).unwrap(), raw);
    assert!(draft.write(&output).is_err());
    assert!(preparation::prepare(
        &input,
        &output.join("sources.json"),
        report["manifest_sha256"].as_str().unwrap()
    )
    .is_err());
    let mut manifest: Value = serde_json::from_slice(&raw).unwrap();
    manifest["reviewer"] = "test operator".into();
    manifest["sources_reviewed"] = true.into();
    manifest["sources"][1]["split"] = "validation".into();
    manifest["sources"][2]["split"] = "test".into();
    let reviewed = serde_json::to_vec(&manifest).unwrap();
    let path = root.join("reviewed.json");
    std::fs::write(&path, &reviewed).unwrap();
    let prepared = preparation::prepare(&input, &path, &sha256(&reviewed)).unwrap();
    let bundle = root.join("prepared");
    let prepared_report = prepared.write(&bundle).unwrap();
    assert_eq!(prepared_report["candidate_created"], false);
    let body = std::fs::read_to_string(bundle.join("artifact.md")).unwrap();
    assert!(body.contains("Reference documentation"));
    assert!(!body.contains("separate test"));
    assert!(!body.contains("separate validation"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn discovery_refuses_unsafe_entries_or_exhausted_bounds_without_a_partial_draft() {
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::symlink},
    };
    for scenario in 0..9 {
        let root = root();
        let input = root.join("input");
        std::fs::write(
            input.join("reference.md"),
            "A valid reference document before adversarial fixture setup.",
        )
        .unwrap();
        match scenario {
            0 => symlink(input.join("reference.md"), input.join("link.md")).unwrap(),
            1 => symlink(&input, input.join("linked-directory")).unwrap(),
            2 => {
                let name = CString::new(input.join("pipe.md").as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            3 => std::fs::write(input.join("big.md"), vec![b'x'; 64_001]).unwrap(),
            4 => {
                for n in 0..33 {
                    std::fs::write(input.join(format!("{n}.md")), format!("Document {n}")).unwrap();
                }
            }
            5 => {
                for n in 0..257 {
                    std::fs::write(input.join(format!("{n}.json")), "{}").unwrap();
                }
            }
            6 => {
                let nested = input.join("a/b/c/d/e/f/g/h");
                std::fs::create_dir_all(&nested).unwrap();
                std::fs::write(nested.join("too-deep.md"), "deep").unwrap();
            }
            7 => std::fs::write(input.join("bad.md"), [0xff, 0xfe]).unwrap(),
            8 => {
                std::fs::remove_file(input.join("reference.md")).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            discovery::discover(&input, "authored-fixture", "GPL-3.0-only").is_err(),
            "scenario {scenario}"
        );
        assert!(!root.join("draft").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
