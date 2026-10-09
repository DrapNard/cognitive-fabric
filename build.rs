use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=protocol/cognitive.fbs");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo"));
    let version = Command::new("flatc").arg("--version").output()
        .expect("flatc not found; install FlatBuffers compiler v25.12.19");
    let version = String::from_utf8_lossy(&version.stdout);
    assert!(version.contains("25.12.19"),
        "Expected FlatBuffers compiler v25.12.19 to match the Rust runtime, got {version}");
    let status = Command::new("flatc")
        .args(["--rust", "--gen-object-api", "-o"])
        .arg(&out_dir)
        .arg("protocol/cognitive.fbs")
        .status()
        .expect("flatc was not found. Install FlatBuffers compiler (e.g. pacman -S flatbuffers).");
    assert!(status.success(), "flatc could not compile protocol/cognitive.fbs");
}
