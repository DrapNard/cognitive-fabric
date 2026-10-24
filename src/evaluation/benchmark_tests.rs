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
                provider_metrics: None,
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
    assert_eq!(report["configuration"]["model"], engine.cfg.model);
    assert!(report["configuration"].get("home").is_none());
    assert_eq!(
        report["configuration_sha256"],
        store::sha256(&serde_json::to_vec(&report["configuration"]).unwrap())
    );
    assert_eq!(
        report["compiled_role_prompts_sha256"],
        crate::backend::runtime_role_prompts_sha256().unwrap()
    );
    assert_eq!(report["weights_attested"], false);
    assert_eq!(report["backend_configuration_attested"], false);
    assert_eq!(report["details"][0]["reported_models"], json!(["fixture"]));
    assert!(report["details"][0]["primary_model_snapshot"]["registration"].is_null());

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
    assert_eq!(report["details"][0]["reported_models"], json!([]));
}

#[tokio::test]
async fn ready_cancellation_returns_empty_partial_report_without_task_admission() {
    let (engine, backend) = engine();
    let dataset = Dataset::new(&json!({"input":"Answer yes.","expected":"yes"}).to_string());
    let report = benchmark_until(&engine, &dataset.0, "fast", 10, async {})
        .await
        .unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(report["stop_reason"], "cancelled");
    assert_eq!(report["executed"], 0);
    assert_eq!(report["unexecuted"], 1);
    assert!(report["accuracy"].is_null());
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
async fn invalid_run_limits_and_modes_fail_before_admission() {
    let (mut engine, backend) = engine();
    let dataset = Dataset::new(&json!({"input":"Answer yes.","expected":"yes"}).to_string());
    for (mode, limit) in [("fast", 0), ("fast", 86401), ("unbounded", 10)] {
        assert!(
            benchmark_until(&engine, &dataset.0, mode, limit, std::future::pending())
                .await
                .is_err()
        );
    }
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    let count: u64 = engine
        .store
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    engine.cfg.api_base = "http://user:secret@127.0.0.1/v1".into();
    assert!(
        benchmark_until(&engine, &dataset.0, "fast", 10, async {})
            .await
            .is_err(),
        "Unsafe configuration must fail before reporting even when already cancelled"
    );
}

struct Pending {
    entered: tokio::sync::Notify,
    calls: AtomicUsize,
    dropped: AtomicUsize,
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl InferenceBackend for Pending {
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Box::pin(async {
                Ok(Generation {
                    text: "yes".into(),
                    model: "fixture".into(),
                    input_tokens: Some(1),
                    output_tokens: Some(1),
                    elapsed_ms: 0.0,
                    queue_wait_ms: None,
                    provider_metrics: None,
                })
            });
        }
        Box::pin(async move {
            let _active = Active(&self.dropped);
            self.entered.notify_one();
            std::future::pending().await
        })
    }
}
#[tokio::test(start_paused = true)]
async fn run_deadline_cancels_active_inference_and_retains_charges_without_later_cases() {
    run_interrupted(false).await;
}
#[tokio::test]
async fn cancellation_cancels_active_inference_and_retains_charges_without_later_cases() {
    run_interrupted(true).await;
}
async fn run_interrupted(cancel: bool) {
    let (engine, _) = engine();
    let backend = Arc::new(Pending {
        entered: tokio::sync::Notify::new(),
        calls: AtomicUsize::new(0),
        dropped: AtomicUsize::new(0),
    });
    let engine = Engine::with_backend(engine.cfg, engine.store, backend.clone());
    let raw = format!(
        "{}\n{}\n{}\n",
        json!({"input":"Answer yes.","expected":"yes"}),
        json!({"input":"Another question","expected":"no"}),
        json!({"input":"Not admitted","expected":"yes"})
    );
    let dataset = Dataset::new(&raw);
    let report = benchmark_until(
        &engine,
        &dataset.0,
        "fast",
        if cancel { 10 } else { 1 },
        async {
            if cancel {
                backend.entered.notified().await;
            } else {
                std::future::pending::<()>().await;
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(
        report["stop_reason"],
        if cancel { "cancelled" } else { "run_deadline" }
    );
    assert_eq!(report["total"], 3);
    assert_eq!(report["executed"], 2);
    assert_eq!(report["unexecuted"], 1);
    assert_eq!(report["model_calls"], 2);
    assert_eq!(report["runtime_status_counts"]["cancelled"], 1);
    assert_eq!(report["passed"], 1);
    assert_eq!(report["accuracy"], 0.5);
    assert_eq!(report["details"][0]["passed"], true);
    let detail = &report["details"][1];
    assert!(detail["resources"]["input_tokens"].is_null());
    assert!(detail["resources"]["output_tokens"].is_null());
    let trace = engine
        .store
        .task(detail["task_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(trace["task"]["status"], "cancelled");
    tokio::time::timeout(Duration::from_secs(1), async {
        while backend.dropped.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let count: u64 = engine
        .store
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}
