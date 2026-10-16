//! Trusted executive revision decisions, subordinate to runtime resource admission.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod empirical;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub id: String,
    pub version: String,
}
impl Descriptor {
    fn validate(&self) -> Result<()> {
        if self.id.len() > 64
            || !crate::registry::identifier(&self.id)
            || self.version.len() > 64
            || semver::Version::parse(&self.version)?.to_string() != self.version
        {
            bail!("Invalid escalation policy identity/version")
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    IndependentRejection,
    UnverifiedAnswer,
}
#[derive(Clone, Debug, Serialize)]
pub struct Request {
    pub reason: Reason,
    pub mode: String,
    pub cycle: u32,
    pub candidate_sha256: String,
    pub verification_criterion: Option<String>,
    pub configuration_sha256: String,
    pub role_prompts_sha256: String,
    pub runtime_sources_sha256: String,
    pub remaining_calls: usize,
    pub remaining_output_tokens: usize,
    pub remaining_prompt_bytes: usize,
    pub remaining_ms: u64,
    pub remaining_tool_calls: u32,
    pub required_tool_calls: u32,
    pub required_calls: usize,
    pub required_output_tokens: usize,
    pub required_prompt_bytes: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Decision {
    pub allow: bool,
    pub reason_code: String,
}
/// Short, nonblocking trusted-native callbacks only. They cannot change model
/// permissions, deadlines, evidence or resource ceilings and are not sandboxed.
pub trait EscalationPolicy: Send + Sync {
    fn descriptor(&self) -> Descriptor;
    fn decide(&self, request: &Request) -> Result<Decision>;
}
pub struct BudgetPolicy;
impl EscalationPolicy for BudgetPolicy {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "jev-plus.budget".into(),
            version: "1.0.0".into(),
        }
    }
    fn decide(&self, _: &Request) -> Result<Decision> {
        Ok(Decision {
            allow: true,
            reason_code: "within_budget".into(),
        })
    }
}
#[derive(Clone)]
pub(crate) struct Handle {
    backend: Arc<dyn EscalationPolicy>,
    descriptor: Descriptor,
}
impl Default for Handle {
    fn default() -> Self {
        Self {
            backend: Arc::new(BudgetPolicy),
            descriptor: BudgetPolicy.descriptor(),
        }
    }
}
impl Handle {
    pub(crate) fn new(backend: Arc<dyn EscalationPolicy>) -> Result<Self> {
        let descriptor = backend.descriptor();
        descriptor.validate()?;
        Ok(Self {
            backend,
            descriptor,
        })
    }
    pub(crate) fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub(crate) fn decide(&self, request: &Request) -> Result<Decision> {
        let decision = self.backend.decide(request)?;
        if decision.reason_code.len() > 64 || !crate::registry::identifier(&decision.reason_code) {
            bail!("Invalid escalation reason code")
        }
        Ok(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::{Generation, InferenceBackend},
        config::Config,
        engine::Engine,
        store::Store,
    };
    use std::{
        collections::VecDeque,
        future::Future,
        pin::Pin,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };
    struct Policy {
        calls: AtomicUsize,
        allow: bool,
        invalid: bool,
        store: Arc<Store>,
    }
    impl EscalationPolicy for Policy {
        fn descriptor(&self) -> Descriptor {
            Descriptor {
                id: "fixture.escalator".into(),
                version: "1.0.0".into(),
            }
        }
        fn decide(&self, r: &Request) -> Result<Decision> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.store.search("fixture", None, 1)?; // Reentry proves no Store mutex is held.
            assert!(
                r.remaining_calls >= r.required_calls
                    && r.remaining_output_tokens >= r.required_output_tokens
                    && r.remaining_prompt_bytes >= r.required_prompt_bytes
            );
            assert_eq!(r.candidate_sha256.len(), 64);
            Ok(Decision {
                allow: self.allow,
                reason_code: if self.invalid {
                    "x".repeat(65)
                } else {
                    "fixture_decision".into()
                },
            })
        }
    }
    struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
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
                .expect("Unexpected extra inference");
            assert_eq!(role, expected);
            Box::pin(async move {
                Ok(Generation {
                    text: text.into(),
                    model: "fixture/v1".into(),
                    input_tokens: Some(20),
                    output_tokens: Some(10),
                    elapsed_ms: 1.0,
                    queue_wait_ms: None,
                })
            })
        }
    }
    #[tokio::test]
    async fn custom_escalator_can_stop_unverified_or_rejected_work_without_changing_evidence() {
        for structured in [false, true] {
            let store = Arc::new(Store::in_memory().unwrap());
            let policy = Arc::new(Policy {
                calls: AtomicUsize::new(0),
                allow: false,
                invalid: false,
                store: store.clone(),
            });
            let backend = Arc::new(Script(Mutex::new(
                vec![
                    ("planner", "invalid plan"),
                    (
                        "solver",
                        if structured {
                            "[3,2,1]"
                        } else {
                            "unverified answer"
                        },
                    ),
                ]
                .into(),
            )));
            let cfg = Config {
                backend: "openai-local".into(),
                interpret_goals: false,
                max_tokens: 3000,
                ..Config::default()
            };
            let engine = Engine::with_backend(cfg, store.clone(), backend)
                .with_escalation_policy(policy.clone())
                .unwrap();
            let outcome = engine
                .run(
                    if structured {
                        r#"sort:{"values":[3,1,2]}"#
                    } else {
                        "Explain ownership"
                    },
                    "deep",
                )
                .await
                .unwrap();
            assert_eq!(outcome.calls, 2);
            assert_eq!(outcome.cycles, 1);
            assert_eq!(outcome.stop_reason, "escalation_declined");
            assert_ne!(outcome.status, "completed");
            assert_eq!(policy.calls.load(Ordering::Acquire), 1);
            let trace = store.task(&outcome.task_id).unwrap();
            let events = trace["events"].as_array().unwrap();
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e["kind"] == "escalation_policy")
                    .count(),
                1
            );
            assert!(events.iter().any(|e| e["kind"] == "escalation_intent"));
            assert!(events.iter().any(|e| e["kind"] == "escalation_decision"
                && e["payload"]["decision"]["allow"] == false));
            assert!(!events.iter().any(|e| e["kind"] == "reasoning"
                && matches!(
                    e["payload"]["node"]["role"].as_str(),
                    Some("critic" | "reflector")
                )));
        }
    }
    #[tokio::test]
    async fn kernel_budget_refusal_precedes_custom_escalator_and_invalid_decisions_fail_closed() {
        for enough in [false, true] {
            let store = Arc::new(Store::in_memory().unwrap());
            let policy = Arc::new(Policy {
                calls: AtomicUsize::new(0),
                allow: true,
                invalid: true,
                store: store.clone(),
            });
            let backend = Arc::new(Script(Mutex::new(
                vec![("planner", "invalid plan"), ("solver", "answer")].into(),
            )));
            let cfg = Config {
                backend: "openai-local".into(),
                interpret_goals: false,
                max_calls: if enough { 6 } else { 2 },
                max_tokens: 3000,
                ..Config::default()
            };
            let outcome = Engine::with_backend(cfg, store, backend)
                .with_escalation_policy(policy.clone())
                .unwrap()
                .run("Explain ownership", "deep")
                .await
                .unwrap();
            assert_eq!(outcome.calls, 2);
            assert_ne!(outcome.status, "completed");
            assert_eq!(policy.calls.load(Ordering::Acquire), usize::from(enough));
            if enough {
                assert_eq!(outcome.status, "failed");
            }
        }
    }
    #[tokio::test]
    async fn audited_allowance_preserves_policy_identity_and_audit_failure_blocks_dispatch() {
        for fail in [None, Some("escalation_intent"), Some("escalation_decision")] {
            let store = Arc::new(Store::in_memory().unwrap());
            if let Some(kind) = fail {
                store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER reject_escalation BEFORE INSERT ON events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'fixture escalation audit failure'); END;")).unwrap();
            }
            let policy = Arc::new(Policy {
                calls: AtomicUsize::new(0),
                allow: true,
                invalid: false,
                store: store.clone(),
            });
            let backend = Arc::new(Script(Mutex::new(
                vec![
                    ("planner", "invalid plan"),
                    ("solver", "answer"),
                    (
                        "critic",
                        r#"{"revise":false,"reason":"No useful revision"}"#,
                    ),
                ]
                .into(),
            )));
            let original = Engine::with_backend(
                Config {
                    backend: "openai-local".into(),
                    interpret_goals: false,
                    max_tokens: 3000,
                    ..Config::default()
                },
                store.clone(),
                backend,
            );
            let configured = original
                .clone()
                .with_escalation_policy(policy.clone())
                .unwrap();
            assert_eq!(
                original.escalation_policy_descriptor().id,
                "jev-plus.budget"
            );
            assert_eq!(
                configured.escalation_policy_descriptor().id,
                "fixture.escalator"
            );
            let outcome = configured.run("Explain ownership", "deep").await.unwrap();
            assert_eq!(outcome.calls, if fail.is_some() { 2 } else { 3 });
            assert_eq!(
                policy.calls.load(Ordering::Acquire),
                usize::from(fail != Some("escalation_intent"))
            );
            if fail.is_some() {
                assert_eq!(outcome.status, "failed");
            } else {
                assert_eq!(outcome.stop_reason, "no_useful_revision");
            }
        }
    }
    #[tokio::test]
    async fn independent_success_and_recursive_ablation_do_not_invoke_custom_escalation() {
        for baseline in [false, true] {
            let store = Arc::new(Store::in_memory().unwrap());
            let policy = Arc::new(Policy {
                calls: AtomicUsize::new(0),
                allow: false,
                invalid: true,
                store: store.clone(),
            });
            let script = if baseline {
                vec![
                    ("solver", "answer"),
                    (
                        "solver",
                        r#"{"revise":false,"reason":"No useful revision"}"#,
                    ),
                ]
            } else {
                vec![("planner", "invalid plan"), ("solver", "[1,2,3]")]
            };
            let backend = Arc::new(Script(Mutex::new(script.into())));
            let engine = Engine::with_backend(
                Config {
                    backend: "openai-local".into(),
                    interpret_goals: false,
                    max_tokens: 3000,
                    ..Config::default()
                },
                store.clone(),
                backend,
            )
            .with_escalation_policy(policy.clone())
            .unwrap();
            let outcome = if baseline {
                engine
                    .run_profile(
                        "Explain ownership",
                        "deep",
                        crate::engine::ExecutionProfile::Recursive,
                    )
                    .await
                    .unwrap()
            } else {
                engine
                    .run(r#"sort:{"values":[3,1,2]}"#, "deep")
                    .await
                    .unwrap()
            };
            assert_eq!(policy.calls.load(Ordering::Acquire), 0);
            assert_eq!(outcome.calls, 2);
            assert_eq!(
                outcome.status,
                if baseline { "unverified" } else { "completed" }
            );
            if baseline {
                assert!(store.task(&outcome.task_id).unwrap()["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["kind"] == "escalation_policy"
                        && e["payload"]["policy"]["id"] == "jev-plus.budget"));
            }
        }
    }
    #[tokio::test]
    async fn rejected_answer_without_future_verification_budget_never_buys_another_solver() {
        let store = Arc::new(Store::in_memory().unwrap());
        let policy = Arc::new(Policy {
            calls: AtomicUsize::new(0),
            allow: true,
            invalid: false,
            store: store.clone(),
        });
        let backend = Arc::new(Script(Mutex::new(
            vec![("planner", "invalid plan"), ("solver", "[3,2,1]")].into(),
        )));
        let outcome = Engine::with_backend(
            Config {
                backend: "openai-local".into(),
                interpret_goals: false,
                max_tokens: 3000,
                max_tool_calls: 1,
                ..Config::default()
            },
            store,
            backend,
        )
        .with_escalation_policy(policy.clone())
        .unwrap()
        .run_profile(
            r#"sort:{"values":[3,1,2]}"#,
            "deep",
            crate::engine::ExecutionProfile::Collective,
        )
        .await
        .unwrap();
        assert_eq!(outcome.calls, 2);
        assert_eq!(outcome.cycles, 1);
        assert_eq!(outcome.resources.tool_calls, 1);
        assert_eq!(outcome.stop_reason, "insufficient_revision_budget");
        assert_ne!(outcome.status, "completed");
        assert_eq!(policy.calls.load(Ordering::Acquire), 0);
    }
    #[test]
    fn policy_versions_and_reason_codes_are_bounded() {
        for (id, version) in [
            ("bad/id", "1.0.0"),
            ("valid.id", "01.0.0"),
            ("valid.id", "not-semver"),
        ] {
            assert!(Descriptor {
                id: id.into(),
                version: version.into()
            }
            .validate()
            .is_err());
        }
        assert!(Descriptor {
            id: "valid.id".into(),
            version: "1.0.0".into()
        }
        .validate()
        .is_ok());
    }
}
