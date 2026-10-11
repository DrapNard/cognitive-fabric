#![cfg(target_os = "linux")]
use serde_json::Value;
use std::process::Command;

#[test]
fn separate_cli_processes_preserve_original_boot_bound_clock() {
    let home = std::env::temp_dir().join(format!("fabric-clock-cli-{}", uuid::Uuid::new_v4()));
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&home)
            .args(["--backend", "demo", "--json"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let outcome = run(&["ask", "calc: 2+2"]);
    let task = outcome["task_id"].as_str().unwrap();
    let first = run(&["inference-audit", task]);
    let second = run(&["inference-audit", task]);
    let time = |v: &Value| {
        v["remaining_inference_admission"]["wall_time"]["remaining_ns"]
            .as_u64()
            .unwrap()
    };
    assert!(time(&second) <= time(&first));
    assert!(time(&first) <= 120_000_000_000);
    assert_eq!(
        second["remaining_inference_admission"]["wall_time"]["status"],
        "same_boot"
    );
    assert_eq!(second["resume_authorized"], false);
    std::fs::remove_dir_all(home).unwrap();
}
