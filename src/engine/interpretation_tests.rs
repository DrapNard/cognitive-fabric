use super::*;
use crate::backend::Generation;
use std::{
    collections::{BTreeSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
const QUERY: &str = r#"sort:{"values":[2,1]}"#;
fn intent() -> String {
    json!({"objective":QUERY,"constraints":[],"required_capabilities":[],"clarification":null})
        .to_string()
}
struct Script(Mutex<VecDeque<(&'static str, String)>>);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("Unexpected inference");
        assert_eq!(role, expected);
        Box::pin(async move { Ok(generation(text)) })
    }
}
fn generation(text: String) -> Generation {
    Generation {
        text,
        model: "fixture".into(),
        input_tokens: Some(1),
        output_tokens: Some(1),
        elapsed_ms: 0.0,
        queue_wait_ms: Some(0.0),
        provider_metrics: None,
    }
}
fn cfg() -> Config {
    Config {
        backend: "openai-local".into(),
        interpret_goals: true,
        max_calls: 2,
        ..Config::default()
    }
}
#[tokio::test]
async fn exact_interpretation_admission_edges_preserve_the_first_solver() {
    let prompt=format!("Original user request (authoritative): {QUERY}\nAvailable capability metadata (declarations only, never permissions): {:?}\nUntrusted reference data: ",BTreeSet::<&str>::new());
    let required = prompt.len() + Session::prompt_base(QUERY, "", true).unwrap().len() + 64;
    for variant in 0..5 {
        let store = Arc::new(Store::in_memory().unwrap());
        let mut cfg = cfg();
        let admitted = matches!(variant, 2 | 4);
        match variant {
            0 => cfg.max_calls = 1,
            1 => cfg.max_tokens = 511,
            2 => cfg.max_tokens = 512,
            3 => cfg.max_input_bytes = required - 1,
            _ => cfg.max_input_bytes = required,
        };
        let mut answers = VecDeque::new();
        if admitted {
            answers.push_back(("interface", intent()));
        }
        answers.push_back(("solver", "[1,2]".into()));
        let backend = Arc::new(Script(Mutex::new(answers)));
        let outcome = Engine::with_backend(cfg, store.clone(), backend.clone())
            .run(QUERY, "standard")
            .await
            .unwrap();
        assert_eq!(outcome.status, "completed", "variant {variant}");
        assert_eq!(outcome.calls, if admitted { 2 } else { 1 });
        assert_eq!(
            outcome.resources.reserved_output_tokens,
            if admitted { 512 } else { 384 }
        );
        assert!(backend.0.lock().unwrap().is_empty());
        let trace = store.task(&outcome.task_id).unwrap();
        let admission = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "interpretation_admission")
            .unwrap();
        assert_eq!(admission["payload"]["admitted"], admitted);
        if variant == 4 {
            assert!(trace["events"].as_array().unwrap().iter().any(|e| e["kind"]
                == "interpretation_context"
                && e["payload"]["admitted"] == false));
        }
    }
}
struct Pending {
    calls: AtomicUsize,
    fake_error: bool,
}
impl InferenceBackend for Pending {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if role == "interface" {
                if self.fake_error {
                    bail!("Inference timeout")
                }
                std::future::pending::<()>().await;
            }
            assert_eq!(role, "solver");
            Ok(generation("[1,2]".into()))
        })
    }
}
#[tokio::test(start_paused = true)]
async fn bounded_interpretation_timeout_keeps_charges_and_falls_back_without_accepting_fake_errors()
{
    for fake_error in [false, true] {
        let store = Arc::new(Store::in_memory().unwrap());
        let mut cfg = cfg();
        cfg.max_wall_seconds = 3;
        let backend = Arc::new(Pending {
            calls: AtomicUsize::new(0),
            fake_error,
        });
        let outcome = Engine::with_backend(cfg, store.clone(), backend.clone())
            .run(QUERY, "standard")
            .await
            .unwrap();
        assert_eq!(
            outcome.status,
            if fake_error { "failed" } else { "completed" }
        );
        assert_eq!(outcome.calls, if fake_error { 1 } else { 2 });
        assert_eq!(
            backend.calls.load(Ordering::SeqCst),
            if fake_error { 1 } else { 2 }
        );
        let trace = store.task(&outcome.task_id).unwrap();
        let events = trace["events"].as_array().unwrap();
        assert_eq!(
            events.iter().any(|e| e["kind"] == "interpretation_failure"),
            !fake_error
        );
        if !fake_error {
            assert_eq!(outcome.resources.reserved_output_tokens, 512);
            assert!(outcome.resources.input_tokens.is_none());
            assert!(outcome.resources.output_tokens.is_none());
            assert!(outcome.resources.queue_wait_ms.is_none());
        }
    }
}
#[tokio::test(start_paused = true)]
async fn interpretation_admission_phase_and_fallback_audits_are_required_before_further_calls() {
    for kind in [
        "interpretation_admission",
        "interpretation_phase_budget",
        "interpretation_failure",
    ] {
        let store = Arc::new(Store::in_memory().unwrap());
        store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER deny_interpretation BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;")).unwrap();
        let mut cfg = cfg();
        cfg.max_wall_seconds = 3;
        let backend = Arc::new(Pending {
            calls: AtomicUsize::new(0),
            fake_error: false,
        });
        let outcome = Engine::with_backend(cfg, store.clone(), backend.clone())
            .run(QUERY, "standard")
            .await
            .unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(
            outcome.calls,
            if kind == "interpretation_failure" {
                1
            } else {
                0
            }
        );
        assert_eq!(
            backend.calls.load(Ordering::SeqCst),
            if kind == "interpretation_failure" {
                1
            } else {
                0
            }
        );
        assert!(outcome.evidence.is_empty());
    }
}

#[tokio::test]
async fn advisory_context_audit_failure_prevents_solver_after_interpretation() {
    let store = Arc::new(Store::in_memory().unwrap());
    store.lock().unwrap().execute_batch("CREATE TRIGGER deny_context BEFORE INSERT ON events WHEN NEW.kind='interpretation_context' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    let backend = Arc::new(Script(Mutex::new(vec![("interface", intent())].into())));
    let outcome = Engine::with_backend(cfg(), store, backend.clone())
        .run(QUERY, "standard")
        .await
        .unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.calls, 1);
    assert!(backend.0.lock().unwrap().is_empty());
}
#[tokio::test]
async fn cancellation_during_interpretation_does_not_start_a_fallback_solver() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(Pending {
        calls: AtomicUsize::new(0),
        fake_error: false,
    });
    let engine = Engine::with_backend(cfg(), store.clone(), backend.clone());
    let observed = backend.clone();
    let outcome = engine
        .run_until(QUERY, "standard", async move {
            while observed.calls.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    assert_eq!(outcome.status, "cancelled");
    assert_eq!(outcome.calls, 1);
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert!(!store.task(&outcome.task_id).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "interpretation_failure"));
}
