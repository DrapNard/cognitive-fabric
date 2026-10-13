use super::*;
use crate::{
    backend::{Generation, InferenceBackend},
    config::Config,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
#[derive(Default)]
struct Fixture {
    calls: AtomicUsize,
    prompts: Mutex<Vec<String>>,
}
impl InferenceBackend for Fixture {
    fn generate<'a>(
        &'a self,
        _: &'a str,
        prompt: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.prompts.lock().unwrap().push(prompt.into());
        Box::pin(async {
            Ok(Generation {
                text: "yes".into(),
                model: "fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: None,
            })
        })
    }
}
fn engine() -> (Engine, Arc<Fixture>) {
    let backend = Arc::new(Fixture::default());
    let cfg = Config {
        backend: "openai-local".into(),
        max_calls: 1,
        max_cycles: 1,
        max_tokens: 512,
        max_tool_calls: 0,
        interpret_goals: false,
        synthesize_results: false,
        ..Default::default()
    };
    (
        Engine::with_backend(cfg, Arc::new(Store::in_memory().unwrap()), backend.clone()),
        backend,
    )
}
struct Dataset(std::path::PathBuf);
impl Dataset {
    fn new(raw: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "fabric-bench-preflight-{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, raw).unwrap();
        Self(path)
    }
}
impl Drop for Dataset {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[tokio::test]
async fn every_benchmark_case_is_validated_before_task_or_inference_admission() {
    let valid = json!({"input":"Answer yes.","expected":"yes"}).to_string();
    for invalid in [
        json!({"input":"x".repeat(12001),"expected":"yes"}).to_string(),
        json!({"input":"Question","expected":"x".repeat(16001)}).to_string(),
        json!({"input":"Question","expected":"yes","verification":"model_confidence"}).to_string(),
        json!({"input":" ","expected":"yes"}).to_string(),
        json!({"input":"Question","expected":" "}).to_string(),
        "not JSON".into(),
        r#"{"input":"Question","expected":"yes","expected":"no"}"#.into(),
    ] {
        let (engine, backend) = engine();
        let dataset = Dataset::new(&format!("{valid}\n{invalid}\n"));
        assert!(benchmark(&engine, &dataset.0, "fast").await.is_err());
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        let count: u64 = engine
            .store
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "A late invalid case must not execute earlier cases"
        );
    }
}
#[tokio::test]
async fn excessive_benchmark_count_fails_before_execution() {
    let (engine, backend) = engine();
    let dataset =
        Dataset::new(&format!("{}\n", json!({"input":"Question","expected":"yes"})).repeat(1001));
    assert!(benchmark(&engine, &dataset.0, "fast").await.is_err());
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    let count: u64 = engine
        .store
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn benchmark_discloses_dataset_identity_and_charged_execution_without_label_leaks() {
    let (engine, backend) = engine();
    let raw = format!(
        "{}\n{}\n",
        json!({"input":"Answer yes.","expected":" YES "}),
        json!({"input":"Answer another question.","expected":"label_not_for_model_7d4b"})
    );
    let dataset = Dataset::new(&raw);
    let report = benchmark(&engine, &dataset.0, "fast").await.unwrap();
    assert_eq!(report["dataset_sha256"], store::sha256(raw.as_bytes()));
    assert_eq!(report["mode"], "fast");
    assert_eq!(report["total"], 2);
    assert_eq!(report["passed"], 1);
    assert_eq!(report["tasks_with_model_calls"], 2);
    assert_eq!(report["tasks_with_tool_calls"], 0);
    assert_eq!(report["runtime_status_counts"]["unverified"], 2);
    assert!(backend
        .prompts
        .lock()
        .unwrap()
        .iter()
        .all(|p| !p.contains("label_not_for_model_7d4b")));
}
#[tokio::test]
async fn exhausted_no_call_benchmark_is_visible_without_becoming_a_pass() {
    let (mut engine, backend) = engine();
    engine.cfg.max_tokens = 128;
    let dataset = Dataset::new(&json!({"input":"Answer yes.","expected":"yes"}).to_string());
    let report = benchmark(&engine, &dataset.0, "fast").await.unwrap();
    assert_eq!(report["passed"], 0);
    assert_eq!(report["model_calls"], 0);
    assert_eq!(report["tasks_with_model_calls"], 0);
    assert_eq!(report["runtime_status_counts"]["exhausted"], 1);
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn deterministic_tool_benchmark_is_a_valid_no_model_execution() {
    let (mut engine, backend) = engine();
    engine.cfg.max_tool_calls = 1;
    let dataset = Dataset::new(&json!({"input":"calc:6*7","expected":"42"}).to_string());
    let report = benchmark(&engine, &dataset.0, "fast").await.unwrap();
    assert_eq!(report["passed"], 1);
    assert_eq!(report["tasks_with_model_calls"], 0);
    assert_eq!(report["tasks_with_tool_calls"], 1);
    assert_eq!(report["runtime_status_counts"]["completed"], 1);
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
}
