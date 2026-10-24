//! Opt-in end-to-end coverage against the installed OpenCode CLI.
//!
//! Run with `cargo test -p cogfabric-server --test opencode_client -- --ignored --nocapture`.
//! The fake backend deterministically proposes read → edit → read; OpenCode itself
//! executes those proposals inside a disposable workspace with all other actions denied.

use cogfabric::{
    agent::{AgentRuntime, ChatBackend, Completion, FunctionCall, ToolCall, Turn},
    config::Config,
    engine::Engine,
    store::Store,
};
use cogfabric_server::{serve_until, Server};
use serde_json::{json, Value};
use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const KEY: &str = "fixture-opencode-key-012345678901234567890123";

struct OpenCodeFixture {
    requests: Mutex<Vec<Turn>>,
}

impl ChatBackend for OpenCodeFixture {
    fn complete<'a>(
        &'a self,
        request: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Completion>> + Send + 'a>> {
        let (call_index, has_tooling) = {
            let mut requests = self.requests.lock().unwrap();
            requests.push(request.clone());
            let has_tooling = request
                .tools
                .iter()
                .any(|tool| matches!(tool.function.name.as_str(), "read" | "edit"));
            let call_index = requests
                .iter()
                .filter(|request| {
                    request
                        .tools
                        .iter()
                        .any(|tool| matches!(tool.function.name.as_str(), "read" | "edit"))
                })
                .count();
            (call_index, has_tooling)
        };
        let request = request.clone();
        Box::pin(async move {
            if !has_tooling {
                return Ok(Completion {
                    content: Some("OpenCode fixture session".into()),
                    tool_calls: vec![],
                    model: "fixture/opencode-v1".into(),
                    finish_reason: "stop".into(),
                    input_tokens: Some(8),
                    output_tokens: Some(4),
                });
            }
            let (tool_name, arguments) = match call_index {
                1 | 3 => ("read", read_arguments(&request)?),
                2 => ("edit", edit_arguments(&request)?),
                _ => {
                    return Ok(Completion {
                        content: Some("I read the file after the edit. Status: after.".into()),
                        tool_calls: vec![],
                        model: "fixture/opencode-v1".into(),
                        finish_reason: "stop".into(),
                        input_tokens: Some(20),
                        output_tokens: Some(10),
                    })
                }
            };
            Ok(Completion {
                content: None,
                tool_calls: vec![ToolCall {
                    id: format!("fixture_call_{call_index}"),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: tool_name.into(),
                        arguments: arguments.to_string(),
                    },
                }],
                model: "fixture/opencode-v1".into(),
                finish_reason: "tool_calls".into(),
                input_tokens: Some(20),
                output_tokens: Some(8),
            })
        })
    }
}

fn tool_schema<'a>(request: &'a Turn, name: &str) -> anyhow::Result<&'a Value> {
    request
        .tools
        .iter()
        .find(|tool| tool.function.name == name)
        .map(|tool| &tool.function.parameters)
        .ok_or_else(|| anyhow::anyhow!("OpenCode did not send the expected {name} tool"))
}

fn property_name(schema: &Value, choices: &[&str]) -> anyhow::Result<String> {
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("Tool schema has no properties: {schema}"))?;
    choices
        .iter()
        .find(|name| properties.contains_key(**name))
        .map(|name| (*name).to_string())
        .ok_or_else(|| anyhow::anyhow!("None of {choices:?} found in tool schema: {schema}"))
}

fn read_arguments(request: &Turn) -> anyhow::Result<Value> {
    let schema = tool_schema(request, "read")?;
    let key = property_name(schema, &["filePath", "path", "file_path"])?;
    Ok(json!({key: "fixture.txt"}))
}

fn edit_arguments(request: &Turn) -> anyhow::Result<Value> {
    let schema = tool_schema(request, "edit")?;
    let file = property_name(schema, &["filePath", "path", "file_path"])?;
    let old = property_name(schema, &["oldString", "old_string", "oldText", "old_text"])?;
    let new = property_name(schema, &["newString", "new_string", "newText", "new_text"])?;
    Ok(json!({file: "fixture.txt", old: "Status: before", new: "Status: after"}))
}

struct TempTree(PathBuf);

