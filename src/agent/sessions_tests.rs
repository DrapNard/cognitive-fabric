//! Durable client collective session regressions. These fixtures exercise only
//! the local state machine and never contact a live inference provider.
use super::*;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{oneshot, Notify};

fn request() -> Turn {
    Turn {
        messages: vec![Message {
            role: "user".into(),
            content: Some("Inspect the requested local note".into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
        tools: vec![Tool {
            kind: "function".into(),
            function: Function {
                name: "read_note".into(),
                description: "Read one client-selected note".into(),
                parameters: json!({
                    "type":"object",
                    "properties":{"path":{"type":"string"}},
                    "required":["path"],
                    "additionalProperties":false
                }),
                strict: Some(true),
            },
        }],
        tool_choice: Some(json!("auto")),
        max_tokens: Some(4096),
        temperature: Some(0.2),
        parallel_tool_calls: Some(false),
        reasoning_effort: Some("high".into()),
    }
}

fn config() -> Config {
    Config {
        model: "fixture/shared-model".into(),
        agent_collective: true,
        reasoning_effort: Some("high".into()),
        max_calls: 8,
        max_cycles: 4,
        max_tokens: 4096,
        max_input_bytes: 512_000,
        max_tool_calls: 4,
        max_wall_seconds: 120,
        ..Config::default()
    }
}

fn answer(content: &str) -> Completion {
    Completion {
        content: Some(content.into()),
        tool_calls: Vec::new(),
        model: "fixture/shared-model".into(),
        finish_reason: "stop".into(),
        input_tokens: Some(13),
        output_tokens: Some(9),
    }
}

fn proposal() -> Completion {
    Completion {
        content: None,
        tool_calls: vec![ToolCall {
            // The runtime must replace this provider-local ID with its opaque
            // durable handle before it returns the completion to the client.
            id: "provider-call-1".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: "read_note".into(),
                arguments: r#"{"path":"notes/today.txt"}"#.into(),
            },
        }],
        model: "fixture/shared-model".into(),
        finish_reason: "tool_calls".into(),
        input_tokens: Some(13),
        output_tokens: Some(9),
    }
}

#[derive(Clone)]
struct Observed {
    task: String,
    turn: Turn,
    allocation: usize,
}

struct ScriptBackend {
    replies: Mutex<VecDeque<Completion>>,
    observed: Mutex<Vec<Observed>>,
}

impl ScriptBackend {
    fn new(replies: impl IntoIterator<Item = Completion>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            observed: Mutex::new(Vec::new()),
        }
    }

    fn observed(&self) -> Vec<Observed> {
        self.observed.lock().unwrap().clone()
    }
}

impl ChatBackend for ScriptBackend {
    fn complete_for<'a>(
        &'a self,
        task: &'a str,
        turn: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        self.observed.lock().unwrap().push(Observed {
            task: task.to_owned(),
            turn: turn.clone(),
            allocation: tokens,
        });
        let reply = self.replies.lock().unwrap().pop_front();
        Box::pin(async move {
            reply.ok_or_else(|| anyhow::anyhow!("No scripted session reply remains"))
        })
    }

    fn complete<'a>(
        &'a self,
        turn: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        self.complete_for("unbound-fixture-task", turn, tokens)
    }
}

fn role(turn: &Turn) -> &str {
    turn.messages
        .iter()
        .find(|message| message.role == "system")
        .and_then(|message| message.content.as_deref())
        .and_then(|s| s.strip_prefix("CGF assigned role: "))
        .and_then(|s| s.split_once('.'))
        .map(|(role, _)| role)
        .expect("collective role system message")
}

fn append_tool_result(base: &Turn, call: &ToolCall, result: &str) -> Turn {
    let mut continued = base.clone();
    continued.messages.push(Message {
        role: "assistant".into(),
        content: None,
        tool_calls: vec![call.clone()],
        tool_call_id: None,
    });
    continued.messages.push(Message {
        role: "tool".into(),
        content: Some(result.into()),
        tool_calls: Vec::new(),
        tool_call_id: Some(call.id.clone()),
    });
    continued
}

