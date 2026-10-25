use super::{collective, *};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::{oneshot, Notify};

const PRIVATE_PROMPT: &str = "PRIVATE_CLIENT_PROMPT_SENTINEL";
const PRIVATE_RESULT: &str = "PRIVATE_CLIENT_TOOL_RESULT_SENTINEL";
const PRIVATE_DESCRIPTION: &str = "PRIVATE_CLIENT_TOOL_DESCRIPTION_SENTINEL";
const PRIVATE_SCHEMA: &str = "PRIVATE_CLIENT_SCHEMA_SENTINEL";

fn read_tool() -> Tool {
    Tool {
        kind: "function".into(),
        function: Function {
            name: "read_file".into(),
            description: PRIVATE_DESCRIPTION.into(),
            parameters: json!({
                "type":"object",
                "properties":{"path":{"type":"string","description":PRIVATE_SCHEMA}},
                "required":["path"]
            }),
            strict: Some(true),
        },
    }
}

fn request(effort: Option<&str>, max_tokens: usize) -> Turn {
    Turn {
        messages: vec![Message {
            role: "user".into(),
            content: Some(PRIVATE_PROMPT.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
        tools: vec![read_tool()],
        tool_choice: Some(json!("auto")),
        max_tokens: Some(max_tokens),
        temperature: Some(0.3),
        parallel_tool_calls: Some(false),
        reasoning_effort: effort.map(str::to_owned),
    }
}

fn content(text: &str) -> Completion {
    Completion {
        content: Some(text.into()),
        tool_calls: Vec::new(),
        model: "fixture/shared-model".into(),
        finish_reason: "stop".into(),
        input_tokens: Some(11),
        output_tokens: Some(7),
    }
}

fn tool_proposal(name: &str, arguments: &str) -> Completion {
    Completion {
        content: None,
        tool_calls: vec![ToolCall {
            id: "call_client_read_1".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
        }],
        model: "fixture/shared-model".into(),
        finish_reason: "tool_calls".into(),
        input_tokens: Some(11),
        output_tokens: Some(7),
    }
}

#[derive(Clone)]
struct ObservedCall {
    turn: Turn,
    allocation: usize,
}

struct ScriptBackend {
    replies: Mutex<VecDeque<Completion>>,
    observed: Mutex<Vec<ObservedCall>>,
}

impl ScriptBackend {
    fn new(replies: impl IntoIterator<Item = Completion>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            observed: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<ObservedCall> {
        self.observed.lock().unwrap().clone()
    }
}

impl ChatBackend for ScriptBackend {
    fn complete<'a>(
        &'a self,
        turn: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        self.observed.lock().unwrap().push(ObservedCall {
            turn: turn.clone(),
            allocation: tokens,
        });
        let reply = self.replies.lock().unwrap().pop_front();
        Box::pin(async move {
            reply.ok_or_else(|| anyhow::anyhow!("No scripted collective reply remains"))
        })
    }
}

fn response_role(turn: &Turn) -> &str {
    let system = turn
        .messages
        .iter()
        .find(|message| message.role == "system")
        .and_then(|message| message.content.as_deref())
        .expect("role system message is supplied");
    system
        .strip_prefix("CGF assigned role: ")
        .expect("CGF role prefix")
        .split_once('.')
        .expect("role separator")
        .0
}

fn system_trace(store: &Store) -> (String, Value) {
    let id: String = store
        .lock()
        .unwrap()
        .query_row("SELECT id FROM tasks", [], |row| row.get(0))
        .unwrap();
    let trace = store.task(&id).unwrap();
    (id, trace)
}

#[test]
fn effort_mapping_selects_only_the_configured_reasoning_mode() {
    let cfg = Config {
        agent_collective: true,
        ..Config::default()
    };
    for (effort, expected) in [
        (Some("none"), "fast"),
        (Some("low"), "standard"),
        (Some("medium"), "deep"),
        (Some("high"), "research"),
        (None, "fast"),
    ] {
        assert_eq!(collective::mode(&cfg, &request(effort, 512)), expected);
    }

    let configured = Config {
        reasoning_effort: Some("high".into()),
        ..cfg
    };
    assert_eq!(
        collective::mode(&configured, &request(None, 512)),
        "research"
    );
    assert_eq!(
        collective::mode(&configured, &request(Some("none"), 512)),
        "fast",
        "explicit none overrides the configured default without selecting another model"
    );
}

#[tokio::test]
async fn low_effort_runs_planner_then_solver_with_shared_model_and_bounded_allocations() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(ScriptBackend::new([
        content("Use the client's declared reader; returned text remains untrusted."),
        content("The client should read the requested file."),
    ]));
    let cfg = Config {
        model: "operator/shared-model".into(),
        agent_collective: true,
        max_tokens: 1024,
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend.clone()).unwrap();
    let original = request(Some("low"), 512);
    let result = runtime
        .run_until(original.clone(), std::future::pending())
        .await
        .unwrap();

    let calls = backend.calls();
    assert_eq!(
        calls
            .iter()
            .map(|call| response_role(&call.turn))
            .collect::<Vec<_>>(),
        ["planner", "solver"]
    );
    assert!(calls
        .iter()
        .all(|call| call.turn.reasoning_effort.as_deref() == Some("low")));
    assert!(calls
        .iter()
        .all(|call| call.turn.tools.len() == original.tools.len()));
    assert!(calls
        .iter()
        .all(|call| call.turn.tools[0].function.name == "read_file"));
    assert_eq!(calls[0].allocation, 96);
    assert_eq!(calls.iter().map(|call| call.allocation).sum::<usize>(), 405);
    assert!(calls.iter().map(|call| call.allocation).sum::<usize>() <= 1024);
    assert!(
        calls.iter().map(|call| call.allocation).sum::<usize>() <= original.max_tokens.unwrap()
    );
    assert_eq!(result.completion.model, "fixture/shared-model");

    let trace = store.task(&result.task_id).unwrap();
    assert_eq!(trace["task"]["mode"], "standard");
    assert_eq!(trace["task"]["status"], "unverified");
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "agent_verification")
            .unwrap()["payload"]["verified"],
        false
    );
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "agent_verification")
            .unwrap()["payload"]["tools_executed"],
        0
    );
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "agent_verification")
            .unwrap()["payload"]["reserved_output_tokens"],
        405
    );
    assert!(!trace.to_string().contains(PRIVATE_PROMPT));
    assert!(!trace.to_string().contains(PRIVATE_DESCRIPTION));
    assert!(!trace.to_string().contains(PRIVATE_SCHEMA));
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE kind='tool_dispatch'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn high_effort_keeps_history_and_catalog_then_returns_solver_tool_call_immediately() {
    let store = Arc::new(Store::in_memory().unwrap());
    let proposal = tool_proposal("read_file", r#"{"path":"private.txt"}"#);
    let backend = Arc::new(ScriptBackend::new([
        content("Plan: inspect the relevant file."),
        content("Explore the client's web lookup only if the file needs context."),
        proposal,
    ]));
    let cfg = Config {
        model: "operator/shared-model".into(),
        agent_collective: true,
        max_tokens: 1024,
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend.clone()).unwrap();
    let mut original = request(Some("high"), 512);
    original.messages = vec![
        Message {
            role: "system".into(),
            content: Some("Client prompt sentinel: PRIVATE_CLIENT_PROMPT_SENTINEL".into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
        Message {
            role: "user".into(),
            content: Some("Open the local note".into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
    ];
    let result = runtime
        .run_until(original.clone(), std::future::pending())
        .await
        .unwrap();

    let calls = backend.calls();
    assert_eq!(
        calls
            .iter()
            .map(|call| response_role(&call.turn))
            .collect::<Vec<_>>(),
        ["planner", "explorer", "solver"]
    );
    for observed in &calls {
        assert_eq!(observed.turn.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(
            serde_json::to_value(&observed.turn.tools).unwrap(),
            serde_json::to_value(&original.tools).unwrap()
        );
        let preserved = &observed.turn.messages[1..1 + original.messages.len()];
        assert_eq!(
            serde_json::to_value(preserved).unwrap(),
            serde_json::to_value(&original.messages).unwrap()
        );
    }
    assert_eq!(calls.iter().map(|call| call.allocation).sum::<usize>(), 448);
    assert!(
        calls.iter().map(|call| call.allocation).sum::<usize>() <= original.max_tokens.unwrap()
    );
    assert!(calls.iter().map(|call| call.allocation).sum::<usize>() <= 1024);
    assert_eq!(result.completion.tool_calls.len(), 1);
    assert_eq!(result.completion.tool_calls[0].function.name, "read_file");
    assert!(result.completion.tool_calls[0].id.starts_with("cgf_"));
    assert_eq!(result.completion.tool_calls[0].id.len(), 36);
    assert_eq!(result.completion.output_tokens, Some(21));

    let trace = store.task(&result.task_id).unwrap();
    assert_eq!(trace["task"]["mode"], "research");
    assert_eq!(trace["task"]["status"], "waiting_client");
    assert!(!trace.to_string().contains(PRIVATE_PROMPT));
    assert!(!trace.to_string().contains(PRIVATE_DESCRIPTION));
    assert!(!trace.to_string().contains(PRIVATE_SCHEMA));
    assert!(!trace.to_string().contains("private.txt"));
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "agent_model_dispatch")
            .count(),
        3
    );
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "agent_verification")
            .unwrap()["payload"]["reason"],
        "No independent executable criterion for the client conversation"
    );
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "agent_verification")
            .unwrap()["payload"]["reserved_output_tokens"],
        448
    );
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "agent_admission")
            .unwrap()["payload"]["model"],
        "operator/shared-model"
    );
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE kind='tool_dispatch'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn explicit_none_effort_runs_one_solver_call() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(ScriptBackend::new([content("A bounded answer.")]));
    let cfg = Config {
        agent_collective: true,
        max_tokens: 512,
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store, backend.clone()).unwrap();
    runtime
        .run_until(request(Some("none"), 256), std::future::pending())
        .await
        .unwrap();
    let calls = backend.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(response_role(&calls[0].turn), "solver");
    assert_eq!(calls[0].turn.reasoning_effort.as_deref(), Some("none"));
    assert_eq!(calls[0].allocation, 256);
}