impl TempTree {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("cogfabric-opencode-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn path(&self, child: &str) -> PathBuf {
        self.0.join(child)
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn create_private_dirs(root: &Path) {
    for name in ["home", "data", "config", "cache", "state", "workspace"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in [".", "home", "data", "config", "cache", "state", "workspace"] {
            fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires installed OpenCode v2; run explicitly with --ignored"]
async fn installed_opencode_v2_executes_read_edit_read_against_the_api() {
    let tree = TempTree::new();
    create_private_dirs(&tree.0);
    let workspace = tree.path("workspace");
    let fixture_path = workspace.join("fixture.txt");
    fs::write(
        &fixture_path,
        "Cognitive Fabric OpenCode integration fixture.\nStatus: before\n",
    )
    .unwrap();

    let cfg = Config::default();
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(OpenCodeFixture {
        requests: Mutex::new(Vec::new()),
    });
    let runtime = AgentRuntime::with_backend(cfg.clone(), store.clone(), backend.clone()).unwrap();
    let server = Server::new(runtime, Engine::new(cfg, store), KEY).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server_task = tokio::spawn(serve_until(server, listener, async {
        let _ = stopped.await;
    }));

    let opencode_config = json!({
        "model": "cogfabric/cogfabric-agent",
        "providers": {
            "cogfabric": {
                "name": "Cognitive Fabric fixture",
                "env": ["COGFABRIC_SERVER_API_KEY"],
                "package": "@opencode/ai/providers/openai-compatible",
                "settings": {"baseURL": base},
                "models": {
                    "cogfabric-agent": {
                        "capabilities": {"tools": true, "input": ["text"], "output": ["text"]},
                        "limit": {"context": 32768, "output": 1024},
                        "variants": [{"id":"high", "settings":{"reasoningEffort":"high"}}]
                    }
                }
            }
        },
        "permissions": [
            {"action": "*", "resource": "*", "effect": "deny"},
            {"action": "read", "resource": "fixture.txt", "effect": "allow"},
            {"action": "edit", "resource": "fixture.txt", "effect": "allow"}
        ],
        "default_agent": "fabric-test",
        "agents": {
            "fabric-test": {
                "mode": "primary",
                "system": "Read fixture.txt, replace Status: before with Status: after, read it again, then report the new status. Only modify fixture.txt. Do not use shell, network, subagents, or any other tool.",
                "steps": 5
            }
        }
    });
    let config_path = workspace.join("opencode.json");
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&opencode_config).unwrap(),
    )
    .unwrap();

    let stdout_path = tree.path("opencode.stdout");
    let stderr_path = tree.path("opencode.stderr");
    let stdout = fs::File::create(&stdout_path).unwrap();
    let stderr = fs::File::create(&stderr_path).unwrap();
    let mut command =
        Command::new(std::env::var_os("OPENCODE_BIN").unwrap_or_else(|| "opencode".into()));
    command
        .current_dir(&workspace)
        .arg("run")
        .arg("--standalone")
        .arg("--agent")
        .arg("fabric-test")
        .arg("--model")
        .arg("cogfabric/cogfabric-agent#high")
        .arg("--format")
        .arg("json")
        .arg("Read fixture.txt, change Status: before to Status: after, read it again, and report the status.")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", tree.path("home"))
        .env("XDG_DATA_HOME", tree.path("data"))
        .env("XDG_CONFIG_HOME", tree.path("config"))
        .env("XDG_CACHE_HOME", tree.path("cache"))
        .env("XDG_STATE_HOME", tree.path("state"))
        .env("TMPDIR", &tree.0)
        .env("COGFABRIC_SERVER_API_KEY", KEY)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    // Keep the installed client bounded, and always stop the API fixture on failure.
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("could not launch OpenCode: {error}"));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    if status.is_none() {
        let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
        let _ = stop.send(());
        let _ = server_task.await;
        panic!("OpenCode exceeded its 90 second limit. stderr: {stderr}");
    }
    let output = fs::read_to_string(&stdout_path).unwrap_or_default();
    let errors = fs::read_to_string(&stderr_path).unwrap_or_default();
    let events: Vec<Value> = output
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let tool_events: Vec<&Value> = events
        .iter()
        .filter(|event| event["type"] == "tool_use")
        .collect();
    let executed_tools: Vec<&str> = tool_events
        .iter()
        .filter_map(|event| event["part"]["tool"].as_str())
        .collect();

    let requests = backend.requests.lock().unwrap().clone();
    let tool_requests: Vec<&Turn> = requests
        .iter()
        .filter(|request| {
            request
                .tools
                .iter()
                .any(|tool| matches!(tool.function.name.as_str(), "read" | "edit"))
        })
        .collect();
    assert!(
        !tool_requests.is_empty(),
        "OpenCode must send tool requests"
    );
    assert!(
        tool_requests
            .iter()
            .all(|request| request.reasoning_effort.as_deref() == Some("high")),
        "OpenCode high variant must send reasoning_effort on the same model"
    );
    let observed_tools: Vec<Vec<String>> = requests
        .iter()
        .map(|request| {
            request
                .tools
                .iter()
                .map(|tool| tool.function.name.clone())
                .collect()
        })
        .collect();
    assert!(
        status.unwrap().success(),
        "OpenCode failed. stdout: {output}\nstderr: {errors}\nobserved toolsets: {observed_tools:?}"
    );
    assert_eq!(
        fs::read_to_string(&fixture_path).unwrap(),
        "Cognitive Fabric OpenCode integration fixture.\nStatus: after\n",
        "OpenCode did not make the requested file edit; stdout: {output}; stderr: {errors}"
    );
    assert!(
        output.contains("Status: after"),
        "OpenCode did not report the edited status. stdout: {output}\nstderr: {errors}"
    );
    assert_eq!(
        executed_tools,
        ["read", "edit", "read"],
        "OpenCode must execute exactly the intended file actions; stdout: {output}; stderr: {errors}"
    );
    assert!(
        tool_events
            .iter()
            .all(|event| event["part"]["state"]["status"] == "completed"),
        "OpenCode reported a failed file action; stdout: {output}; stderr: {errors}"
    );
    assert!(
        !events.iter().any(|event| event["type"] == "error"),
        "OpenCode emitted an error event; stdout: {output}; stderr: {errors}"
    );
    assert!(
        tool_requests.len() >= 4,
        "Expected read, edit, read and final tool-enabled requests; got {} ({} total requests). Toolsets: {observed_tools:?}; stdout: {output}; stderr: {errors}",
        tool_requests.len(), requests.len()
    );
    for (index, expected) in ["read", "edit", "read"].iter().enumerate() {
        assert!(
            tool_requests[index]
                .tools
                .iter()
                .any(|tool| tool.function.name == *expected),
            "Request {} did not advertise {expected}; toolsets: {observed_tools:?}",
            index + 1
        );
    }
    let final_request = tool_requests[3];
    assert!(
        final_request.messages.iter().any(|message| {
            message.role == "tool"
                && message
                    .content
                    .as_deref()
                    .is_some_and(|content| content.contains("Status: after"))
        }),
        "The final inference request did not include the post-edit read result"
    );

    stop.send(()).unwrap();
    server_task.await.unwrap().unwrap();
}
