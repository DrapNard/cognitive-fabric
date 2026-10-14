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
    assert_eq!(first["total"], 31);
    assert_eq!(first["passed"], 31);
    assert_eq!(first["success"], true);
    assert_eq!(first["expected_denials"], 25);
    let results = first["results"].as_array().unwrap();
    for id in [
        "policy.module_solver",
        "policy.module_router",
        "policy.module_interface",
        "scope.module_operator",
        "scope.module_solver",
        "scope.module_router",
    ] {
        assert!(results.iter().any(|r| r["id"] == id
            && r["expected_accept"] == false
            && r["observed_accept"] == false));
    }
    assert!(results.iter().any(|r| r["id"] == "policy.module_operator"
        && r["expected_accept"] == true
        && r["observed_accept"] == true));
    assert_eq!(first["unexpected_accepts"], 0);
    assert_eq!(first["fixture_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(first["neural_safety_evaluated"], false);
    assert!(first["unmeasured"].as_array().unwrap().len() >= 6);
    std::fs::remove_dir_all(home).unwrap();
}
