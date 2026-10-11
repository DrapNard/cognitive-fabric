use cogfabric::{
    backend::LocalBackend,
    config::Config,
    reasoning::Session,
    store::{sha256, Store},
};
use serde_json::json;
use std::{process::Command, sync::Arc};
#[test]
fn cli_recovers_a_checked_saved_answer_without_inference() {
    let home = std::env::temp_dir().join(format!(
        "fabric-cli-result-recovery-{}",
        uuid::Uuid::new_v4()
    ));
    let cfg = Config {
        home: home.clone(),
        ..Default::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let task = store
        .create_task(r#"sort:{"values":[3,1,3]}"#, "fast")
        .unwrap();
    let _session = Session::new(
        cfg.clone(),
        store.clone(),
        Arc::new(LocalBackend(cfg)),
        task.clone(),
        "fast",
    )
    .unwrap();
    store
        .event(
            &task,
            "model_dispatch",
            "jev-r",
            json!({"call":1,"role":"solver","allocation":384,"prompt_bytes":100}),
        )
        .unwrap();
    store.event(&task,"reasoning","jev-r",json!({"call":1,"node":{"role":"solver","model":"fixture","content_hash":sha256(b"[1,3,3]")},"text":"[1,3,3]","input_tokens":null,"output_tokens":null})).unwrap();
    store
        .finish_task(&task, "interrupted", "No durable verified result")
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args(["--json", "recover-verified-result", &task])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(report["inference_repeated"], false);
    assert_eq!(store.task(&task).unwrap()["task"]["answer"], "[1,3,3]");
    assert_eq!(store.inference_audit(&task).unwrap()["reserved_calls"], 1);
    drop(_session);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
