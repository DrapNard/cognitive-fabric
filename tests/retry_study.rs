use serde_json::{json, Value};
use std::{fs, os::unix::fs::symlink, process::Command};

#[test]
fn operator_cli_inspects_retry_evidence_without_state_and_rejects_ambiguous_or_linked_inputs() {
    // These invented measurements test parser/CLI behavior, never model quality.
    let root = std::env::temp_dir().join(format!("cogfabric-retry-study-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let home = root.join("must-not-create");
    let study = root.join("study.json");
    let thresholds = root.join("thresholds.json");
    let config = cogfabric::config::Config::default();
    let data = json!({
        "format_version":1,"mode":"deep","criterion":"integer_sort/v1","cycle":1,
        "configuration_sha256":cogfabric::escalation::empirical::configuration_fingerprint(&config).unwrap(),
        "role_prompts_sha256":"b".repeat(64),
        "runtime_sources_sha256":cogfabric::escalation::empirical::runtime_sources_fingerprint(),
        "dataset_sha256":"a".repeat(64),"source":"Synthetic CLI fixture, not a quality study","license":"CC0-1.0","reviewer":"fixture.operator",
        "held_out_operator_attested":true,"independent_outcomes_operator_attested":true,"includes_failed_attempts_operator_attested":true,
        "observations":[
            {"task_id":uuid::Uuid::new_v4().to_string(),"verified":true,"model_calls":1,"reserved_output_tokens":384,"tool_calls":1,"prompt_bytes":1000,"elapsed_ms":2},
            {"task_id":uuid::Uuid::new_v4().to_string(),"verified":false,"model_calls":1,"reserved_output_tokens":384,"tool_calls":0,"prompt_bytes":1000,"elapsed_ms":2}
        ]
    });
    fs::write(&study, serde_json::to_vec(&data).unwrap()).unwrap();
    fs::write(&thresholds,r#"{"min_observations":2,"min_success_basis_points":5000,"max_output_tokens_per_success":768,"max_prompt_bytes_per_success":2000,"max_milliseconds_per_success":4}"#).unwrap();
    let run = |input: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&home)
            .arg("--json")
            .arg("retry-study-evaluate")
            .arg(input)
            .arg("--thresholds")
            .arg(&thresholds)
            .output()
            .unwrap()
    };
    let output = run(&study);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["evaluation"]["observations"], 2);
    assert_eq!(report["evaluation"]["verified_retries"], 1);
    assert_eq!(report["evaluation"]["total_reserved_output_tokens"], 768);
    assert_eq!(report["evaluation"]["calibrated_probability"], false);
    assert_eq!(report["installed"], false);
    assert_eq!(report["permissions_granted"], false);
    assert!(!home.exists());
    let linked = root.join("linked.json");
    symlink(&study, &linked).unwrap();
    assert!(!run(&linked).status.success());
    let mut bad = data.clone();
    bad["includes_failed_attempts_operator_attested"] = json!(false);
    fs::write(&study, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(!run(&study).status.success());
    fs::write(&study, r#"{"format_version":1,"format_version":1}"#).unwrap();
    assert!(!run(&study).status.success());
    assert!(!home.exists());
    for actor in ["jev-plus", "jev-r", "interface"] {
        assert!(cogfabric::security::authorize(actor, "evaluation.retry").is_err());
    }
    fs::remove_dir_all(root).unwrap();
}
