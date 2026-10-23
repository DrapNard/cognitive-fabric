use anyhow::{bail, Result};
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    fabric::modules::{Capability, Descriptor, Limits, Module, ModuleFuture, Request, Response},
    store::Store,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};

struct Script(&'static str);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            Ok(Generation {
                text: self.0.into(),
                model: "typed-reasoning-fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}

struct Diagnostic {
    calls: Arc<Mutex<Vec<Capability>>>,
    claim: bool,
}
impl Module for Diagnostic {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "fixture.reasoning".into(),
            version: "1.0.0".into(),
            capabilities: [
                Capability::SortingVerify,
                Capability::OrderingVerify,
                Capability::PathVerify,
                Capability::ConstraintsSolve,
            ]
            .into(),
        }
    }
    fn execute<'a>(&'a self, _: &'a str, _: Instant, request: Request) -> ModuleFuture<'a> {
        self.calls.lock().unwrap().push(request.capability());
        Box::pin(async move {
            Ok(match request {
                Request::Sort { .. } => Response::SortDiagnostic {
                    claimed_valid: self.claim,
                },
                Request::DependencyOrder { .. } => Response::OrderingDiagnostic {
                    claimed_valid: self.claim,
                },
                Request::ShortestPath { .. } => Response::PathDiagnostic {
                    claimed_valid: self.claim,
                },
                Request::FiniteDomain { .. } => Response::FiniteAssignment {
                    values: BTreeMap::from([("x".into(), 2)]),
                },
                _ => bail!("Unexpected diagnostic request"),
            })
        })
    }
}

struct Fixture {
    engine: Engine,
    store: Arc<Store>,
    calls: Arc<Mutex<Vec<Capability>>>,
    home: PathBuf,
}
impl Fixture {
    fn new(answer: &'static str, typed: bool, finite: bool, claim: bool) -> Self {
        let home = std::env::temp_dir().join(format!(
            "cogfabric-typed-reasoning-{}",
            uuid::Uuid::new_v4()
        ));
        let mut cfg = Config {
            home: home.clone(),
            backend: "openai-local".into(),
            interpret_goals: false,
            max_calls: 4,
            max_cycles: 1,
            max_tokens: 1024,
            max_tool_calls: 8,
            ..Config::default()
        };
        if finite {
            cfg.finite_search =
                Some(cogfabric::reasoning::finite_search::Settings { max_nodes: 64 });
        }
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        let engine = Engine::with_backend(cfg, store.clone(), Arc::new(Script(answer)));
        let calls = Arc::new(Mutex::new(Vec::new()));
        if typed {
            engine
                .modules
                .register(
                    Arc::new(Diagnostic {
                        calls: calls.clone(),
                        claim,
                    }),
                    Limits::default(),
                )
                .unwrap();
        }
        Self {
            engine,
            store,
            calls,
            home,
        }
    }
    fn reservations(&self, task: &str, capability: &str) -> usize {
        self.store.task(task).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| {
                event["kind"] == "tool_reservation" && event["payload"]["capability"] == capability
            })
            .count()
    }
    fn events(&self, task: &str) -> Vec<Value> {
        self.store.task(task).unwrap()["events"]
            .as_array()
            .unwrap()
            .clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

#[tokio::test]
async fn all_typed_diagnostics_use_one_original_checker_charge_and_cannot_veto_compiled_proof() {
    for (query, answer, capability) in [
        (
            r#"sort:{"values":[3,1,2]}"#,
            "[1,2,3]",
            Capability::SortingVerify,
        ),
        (
            r#"order:{"nodes":["a","b"],"before":[["a","b"]]}"#,
            r#"["a","b"]"#,
            Capability::OrderingVerify,
        ),
        (
            r#"path:{"nodes":["a","b"],"edges":[["a","b"]],"start":"a","goal":"b"}"#,
            r#"["a","b"]"#,
            Capability::PathVerify,
        ),
    ] {
        let fixture = Fixture::new(answer, true, false, false);
        let outcome = fixture.engine.run(query, "fast").await.unwrap();
        assert_eq!(outcome.status, "completed");
        assert_eq!(*fixture.calls.lock().unwrap(), vec![capability]);
        assert_eq!(
            fixture.reservations(&outcome.task_id, capability.as_str()),
            1
        );
        assert!(fixture
            .events(&outcome.task_id)
            .iter()
            .any(|event| event["kind"] == "typed_verifier_result"
                && event["payload"]["claim_valid"] == false
                && event["payload"]["compiled_checker_accepted"] == true));
    }
}

#[tokio::test]
async fn a_lying_typed_diagnostic_cannot_accept_an_invalid_candidate() {
    let fixture = Fixture::new("[3,1,2]", true, false, true);
    let outcome = fixture
        .engine
        .run(r#"sort:{"values":[3,1,2]}"#, "fast")
        .await
        .unwrap();
    assert_ne!(outcome.status, "completed");
    assert_ne!(outcome.status, "failed");
    assert_eq!(
        *fixture.calls.lock().unwrap(),
        vec![Capability::SortingVerify]
    );
    assert!(fixture
        .events(&outcome.task_id)
        .iter()
        .any(|event| event["kind"] == "typed_verifier_result"
            && event["payload"]["claim_valid"] == true
            && event["payload"]["compiled_checker_accepted"] == false));
}

#[tokio::test]
async fn malformed_model_json_is_rejected_without_aborting_default_or_typed_tasks() {
    for typed in [false, true] {
        let fixture = Fixture::new("not JSON", typed, false, true);
        let outcome = fixture
            .engine
            .run(r#"sort:{"values":[3,1,2]}"#, "fast")
            .await
            .unwrap();
        assert_ne!(outcome.status, "completed");
        assert_ne!(outcome.status, "failed");
        assert!(fixture.calls.lock().unwrap().is_empty());
        assert_eq!(fixture.reservations(&outcome.task_id, "sorting.verify"), 1);
        assert!(fixture
            .events(&outcome.task_id)
            .iter()
            .any(|event| event["kind"] == "verification" && event["payload"]["verified"] == false));
    }
}

#[tokio::test]
async fn typed_finite_solver_requires_explicit_operator_setting_and_preserves_reservation() {
    let query = r#"csp:{"variables":[{"id":"x","domain":[1,2]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":2}]}"#;
    for finite in [false, true] {
        let fixture = Fixture::new(r#"{"x":2}"#, true, finite, true);
        let outcome = fixture.engine.run(query, "fast").await.unwrap();
        assert_eq!(outcome.status, "completed");
        let calls = fixture.calls.lock().unwrap().clone();
        if finite {
            assert_eq!(calls, vec![Capability::ConstraintsSolve]);
            assert_eq!(
                fixture.reservations(&outcome.task_id, "constraints.solve"),
                1
            );
            assert_eq!(outcome.calls, 0);
        } else {
            assert!(calls.is_empty());
            assert_eq!(
                fixture.reservations(&outcome.task_id, "constraints.solve"),
                0
            );
        }
    }
}
