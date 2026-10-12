use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=protocol/cognitive.fbs");
    println!("cargo:rerun-if-changed=protocol/module.fbs");
    println!("cargo:rerun-if-changed=protocol/events.fbs");
    println!("cargo:rerun-if-changed=protocol/bootstrap.fbs");
    println!("cargo:rerun-if-changed=protocol/event_stream.fbs");
    println!("cargo:rerun-if-changed=protocol/tool.fbs");
    println!("cargo:rerun-if-changed=protocol/sql_tool.fbs");
    println!("cargo:rerun-if-changed=protocol/tool_bootstrap.fbs");
    println!("cargo:rerun-if-env-changed=FLATC");
    let compiler = env::var_os("FLATC").unwrap_or_else(|| "flatc".into());
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo"));
    let version = Command::new(&compiler)
        .arg("--version")
        .output()
        .expect("flatc not found; build v25.12.19 with bash scripts/build-flatc.sh /tmp/cogfabric-flatc, then export FLATC=/tmp/cogfabric-flatc/build/flatc");
    assert!(version.status.success(), "flatc --version failed");
    let version = String::from_utf8_lossy(&version.stdout);
    assert!(
        version.trim() == "flatc version 25.12.19",
        "Expected FlatBuffers compiler v25.12.19 to match the Rust runtime, got {version}"
    );
    let status = Command::new(&compiler)
        .args(["--rust", "--gen-object-api", "-o"])
        .arg(&out_dir)
        .arg("protocol/cognitive.fbs")
        .arg("protocol/module.fbs")
        .arg("protocol/events.fbs")
        .arg("protocol/bootstrap.fbs")
        .arg("protocol/event_stream.fbs")
        .arg("protocol/tool.fbs")
        .arg("protocol/sql_tool.fbs")
        .arg("protocol/tool_bootstrap.fbs")
        .status()
        .expect("flatc was not found. Install FlatBuffers compiler (e.g. pacman -S flatbuffers).");
    assert!(status.success(), "flatc could not compile protocol schemas");
}
