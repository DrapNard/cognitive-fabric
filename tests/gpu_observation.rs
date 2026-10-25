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

#[test]
fn explicit_nvml_diagnostics_preserve_missing_driver_as_unavailable() {
    let root = std::env::temp_dir().join(format!("fabric-nvml-probe-{}", uuid::Uuid::new_v4()));
    let cfg = Config {
        home: root.clone(),
        ..Default::default()
    };
    cfg.save().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .env_clear()
        .arg("--home")
        .arg(&root)
        .args([
            "--json",
            "diagnostics",
            "--probe-gpu",
            "--gpu-sensor",
            "nvml",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let probe = &report["gpu_telemetry"];
    assert_eq!(probe["sensor"], "nvml");
    assert_eq!(probe["sensor_compiled_support"], cfg!(feature = "nvml-gpu"));
    assert_eq!(probe["scope"], "whole_device");
    assert_eq!(probe["task_attributed"], false);
    assert!(probe["energy_joules"].is_null());
    match probe["status"].as_str().unwrap() {
        "sampled" => assert!(!probe["devices"].as_array().unwrap().is_empty()),
        "unavailable" => {
            assert!(probe["devices"].is_null());
            assert!(!probe["reason"].as_str().unwrap().is_empty());
        }
        other => panic!("Unexpected GPU probe status: {other}"),
    }
    std::fs::remove_dir_all(root).unwrap();
}
