use serde_json::Value;
use std::process::Command;

#[test]
fn collective_toggle_is_explicit_persistent_and_does_not_initialize_tasks() {
    let home = std::env::temp_dir().join(format!(
        "cgf-client-collective-cli-{}",
        uuid::Uuid::new_v4()
    ));
    for enabled in [true, false] {
        let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .env_clear()
            .arg("--home")
            .arg(&home)
            .arg("--json")
            .arg("configure-agent-collective")
            .arg("--enabled")
            .arg(enabled.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["agent_collective_enabled"], enabled);
        assert_eq!(report["running_services_require_restart"], true);
        assert_eq!(report["model_switch"], false);
        let cfg = cogfabric::config::Config::load(Some(home.clone()), None).unwrap();
        assert_eq!(cfg.agent_collective, enabled);
        assert!(
            !cfg.db().exists(),
            "configuration must not bootstrap a database or create tasks"
        );
    }
    let before = std::fs::read(home.join("config.json")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .env_clear()
        .arg("--home")
        .arg(&home)
        .args(["configure-agent-collective", "--enabled", "invalid"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(home.join("config.json")).unwrap(), before);
    std::fs::remove_dir_all(home).unwrap();
}
