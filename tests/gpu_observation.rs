use cogfabric::config::Config;
use serde_json::Value;
use std::process::Command;

#[test]
fn gpu_window_reporting_is_explicit_optional_and_never_claims_task_energy() {
    let root = std::env::temp_dir().join(format!("fabric-gpu-compare-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let dataset = root.join("cases.jsonl");
    std::fs::write(&dataset, "{\"input\":\"calc:6*7\",\"expected\":\"42\"}\n").unwrap();
    for enabled in [false, true] {
        let cfg = Config {
            home: root.join(if enabled { "observed" } else { "default" }),
            backend: "demo".into(),
            interpret_goals: false,
            ..Default::default()
        };
        cfg.save().unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_cogfabric"));
        command
            .env_clear()
            .arg("--home")
            .arg(&cfg.home)
            .args(["--json", "compare"])
            .arg(&dataset)
            .args([
                "--mode",
                "deep",
                "--source",
                "public workflow fixture",
                "--license",
                "GPL-3.0-only",
                "--workflow-check",
                "--code-revision",
            ])
            .arg("a".repeat(40))
            .args(["--max-run-seconds", "60"]);
        if enabled {
            command.arg("--observe-gpu");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["complete"], true);
        assert_eq!(report["arms"].as_array().unwrap().len(), 6);
        assert_eq!(report.get("gpu_observation").is_some(), enabled);
        for detail in report["details"].as_array().unwrap() {
            assert_eq!(detail.get("gpu_observation").is_some(), enabled);
            if enabled {
                let observation = &detail["gpu_observation"];
                assert_eq!(observation["task_attributed"], false);
                assert!(observation["task_energy_joules"].is_null());
                assert_eq!(observation["true_peak_memory_known"], false);
                assert_eq!(observation["full_window_energy_known"], false);
            }
        }
        if enabled {
            assert_eq!(report["gpu_observation"]["task_attributed"], false);
            assert!(
                report["gpu_observation"]["remaining_run_samples"]
                    .as_u64()
                    .unwrap()
                    <= 600
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
