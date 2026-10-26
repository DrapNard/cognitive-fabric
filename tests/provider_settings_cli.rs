//! Provider effort and deadline bounds must survive CLI persistence.
use serde_json::Value;
use std::process::Command;

#[test]
fn provider_effort_and_deadline_are_explicit_persisted_and_bounded() {
    let home = std::env::temp_dir().join(format!("cgf-provider-settings-{}", uuid::Uuid::new_v4()));
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&home)
            .arg("--json")
            .args(args)
            .output()
            .unwrap()
    };
    for invalid in ["0", "301", "invalid"] {
        assert!(!run(&[
            "configure-local",
            "--model",
            "fixture",
            "--timeout-seconds",
            invalid
        ])
        .status
        .success());
        assert!(
            !home.exists(),
            "argument validation precedes workspace creation"
        );
    }
    let result = run(&[
        "configure-local",
        "--model",
        "fixture",
        "--reasoning-effort",
        "none",
        "--timeout-seconds",
        "120",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved: Value =
        serde_json::from_slice(&std::fs::read(home.join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["reasoning_effort"], "none");
    assert_eq!(saved["timeout_seconds"], 120);
    assert_eq!(saved["max_wall_seconds"], 120);
    assert_eq!(saved["max_tokens"], 1600);
    assert!(run(&[
        "configure-local",
        "--model",
        "fixture",
        "--reasoning-effort",
        "high"
    ])
    .status
    .success());
    let saved: Value =
        serde_json::from_slice(&std::fs::read(home.join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["reasoning_effort"], "high");
    assert_eq!(saved["timeout_seconds"], 120);
    std::fs::remove_dir_all(home).unwrap();
}