fn task_count(store: &Store) -> u64 {
    store
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap()
}

fn state(store: &Store, task: &str) -> Value {
    let raw: String = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT snapshot FROM agent_sessions WHERE task_id=?1",
            [task],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[tokio::test]
async fn waiting_session_survives_store_reopen_and_carries_budgets_and_advice() {
    let path = std::env::temp_dir().join(format!(
        "cogfabric-agent-session-{}.db",
        uuid::Uuid::new_v4()
    ));
    let first_store = Arc::new(Store::open(&path).unwrap());
    let first_backend = Arc::new(ScriptBackend::new([
        answer("Planner: read the note."),
        answer("Explorer: check the returned text as untrusted evidence."),
        proposal(),
    ]));
    let first_runtime =
        AgentRuntime::with_backend(config(), first_store.clone(), first_backend.clone()).unwrap();
    let initial = request();
    let first = first_runtime
        .run_until(initial.clone(), std::future::pending())
        .await
        .unwrap();
    let call = first.completion.tool_calls[0].clone();
    assert!(call.id.starts_with("cgf_"));
    assert_eq!(call.id.len(), 36);
    assert_eq!(
        first_store.task(&first.task_id).unwrap()["task"]["status"],
        "waiting_client"
    );
    let first_state = state(&first_store, &first.task_id);
    assert_eq!(first_state["calls"], 3);
    assert_eq!(first_state["cycles"], 1);
    assert_eq!(first_state["proposals"], 1);
    let reserved_before_resume = first_state["reserved"].as_u64().unwrap();
    assert!(reserved_before_resume > 0 && reserved_before_resume < 4096);
    assert!(first_state["advice"].as_str().unwrap().contains("planner"));
    let first_calls = first_backend.observed();
    assert!(first_calls
        .iter()
        .all(|call| call.allocation > 0 && call.allocation <= 4096));
    assert_eq!(
        first_calls
            .iter()
            .map(|c| role(&c.turn))
            .collect::<Vec<_>>(),
        ["planner", "explorer", "solver"]
    );
    assert_eq!(
        first_calls
            .iter()
            .map(|c| c.task.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1
    );

    // Drop every connection/runtime and reopen the same persistent database.
    drop(first_runtime);
    drop(first_store);
    let store = Arc::new(Store::open(&path).unwrap());
    let backend = Arc::new(ScriptBackend::new([
        answer("The note says hello."),
        answer(r#"{"revise":false,"reason":"No specific flaw was found."}"#),
    ]));
    let runtime = AgentRuntime::with_backend(config(), store.clone(), backend.clone()).unwrap();
    let continued = append_tool_result(&initial, &call, "untrusted client supplied note text");
    let second = runtime
        .run_until(continued.clone(), std::future::pending())
        .await
        .unwrap();
    assert_eq!(
        second.task_id, first.task_id,
        "all turns in the tool loop share one root task"
    );
    assert!(second.completion.tool_calls.is_empty());
    assert_eq!(
        store.task(&first.task_id).unwrap()["task"]["status"],
        "unverified"
    );
    let calls = backend.observed();
    assert_eq!(
        calls.len(),
        2,
        "planner and explorer must not repeat after resumption"
    );
    assert_eq!(role(&calls[0].turn), "solver");
    assert_eq!(role(&calls[1].turn), "critic");
    assert!(calls[0]
        .turn
        .messages
        .iter()
        .any(|m| m.content.as_deref() == Some("untrusted client supplied note text")));
    assert!(calls[0].turn.messages.iter().any(|m| m
        .content
        .as_deref()
        .is_some_and(|text| text.contains("planner"))));
    let after = state(&store, &first.task_id);
    assert_eq!(after["calls"], 5);
    assert_eq!(after["cycles"], 1);
    assert!(after["reserved"].as_u64().unwrap() > reserved_before_resume);
    assert!(after["reserved"].as_u64().unwrap() <= 4096);
    assert_eq!(after["proposals"], 1);
    let trace = store.task(&first.task_id).unwrap();
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "agent_model_dispatch")
            .count(),
        5
    );
    assert!(!trace
        .to_string()
        .contains("untrusted client supplied note text"));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[tokio::test]
async fn changed_history_pending_call_arguments_and_missing_results_fail_before_inference() {
    for mutation in ["history", "arguments", "missing_result", "call_id"] {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = Arc::new(ScriptBackend::new([
            answer("Plan"),
            answer("Explore"),
            proposal(),
        ]));
        let runtime = AgentRuntime::with_backend(config(), store.clone(), backend.clone()).unwrap();
        let initial = request();
        let first = runtime
            .run_until(initial.clone(), std::future::pending())
            .await
            .unwrap();
        let call = first.completion.tool_calls[0].clone();
        let mut continued = append_tool_result(&initial, &call, "client result");
        match mutation {
            "history" => continued.messages[0].content = Some("a different objective".into()),
            "arguments" => {
                continued.messages[1].tool_calls[0].function.arguments =
                    r#"{"path":"other.txt"}"#.into()
            }
            "missing_result" => {
                continued.messages.pop();
            }
            "call_id" => {
                continued.messages[1].tool_calls[0].id =
                    "cgf_00000000000040008000000000000000".into()
            }
            _ => unreachable!(),
        }
        let observer = Arc::new(ScriptBackend::new([answer("must not run")]));
        let runtime =
            AgentRuntime::with_backend(config(), store.clone(), observer.clone()).unwrap();
        assert!(
            runtime
                .run_until(continued, std::future::pending())
                .await
                .is_err(),
            "mutation {mutation}"
        );
        assert!(
            observer.observed().is_empty(),
            "mutation {mutation} must fail before model dispatch"
        );
        assert_eq!(
            task_count(&store),
            1,
            "invalid continuation must not create a fresh task"
        );
    }
}

#[tokio::test]
async fn unknown_cgf_handle_does_not_fall_back_to_a_fresh_budget() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(ScriptBackend::new([answer("must not run")]));
    let runtime = AgentRuntime::with_backend(config(), store.clone(), backend.clone()).unwrap();
    let mut unknown = request();
    unknown.messages.push(Message {
        role: "assistant".into(),
        content: None,
        tool_calls: vec![ToolCall {
            id: "cgf_00000000000040008000000000000000".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: "read_note".into(),
                arguments: r#"{"path":"notes/today.txt"}"#.into(),
            },
        }],
        tool_call_id: None,
    });
    unknown.messages.push(Message {
        role: "tool".into(),
        content: Some("fake result".into()),
        tool_calls: Vec::new(),
        tool_call_id: Some("cgf_00000000000040008000000000000000".into()),
    });
    assert!(runtime
        .run_until(unknown, std::future::pending())
        .await
        .is_err());
    assert!(backend.observed().is_empty());
    assert_eq!(task_count(&store), 0);
}

