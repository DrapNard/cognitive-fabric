#[path = "build_support/fingerprint.rs"]
mod fingerprint;
#[path = "build_support/protocol.rs"]
mod protocol;
use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-env-changed=FLATC");
    println!("cargo:rerun-if-changed=build_support/protocol.rs");
    println!("cargo:rerun-if-changed=protocol/generated/manifest.sha256");
    println!("cargo:rerun-if-changed=Cargo.toml");
    for schema in protocol::SCHEMAS {
        println!("cargo:rerun-if-changed=protocol/{schema}.fbs");
        println!(
            "cargo:rerun-if-changed=protocol/generated/{}",
            protocol::generated_name(schema)
        );
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"));
    for path in ["src", "build_support", "protocol", "Cargo.lock"] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!(
        "cargo:rustc-env=COGFABRIC_RUNTIME_SOURCES_SHA256={}",
        fingerprint::calculate(&root).expect("Fingerprint runtime sources")
    );
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("Read Cargo manifest");
    assert!(
        protocol::runtime_pinned(&manifest),
        "Rust FlatBuffers dependency must remain pinned to the reviewed generator version"
    );
    let bundle=protocol::validate(&root).unwrap_or_else(|error|panic!("{error}; regenerate reviewed bindings with bash scripts/update-protocol-bindings.sh /path/to/flatc"));
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    if let Some(compiler) = env::var_os("FLATC") {
        let version = Command::new(&compiler)
            .arg("--version")
            .output()
            .expect("Configured FLATC cannot run; unset FLATC to use reviewed bundled bindings");
        assert!(
            version.status.success(),
            "Configured flatc --version failed"
        );
        assert_eq!(
            String::from_utf8_lossy(&version.stdout).trim(),
            format!("flatc version {}", protocol::VERSION),
            "Configured FlatBuffers compiler does not match the reviewed Rust runtime"
        );
        let mut command = Command::new(compiler);
        command
            .args(["--rust", "--gen-object-api", "-o"])
            .arg(&output);
        for schema in protocol::SCHEMAS {
            command.arg(root.join(format!("protocol/{schema}.fbs")));
        }
        assert!(
            command.status().expect("Run configured flatc").success(),
            "FlatBuffers generation failed"
        );
        for path in bundle {
            let name = path.file_name().unwrap();
            assert!(
                fs::read(output.join(name)).expect("Read generated bindings")
                    == fs::read(&path).expect("Read reviewed bindings"),
                "Compiler output differs from reviewed bindings: {}",
                name.to_string_lossy()
            );
        }
    } else {
        for path in bundle {
            fs::copy(&path, output.join(path.file_name().unwrap()))
                .expect("Copy verified bindings");
        }
    }
}
