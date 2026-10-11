use cogfabric::{config::Config, store::Store};
use std::process::Command;

#[test]
fn cli_backup_and_explicit_restore_preserve_settings_in_a_new_workspace() {
    let root = std::env::temp_dir().join(format!("fabric-cli-restore-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: root.join("source"),
        max_tokens: 1024,
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    cogfabric::store::bootstrap(&store).unwrap();
    let input = root.join("reviewed.md");
    std::fs::write(&input, "Reviewed CLI archive fixture").unwrap();
    cogfabric::evolution::create(&cfg, &store, "rust.s1", "rag", &input, "fixture", "CC0").unwrap();
    drop(store);
    let backup = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&cfg.home)
        .args(["--json", "backup", "--include-rag-artifacts"])
        .output()
        .unwrap();
    assert!(
        backup.status.success(),
        "{}",
        String::from_utf8_lossy(&backup.stderr)
    );
    let backup: serde_json::Value = serde_json::from_slice(&backup.stdout).unwrap();
    assert_eq!(backup["includes_configuration"], true);
    let destination = root.join("restored");
    let result = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(root.join("unused-source"))
        .args([
            "--json",
            "restore-database",
            backup["path"].as_str().unwrap(),
            "--checksum",
            backup["checksum"].as_str().unwrap(),
            "--destination",
        ])
        .arg(&destination)
        .arg("--restore-configuration")
        .arg("--restore-rag-artifacts")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["configuration_restored"], true);
    assert_eq!(result["rag_artifacts_restored"], 1);
    assert_eq!(
        std::fs::read_dir(destination.join("artifacts"))
            .unwrap()
            .count(),
        1
    );
    assert!(!root.join("unused-source").exists());
    let restored: Config =
        serde_json::from_slice(&std::fs::read(destination.join("config.json")).unwrap()).unwrap();
    assert_eq!(restored.max_tokens, 1024);
    assert!(restored.home.as_os_str().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
