//! Compile only fixed repository fixtures; never compile model output or user paths.
use serde::Deserialize;
use std::{collections::BTreeSet, path::PathBuf, process::Stdio, time::Duration};
use tokio::process::Command;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    input: String,
    expected: String,
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[tokio::test]
async fn public_rust_ownership_labels_match_compiler_and_specific_error_codes() {
    let raw = include_str!("../evaluation/rust_ownership_cases.jsonl");
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../evaluation/rust_ownership_manifest.json")).unwrap();
    assert_eq!(
        cogfabric::store::sha256(raw.as_bytes()),
        manifest["dataset_sha256"]
    );
    assert_eq!(manifest["case_count"], 20);
    assert_eq!(manifest["independent_human_review"], false);
    assert_eq!(manifest["held_out"], false);
    assert_eq!(manifest["live_model_evaluated"], false);
    let rows: Vec<Case> = raw
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let metadata = manifest["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 20);
    assert_eq!(metadata.len(), rows.len());
    assert_eq!(rows.iter().filter(|r| r.expected == "yes").count(), 10);
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let version = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(&compiler)
            .arg("--version")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(version.status.success());
    println!(
        "Compiler oracle: {}",
        String::from_utf8_lossy(&version.stdout).trim()
    );
    let scratch =
        Scratch(std::env::temp_dir().join(format!("fabric-rust-corpus-{}", uuid::Uuid::new_v4())));
    std::fs::create_dir(&scratch.0).unwrap();
    let mut ids = BTreeSet::new();
    let mut inputs = BTreeSet::new();
    for (index, (case, meta)) in rows.iter().zip(metadata).enumerate() {
        let id = meta["id"].as_str().unwrap();
        assert!(ids.insert(id));
        assert!(inputs.insert(&case.input));
        assert!(case.input.len() < 2000);
        assert_eq!(meta["line"], index + 1);
        let source = meta["source"].as_str().unwrap();
        assert!(source.len() < 1000);
        assert_eq!(
            meta["source_sha256"],
            cogfabric::store::sha256(source.as_bytes())
        );
        assert_eq!(case.input, format!("Does this standalone Rust 2021 library compile with the standard library, no external crates, and warnings allowed? Answer exactly yes or no.\n```rust\n{source}\n```"));
        assert_eq!(
            case.expected,
            if meta["expected_error_code"].is_null() {
                "yes"
            } else {
                "no"
            }
        );
        let input = scratch.0.join("fixture.rs");
        let diagnostics = scratch.0.join("diagnostics.jsonl");
        std::fs::write(&input, source).unwrap();
        let mut child = Command::new(&compiler)
            .args([
                "--edition=2021",
                "--crate-type=lib",
                "--crate-name=ownership_fixture",
                "--emit=metadata",
                "--error-format=json",
            ])
            .arg(&input)
            .arg("-o")
            .arg(scratch.0.join("fixture.rmeta"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&diagnostics).unwrap())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let status = match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
            Ok(result) => result.unwrap(),
            Err(_) => {
                child.start_kill().unwrap();
                tokio::time::timeout(Duration::from_secs(2), child.wait())
                    .await
                    .unwrap()
                    .unwrap();
                panic!("Compiler deadline exceeded for {id}");
            }
        };
        assert!(std::fs::metadata(&diagnostics).unwrap().len() < 1_000_000);
        assert_eq!(
            status.success(),
            case.expected == "yes",
            "Wrong compiler oracle label for {id}"
        );
        if let Some(code) = meta["expected_error_code"].as_str() {
            let output = std::fs::read_to_string(&diagnostics).unwrap();
            let errors: Vec<serde_json::Value> = output
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert!(
                errors
                    .iter()
                    .any(|e| e["level"] == "error" && e["code"]["code"] == code),
                "Expected specific diagnostic {code} for {id}"
            );
        }
    }
}
