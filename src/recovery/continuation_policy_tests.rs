use super::*;
use crate::escalation::{Decision, Descriptor, EscalationPolicy, Request};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
            if allow { "completed" } else { "unverified" }
        );
        assert_eq!(report["verified"], allow);
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
