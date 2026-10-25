use serde_json::{json, Value};
use std::{path::Path, process::Command};

fn run(home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .env_clear()
        .arg("--home")
        .arg(home)
        .args(["--json"])
        .args(args)
        .output()
        .unwrap()
}

fn success(home: &Path, args: &[&str]) -> Value {
    let output = run(home, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn agent_project_profile_configuration_is_explicit_persistent_and_clearable() {
    let root = std::env::temp_dir().join(format!("cgf-agent-project-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let home = root.join("state");
    let manifest = root.join("profile.json");
    std::fs::write(
        &manifest,
        serde_json::to_vec(&json!({
            "format_version":1,
            "id":"demo.project",
            "name":"Demo project",
            "goal":"A project goal"
        }))
        .unwrap(),
    )
    .unwrap();
    let published = success(
        &home,
        &["project-publish", "--manifest", manifest.to_str().unwrap()],
    );
    assert_eq!(published["version"], 1);

    let configured = success(
        &home,
        &["configure-agent-project", "--profile", "demo.project@1"],
    );
    assert_eq!(configured["agent_project_profile"], "demo.project@1");
    assert_eq!(configured["new_sessions_only"], true);
    assert_eq!(configured["running_services_require_restart"], true);
    assert_eq!(
        cogfabric::config::Config::load(Some(home.clone()), None)
            .unwrap()
            .agent_project_profile
            .as_deref(),
        Some("demo.project@1")
    );

    let before = std::fs::read(home.join("config.json")).unwrap();
    let unknown = run(
        &home,
        &["configure-agent-project", "--profile", "demo.project@2"],
    );
    assert!(!unknown.status.success());
    assert_eq!(std::fs::read(home.join("config.json")).unwrap(), before);

    let cleared = success(&home, &["configure-agent-project", "--clear"]);
    assert!(cleared["agent_project_profile"].is_null());
    assert_eq!(
        cogfabric::config::Config::load(Some(home.clone()), None)
            .unwrap()
            .agent_project_profile,
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}
