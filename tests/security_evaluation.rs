use serde_json::Value;
use std::process::Command;

#[test]
fn cli_reports_reproducible_offline_boundary_results() {
    let home = std::env::temp_dir().join(format!("fabric-security-eval-{}", uuid::Uuid::new_v4()));
    let run = || {
        let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&home)
            .args(["--json", "security-eval"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let first = run();
    assert_eq!(first, run());
    assert_eq!(first["total"], 24);
    assert_eq!(first["passed"], 24);
    assert_eq!(first["success"], true);
    assert_eq!(first["unexpected_accepts"], 0);
    assert_eq!(first["fixture_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(first["neural_safety_evaluated"], false);
    assert!(first["unmeasured"].as_array().unwrap().len() >= 6);
    std::fs::remove_dir_all(home).unwrap();
}