struct PendingBackend {
    entered: Arc<Notify>,
}

impl ChatBackend for PendingBackend {
    fn complete<'a>(
        &'a self,
        _: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        let entered = self.entered.clone();
        Box::pin(async move {
            entered.notify_one();
            std::future::pending().await
        })
    }
}

#[tokio::test]
async fn cancellation_during_resume_keeps_charges_and_prevents_retry() {
    let store = Arc::new(Store::in_memory().unwrap());
    let initial_backend = Arc::new(ScriptBackend::new([
        answer("Plan"),
        answer("Explore"),
        proposal(),
    ]));
    let initial_runtime =
        AgentRuntime::with_backend(config(), store.clone(), initial_backend).unwrap();
    let initial = request();
    let first = initial_runtime
        .run_until(initial.clone(), std::future::pending())
        .await
        .unwrap();
    let call = first.completion.tool_calls[0].clone();
    let before = state(&store, &first.task_id);
    let resumed = append_tool_result(&initial, &call, "untrusted result");
    let entered = Arc::new(Notify::new());
    let pending_backend = Arc::new(PendingBackend {
        entered: entered.clone(),
    });
    let runtime = AgentRuntime::with_backend(config(), store.clone(), pending_backend).unwrap();
    let (cancel, cancelled) = oneshot::channel();
    let running = runtime.clone();
    let resumed_for_task = resumed.clone();
    let task = tokio::spawn(async move {
        running
            .run_until(resumed_for_task, async move {
                let _ = cancelled.await;
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    cancel.send(()).unwrap();
    assert!(task.await.unwrap().is_err());
    let after = state(&store, &first.task_id);
    assert_eq!(after["calls"], before["calls"].as_u64().unwrap() + 1);
    assert!(after["reserved"].as_u64().unwrap() > before["reserved"].as_u64().unwrap());
    assert_eq!(
        store.task(&first.task_id).unwrap()["task"]["status"],
        "cancelled"
    );

    let retry_backend = Arc::new(ScriptBackend::new([answer(
        "must not retry uncertain generation",
    )]));
    let retry_runtime =
        AgentRuntime::with_backend(config(), store.clone(), retry_backend.clone()).unwrap();
    assert!(retry_runtime
        .run_until(resumed, std::future::pending())
        .await
        .is_err());
    assert!(retry_backend.observed().is_empty());
    assert_eq!(state(&store, &first.task_id)["calls"], after["calls"]);
}

#[tokio::test]
async fn changed_model_configuration_denies_resume_before_backend_dispatch() {
    let store = Arc::new(Store::in_memory().unwrap());
    let first_backend = Arc::new(ScriptBackend::new([
        answer("Plan"),
        answer("Explore"),
        proposal(),
    ]));
    let first_runtime = AgentRuntime::with_backend(config(), store.clone(), first_backend).unwrap();
    let initial = request();
    let first = first_runtime
        .run_until(initial.clone(), std::future::pending())
        .await
        .unwrap();
    let continued = append_tool_result(&initial, &first.completion.tool_calls[0], "client result");
    let altered = Config {
        model: "fixture/other-model".into(),
        ..config()
    };
    let backend = Arc::new(ScriptBackend::new([answer("must not run")]));
    let runtime = AgentRuntime::with_backend(altered, store.clone(), backend.clone()).unwrap();
    assert!(runtime
        .run_until(continued, std::future::pending())
        .await
        .is_err());
    assert!(backend.observed().is_empty());
    assert_eq!(task_count(&store), 1);
}

#[tokio::test]
async fn corrupted_checkpoint_is_rejected_before_backend_dispatch() {
    let store = Arc::new(Store::in_memory().unwrap());
    let first_backend = Arc::new(ScriptBackend::new([
        answer("Plan"),
        answer("Explore"),
        proposal(),
    ]));
    let first_runtime = AgentRuntime::with_backend(config(), store.clone(), first_backend).unwrap();
    let initial = request();
    let first = first_runtime
        .run_until(initial.clone(), std::future::pending())
        .await
        .unwrap();
    let continued = append_tool_result(&initial, &first.completion.tool_calls[0], "client result");
    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE agent_sessions SET snapshot_sha256=?2 WHERE task_id=?1",
            rusqlite::params![first.task_id, "0".repeat(64)],
        )
        .unwrap();
    let backend = Arc::new(ScriptBackend::new([answer("must not run")]));
    let runtime = AgentRuntime::with_backend(config(), store.clone(), backend.clone()).unwrap();
    assert!(runtime
        .run_until(continued, std::future::pending())
        .await
        .is_err());
    assert!(backend.observed().is_empty());
    assert_eq!(task_count(&store), 1);
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn expired_same_boot_deadline_is_rejected_without_refunding_or_replaying() {
    let store = Arc::new(Store::in_memory().unwrap());
    let first_backend = Arc::new(ScriptBackend::new([
        answer("Plan"),
        answer("Explore"),
        proposal(),
    ]));
    let first_runtime = AgentRuntime::with_backend(config(), store.clone(), first_backend).unwrap();
    let initial = request();
    let first = first_runtime
        .run_until(initial.clone(), std::future::pending())
        .await
        .unwrap();
    let continued = append_tool_result(&initial, &first.completion.tool_calls[0], "client result");
    let (raw,): (String,) = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT snapshot FROM agent_sessions WHERE task_id=?1",
            [&first.task_id],
            |row| Ok((row.get(0)?,)),
        )
        .unwrap();
    let mut checkpoint: Value = serde_json::from_str(&raw).unwrap();
    // Keep the configured 120-second interval structurally valid while placing
    // it at boot time zero, which is expired on the current Linux boot clock.
    checkpoint["clock"]["started_ns"] = json!(0);
    checkpoint["clock"]["deadline_ns"] = json!(120_000_000_000u64);
    let expired = serde_json::to_string(&checkpoint).unwrap();
    let digest = crate::store::sha256(expired.as_bytes());
    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE agent_sessions SET snapshot=?2,snapshot_sha256=?3 WHERE task_id=?1",
            rusqlite::params![first.task_id, expired, digest],
        )
        .unwrap();
    let backend = Arc::new(ScriptBackend::new([answer("must not run after expiry")]));
    let runtime = AgentRuntime::with_backend(config(), store.clone(), backend.clone()).unwrap();
    assert!(runtime
        .run_until(continued, std::future::pending())
        .await
        .is_err());
    assert!(backend.observed().is_empty());
    assert_eq!(task_count(&store), 1);
}

#[tokio::test]
async fn parallel_resume_requests_cannot_both_claim_one_checkpoint() {
    let store = Arc::new(Store::in_memory().unwrap());
    let first_backend = Arc::new(ScriptBackend::new([
        answer("Plan"),
        answer("Explore"),
        proposal(),
    ]));
    let first_runtime = AgentRuntime::with_backend(config(), store.clone(), first_backend).unwrap();
    let initial = request();
    let first = first_runtime
        .run_until(initial.clone(), std::future::pending())
        .await
        .unwrap();
    let continued = append_tool_result(&initial, &first.completion.tool_calls[0], "client result");
    let entered = Arc::new(Notify::new());
    let pending_backend = Arc::new(PendingBackend {
        entered: entered.clone(),
    });
    let runtime = AgentRuntime::with_backend(config(), store.clone(), pending_backend).unwrap();
    let (cancel, cancelled) = oneshot::channel();
    let running = runtime.clone();
    let first_resume = continued.clone();
    let task = tokio::spawn(async move {
        running
            .run_until(first_resume, async move {
                let _ = cancelled.await;
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    let second_backend = Arc::new(ScriptBackend::new([answer("must not run twice")]));
    let second_runtime =
        AgentRuntime::with_backend(config(), store.clone(), second_backend.clone()).unwrap();
    assert!(second_runtime
        .run_until(continued, std::future::pending())
        .await
        .is_err());
    assert!(second_backend.observed().is_empty());
    cancel.send(()).unwrap();
    assert!(task.await.unwrap().is_err());
    assert_eq!(task_count(&store), 1);
}

#[tokio::test]
async fn final_answer_closes_session_and_a_followup_user_turn_starts_a_new_root_task() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(ScriptBackend::new([
        answer("Plan"),
        answer("Explore"),
        proposal(),
        answer("The note says hello."),
        answer(r#"{"revise":false,"reason":"No specific flaw was found."}"#),
        answer("Plan the follow-up."),
        answer("Explore the follow-up."),
        answer("The follow-up answer."),
        answer(r#"{"revise":false,"reason":"No specific flaw was found."}"#),
    ]));
    let runtime = AgentRuntime::with_backend(config(), store.clone(), backend.clone()).unwrap();
    let initial = request();
    let first = runtime
        .run_until(initial.clone(), std::future::pending())
        .await
        .unwrap();
    let call = first.completion.tool_calls[0].clone();
    let resumed = append_tool_result(&initial, &call, "untrusted client result");
    let final_result = runtime
        .run_until(resumed.clone(), std::future::pending())
        .await
        .unwrap();
    assert_eq!(final_result.task_id, first.task_id);
    assert_eq!(
        store.task(&first.task_id).unwrap()["task"]["status"],
        "unverified"
    );
    let mut next_user_turn = resumed;
    next_user_turn.messages.push(Message {
        role: "assistant".into(),
        content: final_result.completion.content.clone(),
        tool_calls: Vec::new(),
        tool_call_id: None,
    });
    next_user_turn.messages.push(Message {
        role: "user".into(),
        content: Some("Now inspect the other note".into()),
        tool_calls: Vec::new(),
        tool_call_id: None,
    });
    let next = runtime
        .run_until(next_user_turn, std::future::pending())
        .await
        .unwrap();
    assert_ne!(next.task_id, first.task_id);
    assert_eq!(task_count(&store), 2);
    assert_eq!(backend.observed().len(), 9);
}

#[tokio::test]
async fn only_original_compiled_constraints_can_establish_verified_completion() {
    for (candidate, expected) in [("[1,2,3]", true), ("[3,2,1]", false)] {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = Arc::new(ScriptBackend::new([
            answer("Sort the original multiset"),
            answer(candidate),
        ]));
        let runtime = AgentRuntime::with_backend(config(), store.clone(), backend).unwrap();
        let mut turn = request();
        turn.messages[0].content = Some("sort:{\"values\":[3,1,2]}".into());
        turn.tools.clear();
        turn.tool_choice = None;
        turn.reasoning_effort = Some("low".into());
        let result = runtime
            .run_until(turn, std::future::pending())
            .await
            .unwrap();
        let trace = store.task(&result.task_id).unwrap();
        assert_eq!(
            trace["task"]["status"],
            if expected { "completed" } else { "unverified" }
        );
        assert!(trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "agent_checker_result"
                && event["payload"]["verified"] == expected));
        assert_eq!(state(&store, &result.task_id)["internal_tools"], 2);
    }
}

#[tokio::test]
async fn expired_waiting_sessions_release_capacity_without_refunding_reservations() {
    let store = Arc::new(Store::in_memory().unwrap());
    let runtime = AgentRuntime::with_backend(
        config(),
        store.clone(),
        Arc::new(ScriptBackend::new([
            answer("Plan"),
            answer("Explore"),
            proposal(),
        ])),
    )
    .unwrap();
    let initial = request();
    let first = runtime
        .run_until(initial.clone(), std::future::pending())
        .await
        .unwrap();
    let before = state(&store, &first.task_id);
    let mut expired = before.clone();
    expired["clock"]["started_ns"] = json!(0);
    expired["clock"]["deadline_ns"] = json!(120_000_000_000u64);
    let raw = serde_json::to_string(&expired).unwrap();
    store
        .lock()
        .unwrap()
        .execute(
            "UPDATE agent_sessions SET snapshot=?2,snapshot_sha256=?3 WHERE task_id=?1",
            rusqlite::params![first.task_id, raw, crate::store::sha256(raw.as_bytes())],
        )
        .unwrap();
    let mut next = request();
    next.messages[0].content = Some("A separate new goal".into());
    let runtime = AgentRuntime::with_backend(
        config(),
        store.clone(),
        Arc::new(ScriptBackend::new([
            answer("Plan"),
            answer("Explore"),
            answer("New answer"),
            answer(r#"{"revise":false,"reason":"No specific flaw"}"#),
        ])),
    )
    .unwrap();
    runtime
        .run_until(next, std::future::pending())
        .await
        .unwrap();
    assert_eq!(
        store.task(&first.task_id).unwrap()["task"]["status"],
        "exhausted"
    );
    let after = state(&store, &first.task_id);
    assert_eq!(after["reserved"], before["reserved"]);
    assert_eq!(after["calls"], before["calls"]);
    assert_eq!(after["advice"], "");
}
