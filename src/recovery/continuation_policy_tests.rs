use super::*;
use crate::escalation::{Decision, Descriptor, EscalationPolicy, Request};
use crate::verification::VerifierProvider;
use std::time::Instant;
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const ORIGINAL_QUERY: &str = r#"sort:{"values":[3,1,2]}"#;

type RecordedVerification = (String, String, String, String, Instant);

struct Verifier {
    descriptor: crate::verification::VerifierDescriptor,
    calls: AtomicUsize,
    requests: std::sync::Mutex<Vec<RecordedVerification>>,
}

impl Verifier {
    fn new(id: &str) -> Self {
        Self {
            descriptor: crate::verification::VerifierDescriptor {
                id: id.into(),
                version: "1.0.0".into(),
                implementation_sha256: "a".repeat(64),
            },
            calls: AtomicUsize::new(0),
            requests: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl crate::verification::VerifierProvider for Verifier {
    fn descriptor(&self) -> crate::verification::VerifierDescriptor {
        self.descriptor.clone()
    }

    fn verify(
        &self,
        request: crate::verification::VerificationRequest,
    ) -> crate::verification::VerificationFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push((
            request.criterion,
            request.objective,
            request.objective_sha256,
            request.answer_sha256,
            request.deadline,
        ));
        Box::pin(async { Ok(crate::verification::VerificationClaim::accepted()) })
    }
}

struct SolverReplies(std::sync::Mutex<std::collections::VecDeque<&'static str>>);

impl crate::backend::InferenceBackend for SolverReplies {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<crate::backend::Generation>> + Send + 'a>> {
        assert_eq!(role, "solver");
        let text = self.0.lock().unwrap().pop_front().unwrap();
        Box::pin(async move {
            Ok(crate::backend::Generation {
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

fn pin_verifier(store: &Store, task: &str, provider: &Verifier) -> crate::verification::Criterion {
    let query: String = store
        .lock()
        .unwrap()
        .query_row("SELECT query FROM tasks WHERE id=?1", [task], |row| {
            row.get(0)
        })
        .unwrap();
    let criterion = crate::verification::Criterion::from_query(&query)
        .unwrap()
        .unwrap();
    store
        .event(
            task,
            "verification_provider_snapshot",
            "security-kernel",
            serde_json::json!({
                "format_version":1,
                "provider":provider.descriptor(),
                "criterion":criterion.name(),
                "objective_sha256":crate::store::sha256(query.as_bytes()),
                "compiled_checker_required":true,
                "permissions_granted":false,
                "implementation_attested":false
            }),
        )
        .unwrap();
    criterion
}

fn append_unresolved_verifier_intent(
    store: &Store,
    task: &str,
    provider: &Verifier,
    criterion: &crate::verification::Criterion,
) {
    let query: String = store
        .lock()
        .unwrap()
        .query_row("SELECT query FROM tasks WHERE id=?1", [task], |row| {
            row.get(0)
        })
        .unwrap();
    store
        .event(
            task,
            "tool_reservation",
            "security-kernel",
            serde_json::json!({
                "format_version":1,
                "invocation":1,
                "actor":"jev-plus",
                "capability":criterion.capability()
            }),
        )
        .unwrap();
    store
        .event(
            task,
            "verification_provider_intent",
            "security-kernel",
            serde_json::json!({
                "format_version":1,
                "provider":provider.descriptor(),
                "criterion":criterion.name(),
                "objective_sha256":crate::store::sha256(query.as_bytes()),
                "answer_sha256":"b".repeat(64),
                "remaining_ms":30000,
                "compiled_checker_required":true,
                "permissions_granted":false
            }),
        )
        .unwrap();
}

struct Policy {
    allow: bool,
    version: &'static str,
    descriptor_calls: AtomicUsize,
    decisions: AtomicUsize,
    store: Arc<Store>,
}
impl Policy {
    fn new(store: Arc<Store>, allow: bool) -> Self {
        Self {
            allow,
            version: "1.0.0",
            descriptor_calls: AtomicUsize::new(0),
            decisions: AtomicUsize::new(0),
            store,
        }
    }
}
impl EscalationPolicy for Policy {
    fn descriptor(&self) -> Descriptor {
        self.descriptor_calls.fetch_add(1, Ordering::SeqCst);
        self.store.search("fixture", None, 1).unwrap(); // Descriptor runs before Store locking.
        Descriptor {
            id: if self.allow {
                "fixture.restored_allow"
            } else {
                "fixture.restored_deny"
            }
            .into(),
            version: self.version.into(),
        }
    }
    fn decide(&self, request: &Request) -> Result<Decision> {
        self.decisions.fetch_add(1, Ordering::SeqCst);
        self.store.search("fixture", None, 1)?;
        assert_eq!(
            request.verification_criterion.as_deref(),
            Some("integer_sort/v1")
        );
        assert_eq!(request.required_calls, 1);
        assert_eq!(request.required_tool_calls, 1);
        Ok(Decision {
            allow: self.allow,
            reason_code: "restored_operator_policy".into(),
        })
    }
}
fn persistent() -> (Arc<Store>, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("fabric-policy-resume-{}", uuid::Uuid::new_v4()));
    (Arc::new(Store::open(&root.join("state.db")).unwrap()), root)
}

#[tokio::test]
async fn custom_policy_preview_matches_all_snapshots_and_is_read_only() {
    for mutation in 0..7 {
        let (store, root) = persistent();
        let policy = Arc::new(Policy::new(store.clone(), false));
        let descriptor = policy.descriptor();
        let (store, task) =
            super::tests::fixture_in_with_policy(store, false, Some(descriptor.clone())).await;
        let before = store.task(&task).unwrap();
        assert!(preview(&store, &task).is_err());
        let report = preview_with_escalation_policy(&store, &task, policy.clone()).unwrap();
        assert_eq!(
            report["escalation_policy"],
            serde_json::to_value(&descriptor).unwrap()
        );
        assert_eq!(report["resume_authorized"], false);
        assert_eq!(store.task(&task).unwrap(), before);
        assert_eq!(policy.decisions.load(Ordering::Acquire), 0);
        {
            let db = store.lock().unwrap();
            match mutation {
                0 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.policy.version','2.0.0') WHERE task_id=?1 AND kind='escalation_policy'",[&task]).unwrap();
                }
                1 => {
                    db.execute("UPDATE events SET source='foreign' WHERE task_id=?1 AND kind='escalation_policy'",[&task]).unwrap();
                }
                2 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.permissions_granted',json('true')) WHERE task_id=?1 AND kind='escalation_policy'",[&task]).unwrap();
                }
                3 => {
                    db.execute(
                        "DELETE FROM events WHERE task_id=?1 AND kind='escalation_policy'",
                        [&task],
                    )
                    .unwrap();
                }
                4 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.extra','ignored') WHERE task_id=?1 AND kind='escalation_policy'",[&task]).unwrap();
                }
                5 => {
                    let raw = format!(
                        r#"{{"format_version":1,"policy":{{"id":"foreign.policy","id":"{}","version":"1.0.0"}},"permissions_granted":false,"implementation_attested":false}}"#,
                        descriptor.id
                    );
                    db.execute(
                    "UPDATE events SET payload=?2 WHERE task_id=?1 AND kind='escalation_policy'",
                    rusqlite::params![task, raw],
                )
                .unwrap();
                }
                _ => {}
            }
        }
        if mutation == 6 {
            use crate::escalation::BudgetPolicy;
            store.event(&task,"escalation_policy","jev-plus",json!({"format_version":1,"policy":BudgetPolicy.descriptor(),"permissions_granted":false,"implementation_attested":false})).unwrap();
        }
        let before = store.task(&task).unwrap();
        assert!(
            preview_with_escalation_policy(&store, &task, policy.clone()).is_err(),
            "Accepted mutation {mutation}"
        );
        assert!(resume_with_escalation_policy(
            store.clone(),
            &task,
            true,
            policy.clone(),
            std::future::pending()
        )
        .await
        .is_err());
        assert_eq!(store.task(&task).unwrap(), before);
        assert_eq!(policy.decisions.load(Ordering::Acquire), 0);
        drop(policy);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn explicit_restoration_cannot_invoke_callbacks_without_approval_or_take_a_live_task() {
    let (store, root) = persistent();
    let policy = Arc::new(Policy::new(store.clone(), false));
    let (store, task) =
        super::tests::fixture_in_with_policy(store, false, Some(policy.descriptor())).await;
    let descriptors = policy.descriptor_calls.load(Ordering::Acquire);
    let before = store.task(&task).unwrap();
    assert!(resume_with_escalation_policy(
        store.clone(),
        &task,
        false,
        policy.clone(),
        std::future::pending()
    )
    .await
    .is_err());
    assert!(resume_with_escalation_policy(
        store.clone(),
        "not-a-task",
        true,
        policy.clone(),
        std::future::pending()
    )
    .await
    .is_err());
    assert_eq!(policy.descriptor_calls.load(Ordering::Acquire), descriptors);
    let lease = store.execution_lease().unwrap();
    assert!(preview_with_escalation_policy(&store, &task, policy.clone()).is_err());
    assert!(resume_with_escalation_policy(
        store.clone(),
        &task,
        true,
        policy.clone(),
        std::future::pending()
    )
    .await
    .is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    assert_eq!(policy.decisions.load(Ordering::Acquire), 0);
    drop(lease);
    drop(policy);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

async fn response(listener: tokio::net::TcpListener, answers: Vec<&'static str>) {
    for answer in answers {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 131072);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|v| v.parse::<usize>().ok())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    let body: Value =
                        serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                    assert_eq!(body["model"], "local-model");
                    break;
                }
            }
        }
        let body=json!({"model":"fixture/v1","choices":[{"message":{"content":answer}}],"usage":{"prompt_tokens":20,"completion_tokens":10}}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
    }
}

#[tokio::test]
async fn public_custom_policy_continuation_preserves_decisions_original_charges_and_http_calls() {
    for allow in [false, true] {
        let (store, root) = persistent();
        let policy = Arc::new(Policy::new(store.clone(), allow));
        let descriptor = policy.descriptor();
        let (store, task) =
            super::tests::fixture_in_with_policy(store, false, Some(descriptor.clone())).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",rusqlite::params![task,endpoint]).unwrap();
        let mut answers = vec!["remaining advice", "[3,2,1]"];
        if allow {
            answers.push("[1,2,3]");
        }
        let server = tokio::spawn(response(listener, answers));
        let report = tokio::time::timeout(
            Duration::from_secs(10),
            resume_with_escalation_policy(
                store.clone(),
                &task,
                true,
                policy.clone(),
                std::future::pending(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(report["new_calls"], if allow { 3 } else { 2 });
        assert_eq!(
            report["status"],
            if allow { "completed" } else { "rejected" }
        );
        assert_eq!(report["verified"], allow);
        assert_eq!(report["verification_attempted"], true);
        assert_eq!(
            report["escalation_policy"],
            serde_json::to_value(&descriptor).unwrap()
        );
        assert_eq!(policy.decisions.load(Ordering::Acquire), 1);
        let audit = store.inference_audit(&task).unwrap();
        assert_eq!(audit["reserved_calls"], if allow { 5 } else { 4 });
        assert_eq!(
            audit["reserved_output_tokens"],
            if allow { 1408 } else { 1024 }
        );
        let trace = store.task(&task).unwrap();
        let events = trace["events"].as_array().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e["kind"] == "execution_settings")
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e["kind"] == "escalation_policy")
                .count(),
            2
        );
        assert!(events
            .iter()
            .filter(|e| e["kind"] == "escalation_policy")
            .all(|e| e["payload"]["policy"] == serde_json::to_value(&descriptor).unwrap()));
        drop(policy);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn custom_policy_admission_audit_failure_rolls_back_without_contacting_http() {
    let (store, root) = persistent();
    let policy = Arc::new(Policy::new(store.clone(), false));
    let (store, task) =
        super::tests::fixture_in_with_policy(store, false, Some(policy.descriptor())).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.configuration.api_base',?2) WHERE task_id=?1 AND kind='execution_settings'",rusqlite::params![task,endpoint]).unwrap();
    store.lock().unwrap().execute_batch("CREATE TRIGGER refuse_custom_resume BEFORE INSERT ON events WHEN NEW.kind='continuation' BEGIN SELECT RAISE(ABORT,'fixture audit denial'); END;").unwrap();
    let before = store.task(&task).unwrap();
    assert!(resume_with_escalation_policy(
        store.clone(),
        &task,
        true,
        policy.clone(),
        std::future::pending()
    )
    .await
    .is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    assert_eq!(policy.decisions.load(Ordering::Acquire), 0);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    drop(policy);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn legacy_records_only_use_the_runtime_owned_builtin_compatibility_path() {
    let (store, root) = persistent();
    let (store, task) = super::tests::fixture_in_with_policy(store, false, None).await;
    let before = store.task(&task).unwrap();
    assert!(preview(&store, &task).is_ok());
    // A caller-supplied native handle cannot obtain legacy compatibility merely
    // by declaring the built-in name, even when it is a real BudgetPolicy value.
    let supplied = Arc::new(crate::escalation::BudgetPolicy);
    assert!(preview_with_escalation_policy(&store, &task, supplied.clone()).is_err());
    assert!(resume_with_escalation_policy(
        store.clone(),
        &task,
        true,
        supplied,
        std::future::pending()
    )
    .await
    .is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn empirical_policy_can_be_reconstructed_only_from_the_original_study_and_thresholds() {
    use crate::escalation::empirical::{
        configuration_fingerprint, runtime_sources_fingerprint, EmpiricalPolicy, Thresholds,
    };
    let (store, root) = persistent();
    let cfg = Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_tokens: 3000,
        ..Config::default()
    };
    // Invented study used solely to test restoration and content identity.
    let data = json!({"format_version":1,"mode":"deep","criterion":"integer_sort/v1","cycle":1,
        "configuration_sha256":configuration_fingerprint(&cfg).unwrap(),
        "role_prompts_sha256":crate::backend::runtime_role_prompts_sha256().unwrap(),
        "runtime_sources_sha256":runtime_sources_fingerprint(),"dataset_sha256":"a".repeat(64),
        "source":"Synthetic restoration fixture, not a measured quality study","license":"CC0-1.0","reviewer":"fixture.operator",
        "held_out_operator_attested":true,"independent_outcomes_operator_attested":true,"includes_failed_attempts_operator_attested":true,
        "observations":[{"task_id":uuid::Uuid::new_v4().to_string(),"verified":true,"model_calls":1,"reserved_output_tokens":384,"tool_calls":1,"prompt_bytes":1000,"elapsed_ms":1}]
    });
    let bytes = serde_json::to_vec(&data).unwrap();
    let thresholds = Thresholds {
        min_observations: 1,
        min_success_basis_points: 10_000,
        max_output_tokens_per_success: 384,
        max_prompt_bytes_per_success: 1000,
        max_milliseconds_per_success: 1,
    };
    let policy = EmpiricalPolicy::from_json(&bytes, thresholds.clone()).unwrap();
    let (store, task) =
        super::tests::fixture_in_with_policy(store, false, Some(policy.descriptor())).await;
    let restored = Arc::new(EmpiricalPolicy::from_json(&bytes, thresholds.clone()).unwrap());
    let before = store.task(&task).unwrap();
    assert!(preview(&store, &task).is_err());
    assert!(preview_with_escalation_policy(&store, &task, restored).is_ok());
    let mut changed = thresholds;
    changed.max_milliseconds_per_success = 2;
    let altered = Arc::new(EmpiricalPolicy::from_json(&bytes, changed).unwrap());
    assert!(preview_with_escalation_policy(&store, &task, altered).is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

struct Original(std::sync::Mutex<std::collections::VecDeque<(&'static str, &'static str)>>);
impl crate::backend::InferenceBackend for Original {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::backend::Generation>> + Send + 'a>,
    > {
        let (expected, text) = self.0.lock().unwrap().pop_front().unwrap();
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(crate::backend::Generation {
                text: text.into(),
                model: "fixture/v1".into(),
                input_tokens: Some(10),
                output_tokens: Some(10),
                elapsed_ms: 1.0,
                queue_wait_ms: None,
            })
        })
    }
}

#[tokio::test]
async fn restored_custom_revision_requires_the_original_durable_allowance_before_http() {
    for mutation in 0..10 {
        let (store, root) = persistent();
        let policy = Arc::new(Policy::new(store.clone(), true));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let cfg = Config {
            backend: "openai-local".into(),
            api_base: endpoint,
            interpret_goals: false,
            max_tokens: 3000,
            ..Config::default()
        };
        let script = Arc::new(Original(std::sync::Mutex::new(
            vec![
                ("planner", "invalid plan"),
                ("solver", "[3,2,1]"),
                ("solver", "[1,2,3]"),
            ]
            .into(),
        )));
        let engine = crate::engine::Engine::with_backend(cfg, store.clone(), script)
            .with_escalation_policy(policy.clone())
            .unwrap();
        let original = engine
            .run_profile(
                r#"sort:{"values":[3,1,2]}"#,
                "deep",
                crate::engine::ExecutionProfile::Collective,
            )
            .await
            .unwrap();
        assert_eq!(original.status, "completed");
        let task = original.task_id;
        // Retain the real audited prefix through the second ready-solver boundary,
        // synthesizing interruption before its next charged dispatch.
        {
            let db = store.lock().unwrap();
            let boundary: u64 = db
                .query_row(
                    "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary'",
                    [&task],
                    |r| r.get(0),
                )
                .unwrap();
            db.execute(
                "DELETE FROM events WHERE task_id=?1 AND id>?2",
                rusqlite::params![task, boundary],
            )
            .unwrap();
            db.execute(
                "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
                [&task],
            )
            .unwrap();
            match mutation {
                1 => {
                    db.execute(
                        "DELETE FROM events WHERE task_id=?1 AND kind='escalation_intent'",
                        [&task],
                    )
                    .unwrap();
                }
                2 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.decision.allow',json('false')) WHERE task_id=?1 AND kind='escalation_decision'",[&task]).unwrap();
                }
                3 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.request.configuration_sha256',?2) WHERE task_id=?1 AND kind='escalation_intent'",rusqlite::params![task,"f".repeat(64)]).unwrap();
                }
                4 => {
                    db.execute("UPDATE events SET source='foreign' WHERE task_id=?1 AND kind='escalation_decision'",[&task]).unwrap();
                }
                5 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.request.runtime_sources_sha256',?2) WHERE task_id=?1 AND kind='escalation_intent'",rusqlite::params![task,"f".repeat(64)]).unwrap();
                }
                6 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.request.required_calls',3) WHERE task_id=?1 AND kind='escalation_intent'",[&task]).unwrap();
                }
                7 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.candidate_sha256',?2) WHERE task_id=?1 AND kind='escalation_decision'",rusqlite::params![task,"f".repeat(64)]).unwrap();
                }
                8 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.request.required_prompt_bytes',0) WHERE task_id=?1 AND kind='escalation_intent'",[&task]).unwrap();
                }
                9 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.request.remaining_output_tokens',0) WHERE task_id=?1 AND kind='escalation_intent'",[&task]).unwrap();
                }
                _ => {}
            }
        }
        let before = store.task(&task).unwrap();
        if mutation == 0 {
            assert!(preview_with_escalation_policy(&store, &task, policy.clone()).is_ok());
            assert_eq!(store.task(&task).unwrap(), before);
            let server = tokio::spawn(response(listener, vec!["[1,2,3]"]));
            let report = tokio::time::timeout(
                Duration::from_secs(10),
                resume_with_escalation_policy(
                    store.clone(),
                    &task,
                    true,
                    policy.clone(),
                    std::future::pending(),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(report["new_calls"], 1);
            assert_eq!(report["status"], "completed");
            assert_eq!(store.inference_audit(&task).unwrap()["reserved_calls"], 3);
        } else {
            assert!(
                preview_with_escalation_policy(&store, &task, policy.clone()).is_err(),
                "Accepted mutation {mutation}"
            );
            assert!(resume_with_escalation_policy(
                store.clone(),
                &task,
                true,
                policy.clone(),
                std::future::pending()
            )
            .await
            .is_err());
            assert_eq!(store.task(&task).unwrap(), before);
            assert!(
                tokio::time::timeout(Duration::from_millis(20), listener.accept())
                    .await
                    .is_err()
            );
        }
        assert_eq!(
            policy.decisions.load(Ordering::Acquire),
            1,
            "Already recorded allowance must not rerun"
        );
        drop(engine);
        drop(policy);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn ready_cancellation_preserves_custom_policy_and_original_charges_without_inference() {
    let (store, root) = persistent();
    let policy = Arc::new(Policy::new(store.clone(), false));
    let descriptor = policy.descriptor();
    let (store, task) =
        super::tests::fixture_in_with_policy(store, false, Some(descriptor.clone())).await;
    let before = store.inference_audit(&task).unwrap();
    let report =
        resume_with_escalation_policy(store.clone(), &task, true, policy.clone(), async {})
            .await
            .unwrap();
    assert_eq!(report["new_calls"], 0);
    assert_eq!(report["status"], "interrupted");
    assert_eq!(report["verified"], false);
    assert_eq!(
        report["escalation_policy"],
        serde_json::to_value(&descriptor).unwrap()
    );
    let after = store.inference_audit(&task).unwrap();
    for field in [
        "reserved_calls",
        "reserved_output_tokens",
        "charged_prompt_bytes",
    ] {
        assert_eq!(after[field], before[field]);
    }
    assert_eq!(policy.decisions.load(Ordering::Acquire), 0);
    drop(policy);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn restored_unstructured_revision_validates_the_original_whole_chain_prompt_bound() {
    struct RevisionPolicy;
    impl EscalationPolicy for RevisionPolicy {
        fn descriptor(&self) -> Descriptor {
            Descriptor {
                id: "fixture.unstructured_revision".into(),
                version: "1.0.0".into(),
            }
        }
        fn decide(&self, request: &Request) -> Result<Decision> {
            assert!(request.verification_criterion.is_none());
            Ok(Decision {
                allow: true,
                reason_code: "fixture_reviewed_chain".into(),
            })
        }
    }
    let (store, root) = persistent();
    let policy = Arc::new(RevisionPolicy);
    let cfg = Config {
        backend: "openai-local".into(),
        interpret_goals: false,
        max_tokens: 3000,
        ..Config::default()
    };
    let script = Arc::new(Original(std::sync::Mutex::new(
        vec![
            ("planner", "invalid plan"),
            ("solver", "original candidate"),
            (
                "critic",
                r#"{"revise":true,"reason":"Missing explanation"}"#,
            ),
            ("reflector", "Add the missing explanation"),
            ("solver", "revised candidate"),
            (
                "critic",
                r#"{"revise":false,"reason":"No useful revision"}"#,
            ),
        ]
        .into(),
    )));
    let engine = crate::engine::Engine::with_backend(cfg, store.clone(), script)
        .with_escalation_policy(policy.clone())
        .unwrap();
    let outcome = engine
        .run_profile(
            "Explain Rust ownership",
            "deep",
            crate::engine::ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(outcome.status, "unverified");
    let task = outcome.task_id;
    {
        let db = store.lock().unwrap();
        let boundary: u64 = db
            .query_row(
                "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary'",
                [&task],
                |r| r.get(0),
            )
            .unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND id>?2",
            rusqlite::params![task, boundary],
        )
        .unwrap();
        db.execute(
            "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
            [&task],
        )
        .unwrap();
    }
    let before = store.task(&task).unwrap();
    assert!(preview_with_escalation_policy(&store, &task, policy.clone()).is_ok());
    assert_eq!(store.task(&task).unwrap(), before);
    store.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.request.required_prompt_bytes',json_extract(payload,'$.request.required_prompt_bytes')+1) WHERE task_id=?1 AND kind='escalation_intent'",[&task]).unwrap();
    assert!(preview_with_escalation_policy(&store, &task, policy).is_err());
    drop(engine);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn continuation_never_silently_discards_an_original_shared_resource_pool() {
    let (store, root) = persistent();
    let (store, task) = super::tests::fixture_in_with_policy(store, false, None).await;
    assert!(preview(&store, &task).is_ok());
    store
        .event(
            &task,
            "resource_intent",
            "jev-plus",
            json!({"format_version":1,"pool_id":uuid::Uuid::new_v4().to_string()}),
        )
        .unwrap();
    let before = store.task(&task).unwrap();
    assert!(preview(&store, &task)
        .unwrap_err()
        .to_string()
        .contains("resource pool"));
    assert!(resume(store.clone(), &task, true, std::future::pending())
        .await
        .unwrap_err()
        .to_string()
        .contains("resource pool"));
    assert_eq!(store.task(&task).unwrap(), before);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn combined_policy_preview_restores_exact_verifier_identity_without_callbacks_or_writes() {
    let (store, root) = persistent();
    let policy = Arc::new(Policy::new(store.clone(), false));
    let verifier = Arc::new(Verifier::new("fixture.verifier"));
    let (store, task) =
        super::tests::fixture_in_with_policy(store, false, Some(policy.descriptor())).await;
    pin_verifier(&store, &task, &verifier);

    let before = store.task(&task).unwrap();
    let report =
        preview_with_restored_policies(&store, &task, policy.clone(), verifier.clone()).unwrap();
    assert_eq!(report["escalation_policy"]["id"], "fixture.restored_deny");
    assert_eq!(report["verifier_provider"]["id"], "fixture.verifier");
    assert_eq!(report["verifier_implementation_attested"], false);
    assert_eq!(report["resume_authorized"], false);
    assert_eq!(store.task(&task).unwrap(), before);
    assert_eq!(policy.decisions.load(Ordering::Acquire), 0);
    assert_eq!(verifier.calls.load(Ordering::Acquire), 0);

    drop(verifier);
    drop(policy);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn restored_verifier_checks_the_original_candidate_without_replaying_charged_work() {
    let (store, root) = persistent();
    let verifier = Arc::new(Verifier::new("fixture.verifier"));
    let (store, task) = super::tests::fixture_in_with_policy(store, false, None).await;
    pin_verifier(&store, &task, &verifier);
    let handle = crate::verification::provider::Handle::new(verifier.clone()).unwrap();
    let checkpoint = {
        let db = store.lock().unwrap();
        super::prepare_with_policy(
            &db,
            &task,
            &crate::escalation::Handle::default(),
            Some(&handle),
        )
        .unwrap()
    };
    let original_deadline = checkpoint.state.deadline;
    let original_calls = checkpoint.state.resources.calls;
    let original_tokens = checkpoint.state.resources.reserved_output_tokens;
    let backend = Arc::new(SolverReplies(std::sync::Mutex::new(
        ["remaining advice", "[1,2,3]"].into(),
    )));
    let mut session = crate::reasoning::Session::restored(
        checkpoint.cfg,
        store.clone(),
        backend.clone(),
        task.clone(),
        &checkpoint.mode,
        checkpoint.state,
    )
    .unwrap();
    session.restore_verifier_provider(handle);
    let answer = session
        .continue_graph(
            checkpoint.subtasks,
            &checkpoint.query,
            &checkpoint.context,
            &checkpoint.mode,
            None,
        )
        .await
        .unwrap();

    assert_eq!(answer, "[1,2,3]");
    assert!(session.verified);
    assert_eq!(session.resources.calls, original_calls + 2);
    assert_eq!(
        session.resources.reserved_output_tokens,
        original_tokens + 576,
        "continuation reserves 192 tokens for the remaining subtask and 384 for the final solver call"
    );
    assert_eq!(session.resources.tool_calls, 1);
    assert_eq!(verifier.calls.load(Ordering::Acquire), 1);
    let requests = verifier.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0, "integer_sort/v1");
    assert_eq!(requests[0].1, ORIGINAL_QUERY);
    assert_eq!(
        requests[0].2,
        crate::store::sha256(ORIGINAL_QUERY.as_bytes())
    );
    assert_eq!(requests[0].3, crate::store::sha256(b"[1,2,3]"));
    assert_eq!(requests[0].4, original_deadline);
    drop(requests);
    let audit = store.task(&task).unwrap();
    assert_eq!(
        audit["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "verification_provider_result")
            .count(),
        1
    );
    assert!(backend.0.lock().unwrap().is_empty());

    drop(session);
    drop(verifier);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn missing_changed_or_unresolved_verifier_restoration_denies_before_writes() {
    for variant in 0..3 {
        let (store, root) = persistent();
        let pinned = Arc::new(Verifier::new("fixture.verifier"));
        let supplied = if variant == 1 {
            Arc::new(Verifier::new("fixture.other_verifier"))
        } else {
            pinned.clone()
        };
        let (store, task) = super::tests::fixture_in_with_policy(store, false, None).await;
        let criterion = pin_verifier(&store, &task, &pinned);
        if variant == 2 {
            append_unresolved_verifier_intent(&store, &task, &pinned, &criterion);
        }

        let before = store.task(&task).unwrap();
        let event_count: u64 = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1",
                [&task],
                |row| row.get(0),
            )
            .unwrap();
        let preview = if variant == 0 {
            preview(&store, &task)
        } else {
            preview_with_verifier_provider(&store, &task, supplied.clone())
        };
        assert!(
            preview.is_err(),
            "variant {variant} was accepted by preview"
        );
        let resume = if variant == 0 {
            resume(store.clone(), &task, true, std::future::pending()).await
        } else {
            resume_with_verifier_provider(
                store.clone(),
                &task,
                true,
                supplied.clone(),
                std::future::pending(),
            )
            .await
        };
        assert!(resume.is_err(), "variant {variant} was accepted by resume");
        assert_eq!(store.task(&task).unwrap(), before);
        let after_event_count: u64 = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1",
                [&task],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after_event_count, event_count);
        assert_eq!(pinned.calls.load(Ordering::Acquire), 0);
        assert_eq!(supplied.calls.load(Ordering::Acquire), 0);

        drop(supplied);
        drop(pinned);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
