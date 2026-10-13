#[path = "../build_support/protocol.rs"]
mod protocol;
use std::{
    fs,
    path::{Path, PathBuf},
};
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture(root: &Path) -> Scratch {
    let scratch = Scratch(
        std::env::temp_dir().join(format!("fabric-protocol-bundle-{}", uuid::Uuid::new_v4())),
    );
    fs::create_dir_all(scratch.0.join("protocol/generated")).unwrap();
    for schema in protocol::SCHEMAS {
        fs::copy(
            root.join(format!("protocol/{schema}.fbs")),
            scratch.0.join(format!("protocol/{schema}.fbs")),
        )
        .unwrap();
        let name = protocol::generated_name(schema);
        fs::copy(
            root.join("protocol/generated").join(&name),
            scratch.0.join("protocol/generated").join(name),
        )
        .unwrap();
    }
    fs::copy(
        root.join("protocol/generated/manifest.sha256"),
        scratch.0.join("protocol/generated/manifest.sha256"),
    )
    .unwrap();
    scratch
}
#[test]
fn reviewed_bundle_matches_every_schema_and_the_exact_rust_runtime_pin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = protocol::validate(root).unwrap();
    assert_eq!(files.len(), 9);
    assert!(protocol::runtime_pinned(include_str!("../Cargo.toml")));
    assert!(protocol::runtime_pinned(
        r#"[dependencies.flatbuffers]
version = "=25.12.19"
"#
    ));
    assert!(!protocol::runtime_pinned(
        r#"[dependencies.flatbuffers]
version = "25.12.19"
"#
    ));
    assert!(!protocol::runtime_pinned(
        r#"[dev-dependencies.flatbuffers]
version = "=25.12.19"
"#
    ));
    for file in files {
        assert!(fs::metadata(file).unwrap().len() > 0);
    }
}
#[test]
fn stale_schema_generated_output_and_manifest_are_rejected_without_repair() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = fixture(root);
    protocol::validate(&scratch.0).unwrap();
    for relative in [
        "protocol/agent.fbs",
        "protocol/generated/agent_generated.rs",
    ] {
        let path = scratch.0.join(relative);
        let original = fs::read(&path).unwrap();
        let mut stale = original.clone();
        stale.extend_from_slice(b"\n// stale fixture\n");
        fs::write(&path, &stale).unwrap();
        assert!(protocol::validate(&scratch.0)
            .unwrap_err()
            .contains("checksum mismatch"));
        assert_eq!(fs::read(&path).unwrap(), stale);
        fs::write(path, original).unwrap();
    }
    let path = scratch.0.join("protocol/generated/manifest.sha256");
    let original = fs::read_to_string(&path).unwrap();
    let duplicate = format!("{original}{}\n", original.lines().nth(2).unwrap());
    fs::write(&path, &duplicate).unwrap();
    assert!(protocol::validate(&scratch.0).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), duplicate);
    fs::write(&path, original.replace("flatc=25.12.19", "flatc=0.0.0")).unwrap();
    assert!(protocol::validate(&scratch.0)
        .unwrap_err()
        .contains("pin mismatch"));
}
