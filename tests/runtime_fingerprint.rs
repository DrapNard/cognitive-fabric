#[path = "../build_support/fingerprint.rs"]
mod fingerprint;
use std::{fs, os::unix::fs::symlink};

#[test]
fn runtime_fingerprint_is_path_independent_and_changes_with_execution_inputs() {
    let root = std::env::temp_dir().join(format!("cogfabric-source-hash-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    for dir in ["src", "build_support", "protocol"] {
        fs::create_dir(root.join(dir)).unwrap();
    }
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/lib.rs",
        "protocol/frame.fbs",
    ] {
        fs::write(root.join(file), file).unwrap();
    }
    let original = fingerprint::calculate(&root).unwrap();
    assert_eq!(original.len(), 64);
    fs::write(
        root.join("src/notes.md"),
        "untrusted text is not Rust source",
    )
    .unwrap();
    assert_eq!(original, fingerprint::calculate(&root).unwrap());
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/lib.rs",
        "protocol/frame.fbs",
    ] {
        fs::write(root.join(file), "changed").unwrap();
        assert_ne!(
            original,
            fingerprint::calculate(&root).unwrap(),
            "Ignored {file}"
        );
        fs::write(root.join(file), file).unwrap();
    }
    fs::write(root.join("src/new.rs"), "new module").unwrap();
    assert_ne!(original, fingerprint::calculate(&root).unwrap());
    fs::remove_file(root.join("src/new.rs")).unwrap();
    symlink(root.join("src/lib.rs"), root.join("src/link.rs")).unwrap();
    assert!(fingerprint::calculate(&root).is_err());
    fs::remove_file(root.join("src/link.rs")).unwrap();
    let moved = root.with_extension("moved");
    fs::rename(&root, &moved).unwrap();
    assert_eq!(original, fingerprint::calculate(&moved).unwrap());
    fs::remove_dir_all(moved).unwrap();
}

#[test]
fn compiled_source_fingerprint_matches_current_checkout() {
    assert_eq!(
        cogfabric::escalation::empirical::runtime_sources_fingerprint(),
        fingerprint::calculate(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
    );
}