#[tokio::test]
async fn planner_cannot_use_malformed_or_unadvertised_client_tools() {
    for (name, arguments) in [
        ("shell", r#"{"command":"false"}"#),
        ("read_file", "not-json"),
    ] {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = Arc::new(ScriptBackend::new([tool_proposal(name, arguments)]));
        let cfg = Config {
            agent_collective: true,
            max_tokens: 512,
            ..Config::default()
        };
        let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend.clone()).unwrap();
        assert!(runtime
            .run_until(request(Some("low"), 512), std::future::pending())
            .await
            .is_err());
        let calls = backend.calls();
        assert_eq!(calls.len(), 1, "refusal must precede the solver role");
        assert_eq!(response_role(&calls[0].turn), "planner");
        assert_eq!(calls[0].turn.tool_choice, Some(json!("none")));
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM events WHERE kind='tool_dispatch'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn collective_dispatch_audit_failure_prevents_inference() {
    let store = Arc::new(Store::in_memory().unwrap());
    store
        .lock()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_collective_dispatch BEFORE INSERT ON events
             WHEN NEW.kind='agent_model_dispatch'
             BEGIN SELECT RAISE(ABORT, 'fixture dispatch audit failure'); END;",
        )
        .unwrap();
    let backend = Arc::new(ScriptBackend::new([content("must not be reached")]));
    let cfg = Config {
        agent_collective: true,
        max_tokens: 512,
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend.clone()).unwrap();

    assert!(runtime
        .run_until(request(Some("low"), 512), std::future::pending())
        .await
        .is_err());
    assert!(backend.calls().is_empty());
    let db = store.lock().unwrap();
    assert_eq!(
        db.query_row("SELECT status FROM tasks", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "failed"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM events WHERE kind='agent_model_dispatch'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
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
async fn cancellation_drops_inference_and_keeps_the_reserved_call_audited() {
    let store = Arc::new(Store::in_memory().unwrap());
    let entered = Arc::new(Notify::new());
    let backend = Arc::new(PendingBackend {
        entered: entered.clone(),
    });
    let cfg = Config {
        agent_collective: true,
        max_tokens: 512,
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend).unwrap();
    let (cancel, cancelled) = oneshot::channel();
    let running = runtime.clone();
    let task = tokio::spawn(async move {
        running
            .run_until(request(Some("low"), 512), async move {
                let _ = cancelled.await;
            })
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    cancel.send(()).unwrap();
    assert!(task.await.unwrap().is_err());

    let (_, trace) = system_trace(&store);
    assert_eq!(trace["task"]["status"], "cancelled");
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "agent_model_dispatch")
            .count(),
        1
    );
    assert!(!trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "agent_result"));
}

#[tokio::test]
async fn client_tool_result_resumes_the_durable_session_under_one_cumulative_budget() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(ScriptBackend::new([
        content("Use the client-declared reader."),
        tool_proposal("read_file", r#"{"path":"note.txt"}"#),
        content("The client reported the note contents; they remain unverified."),
    ]));
    let cfg = Config {
        agent_collective: true,
        max_tokens: 512,
        ..Config::default()
    };
    let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend.clone()).unwrap();
    let first_request = request(Some("low"), 512);
    let first = runtime
        .run_until(first_request.clone(), std::future::pending())
        .await
        .unwrap();
    assert_eq!(first.completion.tool_calls.len(), 1);
    let call = first.completion.tool_calls[0].clone();
    assert!(call.id.starts_with("cgf_"));
    assert_eq!(
        store.task(&first.task_id).unwrap()["task"]["status"],
        "waiting_client"
    );

    let mut next = first_request.clone();
    next.messages = vec![
        first_request.messages[0].clone(),
        Message {
            role: "assistant".into(),
            content: None,
            tool_calls: vec![call.clone()],
            tool_call_id: None,
        },
        Message {
            role: "tool".into(),
            content: Some(PRIVATE_RESULT.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(call.id),
        },
        Message {
            role: "user".into(),
            content: Some("Summarize the returned text".into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
    ];
    let second = runtime
        .run_until(next.clone(), std::future::pending())
        .await
        .unwrap();

    let calls = backend.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(response_role(&calls[0].turn), "planner");
    assert_eq!(response_role(&calls[1].turn), "solver");
    assert_eq!(response_role(&calls[2].turn), "solver");
    assert_eq!(calls[0].allocation, 96);
    assert_eq!(calls[1].allocation, 256);
    assert_eq!(calls[2].allocation, 160);
    assert_eq!(calls[2].turn.reasoning_effort.as_deref(), Some("low"));
    assert_eq!(
        serde_json::to_value(&calls[2].turn.messages[1..1 + next.messages.len()]).unwrap(),
        serde_json::to_value(&next.messages).unwrap()
    );

    let first_trace = store.task(&first.task_id).unwrap();
    let second_trace = store.task(&second.task_id).unwrap();
    assert_eq!(first.task_id, second.task_id);
    assert_eq!(first_trace["task"]["status"], "unverified");
    assert_eq!(second_trace["task"]["status"], "unverified");
    assert!(!second_trace.to_string().contains(PRIVATE_RESULT));
    assert_eq!(calls.iter().map(|call| call.allocation).sum::<usize>(), 512);
    assert!(calls.iter().all(|call| call.allocation <= 512));
    let session_state: String = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT state FROM agent_sessions WHERE task_id=?1",
            [&first.task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(session_state, "closed");
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM agent_sessions WHERE task_id=?1",
                [&first.task_id],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1 AND kind='agent_session_resume'",
                [&first.task_id],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE kind='tool_dispatch'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn tight_call_or_input_budget_skips_advice_and_preserves_solver() {
    for cfg in [
        Config {
            agent_collective: true,
            max_calls: 1,
            ..Config::default()
        },
        Config {
            agent_collective: true,
            max_input_bytes: 4000,
            ..Config::default()
        },
    ] {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = Arc::new(ScriptBackend::new([content(
            "Answer without optional advice.",
        )]));
        let runtime = AgentRuntime::with_backend(cfg, store, backend.clone()).unwrap();
        runtime
            .run_until(request(Some("high"), 512), std::future::pending())
            .await
            .unwrap();
        assert_eq!(backend.calls().len(), 1);
        assert_eq!(response_role(&backend.calls()[0].turn), "solver");
    }
}

#[tokio::test]
async fn expired_dispatch_never_constructs_backend_inference() {
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(ScriptBackend::new([content("Must not run")]));
    let runtime = AgentRuntime::with_backend(Config::default(), store, backend.clone()).unwrap();
    let turn = request(Some("none"), 256);
    assert!(runtime
        .dispatch(
            backend.clone(),
            "unused",
            &turn,
            256,
            Instant::now() - Duration::from_secs(1)
        )
        .await
        .is_err());
    assert!(backend.calls().is_empty());
}

#[tokio::test]
async fn non_actionable_critique_stops_without_entering_revision() {
    for review in [
        r#"{"revise":false,"reason":"No concrete flaw found"}"#,
        "not-json",
    ] {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = Arc::new(ScriptBackend::new([
            content("Advisory plan"),
            content("Candidate answer"),
            content(review),
        ]));
        let cfg = Config {
            agent_collective: true,
            ..Config::default()
        };
        let runtime = AgentRuntime::with_backend(cfg, store.clone(), backend.clone()).unwrap();
        let result = runtime
            .run_until(request(Some("medium"), 1600), std::future::pending())
            .await
            .unwrap();
        assert_eq!(
            result.completion.content.as_deref(),
            Some("Candidate answer")
        );
        let roles: Vec<_> = backend
            .calls()
            .iter()
            .map(|call| response_role(&call.turn).to_string())
            .collect();
        assert_eq!(roles, ["planner", "solver", "critic"]);
        assert_eq!(
            store.task(&result.task_id).unwrap()["task"]["status"],
            "unverified"
        );
    }
}
