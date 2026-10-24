//! Invalid acquisition plans fail before network or runtime initialization.
use cogfabric::store::sha256;
use serde_json::{json, Value};
use std::process::Command;

#[test]
fn cli_refuses_unapproved_or_unsafe_manifests_without_creating_state_or_output() {
    let root =
        std::env::temp_dir().join(format!("fabric-acquisition-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let plan = json!({"format_version":1,"reviewer":"test operator","acquisition_approved":true,
        "sources":[{"id":"source.fixture","url":"https://example.com/doc.txt",
        "sha256":"a".repeat(64),"source":"operator-selected-fixture","license":"GPL-3.0-only"}]});
    let mut cases: Vec<Value> = Vec::new();
    let mut unapproved = plan.clone();
    unapproved["acquisition_approved"] = false.into();
    cases.push(unapproved);
    for url in [
        "http://127.0.0.1:1/doc.txt",
        "https://127.0.0.1/doc.txt",
        "https://[::1]/doc.txt",
        "https://example.com:8443/doc.txt",
        "https://user:secret@example.com/doc.txt",
        "https://example.com/doc.txt?token=secret",
        "https://example.com/doc.txt#fragment",
    ] {
        let mut invalid = plan.clone();
        invalid["sources"][0]["url"] = url.into();
        cases.push(invalid);
    }
    let mut traversal = plan.clone();
    traversal["sources"][0]["id"] = "../../escape".into();
    cases.push(traversal);
    let mut unknown = plan.clone();
    unknown["grant_tools"] = true.into();
    cases.push(unknown);
    let mut duplicate = plan.clone();
    duplicate["sources"]
        .as_array_mut()
        .unwrap()
        .push(plan["sources"][0].clone());
    cases.push(duplicate);
    for (index, value) in cases.iter().enumerate() {
        let manifest = root.join(format!("plan-{index}.json"));
        let bytes = serde_json::to_vec(value).unwrap();
        std::fs::write(&manifest, &bytes).unwrap();
        let home = root.join(format!("unused-home-{index}"));
        let output = root.join(format!("unpublished-{index}"));
        let result = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&home)
            .args(["--json", "candidate", "fetch-sources", "--manifest"])
            .arg(&manifest)
            .args(["--sha256", &sha256(&bytes), "--output"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success(), "unsafe plan succeeded: {value}");
        assert!(!home.exists() && !output.exists());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("secret"));
    }
    std::fs::remove_dir_all(root).unwrap();
}
