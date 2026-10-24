//! Exercise local effort commands through the installed-style chat entry point.
use std::{process::Stdio, time::Duration};
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn effort_commands_change_session_setting_without_contacting_a_server() {
    let root = std::env::temp_dir().join(format!("fabric-chat-effort-{}", uuid::Uuid::new_v4()));
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric-chat"))
        .args(["--server", "http://127.0.0.1:1/v1", "--home"])
        .arg(&root)
        .args([
            "--reasoning-effort",
            "medium",
            "--api-key-env",
            "COGFABRIC_EFFORT_FIXTURE_KEY",
        ])
        .env("COGFABRIC_EFFORT_FIXTURE_KEY", "offline-fixture-key")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(b"/effort\n/effort high\n/effort low\n/effort none\n/effort invalid\n/effort\n/reset\n/effort\n/effort default\n/quit\n").await.unwrap();
    drop(input);
    let result = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    for value in ["medium", "high", "low", "none", "server default"] {
        assert!(
            output.contains(&format!("Reasoning effort: {value}")),
            "{output}"
        );
    }
    assert!(output.contains("Use /effort default, none, low, medium or high."));
    // An invalid command and reset both preserve the last valid effort.
    assert_eq!(output.matches("Reasoning effort: none").count(), 3);
    assert!(output.contains("Goodbye."));
    assert!(!output.contains("offline-fixture-key"));
}
