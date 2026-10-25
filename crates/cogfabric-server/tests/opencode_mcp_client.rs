//! Opt-in installed-client interoperability: OpenCode owns MCP and skill loading,
//! while CGF supplies only an OpenAI-compatible model endpoint.
//!
//! Run with `cargo test -p cogfabric-server --test opencode_mcp_client -- --ignored`.
//! The MCP fixture is loopback-only and returns synthetic REA/web observations.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
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
use tokio::sync::oneshot;

const KEY: &str = "fixture-opencode-mcp-key-012345678901234567890";
const SKILL_SENTINEL: &str = "CGF_SKILL_CONTEXT_7b63c2";
const REA_SENTINEL: &str = "REA_FIXTURE_RESULT_15e9a1";
const WEB_SENTINEL: &str = "WEB_FIXTURE_RESULT_42c7bd";

#[derive(Default)]
struct OpenCodeFixture {
    requests: Mutex<Vec<Turn>>,
}

impl ChatBackend for OpenCodeFixture {
    fn complete<'a>(
        &'a self,
        request: &'a Turn,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Completion>> + Send + 'a>> {
        let request = request.clone();
        self.requests.lock().unwrap().push(request.clone());
        Box::pin(async move {
            let invoked = invoked_tool_names(&request);
            let next = if !invoked.iter().any(|name| name == "skill") {
                request
                    .tools
                    .iter()
                    .find(|tool| tool.function.name == "skill")
                    .and_then(|tool| {
                        skill_arguments(&request)
                            .ok()
                            .map(|arguments| (tool.function.name.clone(), arguments))
                    })
            } else if invoked.iter().filter(|name| *name == "execute").count() < 2 {
                request
                    .tools
                    .iter()
                    .find(|tool| tool.function.name == "execute")
                    .map(|tool| {
                        let call_count = invoked.iter().filter(|name| *name == "execute").count();
                        (
                            tool.function.name.clone(),
                            execute_arguments(call_count > 0),
                        )
                    })
            } else {
                None
            };
            if let Some((name, arguments)) = next {
                return Ok(Completion {
                    content: None,
                    tool_calls: vec![ToolCall {
                        id: format!("fixture_call_{}", invoked.len() + 1),
                        kind: "function".into(),
                        function: FunctionCall {
                            name,
                            arguments: arguments.to_string(),
                        },
                    }],
                    model: "fixture/opencode-v2".into(),
                    finish_reason: "tool_calls".into(),
                    input_tokens: Some(24),
                    output_tokens: Some(8),
                });
            }
            Ok(Completion {
                content: Some(format!(
                    "{} Tool catalog seen by CGF: {:?}. {REA_SENTINEL} {WEB_SENTINEL}",
                    if serialized_turn(&request).contains(SKILL_SENTINEL) {
                        "Loaded the client skill and completed both external lookups."
                    } else {
                        "Completed both external lookups."
                    },
                    request
                        .tools
                        .iter()
                        .map(|tool| &tool.function.name)
                        .collect::<Vec<_>>()
                )),
                tool_calls: vec![],
                model: "fixture/opencode-v2".into(),
                finish_reason: "stop".into(),
                input_tokens: Some(32),
                output_tokens: Some(12),
            })
        })
    }
}

fn invoked_tool_names(request: &Turn) -> Vec<String> {
    request
        .messages
        .iter()
        .flat_map(|message| message.tool_calls.iter())
        .map(|call| call.function.name.clone())
        .collect()
}

fn serialized_turn(request: &Turn) -> String {
    serde_json::to_string(request).unwrap_or_default()
}

fn skill_tool_result_contains_marker(request: &Turn) -> bool {
    let skill_call_ids: Vec<&str> = request
        .messages
        .iter()
        .flat_map(|message| message.tool_calls.iter())
        .filter(|call| call.function.name == "skill")
        .map(|call| call.id.as_str())
        .collect();
    request.messages.iter().any(|message| {
        message.role == "tool"
            && message
                .tool_call_id
                .as_deref()
                .is_some_and(|id| skill_call_ids.contains(&id))
            && message
                .content
                .as_deref()
                .is_some_and(|content| content.contains(SKILL_SENTINEL))
    })
}

fn skill_arguments(request: &Turn) -> anyhow::Result<Value> {
    let schema = request
        .tools
        .iter()
        .find(|tool| tool.function.name == "skill")
        .map(|tool| &tool.function.parameters)
        .ok_or_else(|| anyhow::anyhow!("OpenCode did not advertise its skill tool"))?;
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("OpenCode skill tool has no property schema: {schema}"))?;
    let key = ["id", "name", "skill", "skill_name"]
        .iter()
        .find(|key| properties.contains_key(**key))
        .ok_or_else(|| anyhow::anyhow!("No skill-name parameter in OpenCode schema: {schema}"))?;
    Ok(json!({(*key): "cgf-workflow"}))
}

fn execute_arguments(discovered: bool) -> Value {
    let code = if discovered {
        "const rea = await tools.cognitive_fabric_fixture.rea_inspect({target: \"fixture.txt\"}); const web = await tools.cognitive_fabric_fixture.web_lookup({query: \"synthetic local documentation lookup\"}); return {rea, web};"
    } else {
        "return await search({query: \"REA fixture inspection and synthetic web lookup\"});"
    };
    json!({"code": code})
}

#[derive(Default)]
struct McpFixture {
    messages: Mutex<Vec<Value>>,
    calls: Mutex<Vec<(String, Value)>>,
}

async fn mcp_post(
    State(state): State<Arc<McpFixture>>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    let _accept = headers.get(axum::http::header::ACCEPT);
    state.messages.lock().unwrap().push(message.clone());
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let method = message["method"].as_str().unwrap_or_default();
    if method == "notifications/initialized" || message.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let response = match method {
        "initialize" => json!({
            "jsonrpc": "2.0", "id": id,
            "result": {
                "protocolVersion": message["params"]["protocolVersion"],
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "cognitive-fabric-mcp-fixture", "version": "1.0.0"}
            }
        }),
        "tools/list" => json!({
            "jsonrpc": "2.0", "id": id,
            "result": {"tools": [
                {
                    "name": "rea_inspect",
                    "description": "Inspect a local artifact with the REA fixture; returns synthetic inspection evidence.",
                    "inputSchema": {
                        "type": "object", "properties": {"target": {"type":"string","description":"Local fixture path"}},
                        "required": ["target"], "additionalProperties": false
                    }
                },
                {
                    "name": "web_lookup",
                    "description": "Search synthetic web documentation fixture results; performs no internet access.",
                    "inputSchema": {
                        "type": "object", "properties": {"query": {"type":"string","description":"Documentation query"}},
                        "required": ["query"], "additionalProperties": false
                    }
                }
            ]}
        }),
        "tools/call" => {
            let name = message["params"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let arguments = message["params"]["arguments"].clone();
            state.calls.lock().unwrap().push((name.clone(), arguments));
            let (text, structured) = match name.as_str() {
                "rea_inspect" => (
                    format!("{REA_SENTINEL}: fixture.txt contains no executable payload."),
                    json!({"result": REA_SENTINEL, "target": "fixture.txt", "synthetic": true}),
                ),
                "web_lookup" => (
                    format!("{WEB_SENTINEL}: synthetic documentation confirms the fixture workflow."),
                    json!({"result": WEB_SENTINEL, "source": "local synthetic fixture", "synthetic": true}),
                ),
                _ => {
                    return (
                        StatusCode::OK,
                        Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unknown fixture tool"}})),
                    )
                        .into_response()
                }
            };
            json!({
                "jsonrpc":"2.0", "id":id,
                "result":{"content":[{"type":"text","text":text}],"structuredContent":structured,"isError":false}
            })
        }
        _ => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unknown fixture method"}})
        }
    };
    (StatusCode::OK, Json(response)).into_response()
}

struct TempTree(PathBuf);

impl TempTree {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "cogfabric-opencode-mcp-{}-{nonce}",
            std::process::id()
        )))
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

fn redact(text: &str) -> String {
    text.replace(KEY, "<redacted fixture credential>")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires installed OpenCode v2; run explicitly with --ignored"]
async fn installed_opencode_v2_owns_mcp_and_skill_workflow() {
    let tree = TempTree::new();
    create_private_dirs(&tree.0);
    let workspace = tree.path("workspace");
    fs::create_dir_all(workspace.join(".opencode/skills/cgf-workflow")).unwrap();
    fs::write(
        workspace.join(".opencode/skills/cgf-workflow/SKILL.md"),
        format!(
            "---\nname: cgf-workflow\ndescription: Use the isolated CGF workflow fixture\n---\nWhen answering, preserve this marker: {SKILL_SENTINEL}."
        ),
    )
    .unwrap();
    fs::write(
        workspace.join("opencode.json"),
        json!({
            "$schema": "https://opencode.ai/config.json",
            "update": "disable",
            "model": "cogfabric/cogfabric-agent",
            "permissions": [
                {"action":"*","resource":"*","effect":"deny"},
                {"action":"skill","resource":"cgf-workflow","effect":"allow"},
                {"action":"cognitive_fabric_fixture_*","resource":"*","effect":"allow"},
                {"action":"execute","resource":"*","effect":"allow"}
            ],
            "agents": {
                "fabric-test": {
                    "mode": "primary",
                    "steps": 8
                }
            },
            "providers": {
                "cogfabric": {
                    "name": "Cognitive Fabric fixture",
                    "env": ["COGFABRIC_SERVER_API_KEY"],
                    "package": "@opencode/ai/providers/openai-compatible",
                    "settings": {"baseURL": "http://127.0.0.1:1/v1"},
                    "models": {
                        "cogfabric-agent": {
                            "name": "CGF fixture model",
                            "capabilities": {"tools": true, "input": ["text"], "output": ["text"]},
                            "limit": {"context": 32768, "output": 1024},
                            "variants": [{"id":"high", "settings":{"reasoningEffort":"high"}}]
                        }
                    }
                }
            },
            "mcp": {
                "servers": {
                    "cognitive_fabric_fixture": {
                        "type":"remote",
                        "url":"http://127.0.0.1:1/mcp",
                        "oauth": false,
                        "codemode": true
                    }
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    // Replace placeholder ports after binding both loopback listeners.
    let cfg = Config::default();
    let store = Arc::new(Store::in_memory().unwrap());
    let backend = Arc::new(OpenCodeFixture::default());
    let runtime = AgentRuntime::with_backend(cfg.clone(), store.clone(), backend.clone()).unwrap();
    let server = Server::new(runtime, Engine::new(cfg, store.clone()), KEY).unwrap();
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_base = format!("http://{}/v1", api_listener.local_addr().unwrap());
    let (api_stop, api_stopped) = oneshot::channel::<()>();
    let api_task = tokio::spawn(serve_until(server, api_listener, async move {
        let _ = api_stopped.await;
    }));

    let mcp_fixture = Arc::new(McpFixture::default());
    let mcp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mcp_addr = mcp_listener.local_addr().unwrap();
    let mcp_router = Router::new()
        .route("/mcp", post(mcp_post))
        .with_state(mcp_fixture.clone());
    let (mcp_stop, mcp_stopped) = oneshot::channel::<()>();
    let mcp_task = tokio::spawn(async move {
        axum::serve(mcp_listener, mcp_router)
            .with_graceful_shutdown(async move {
                let _ = mcp_stopped.await;
            })
            .await
    });

    let config_path = workspace.join("opencode.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["providers"]["cogfabric"]["settings"]["baseURL"] = json!(api_base);
    config["mcp"]["servers"]["cognitive_fabric_fixture"]["url"] =
        json!(format!("http://{mcp_addr}/mcp"));
    fs::write(&config_path, config.to_string()).unwrap();

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
        .arg("Load the cgf-workflow skill. Then call both the REA inspection and web lookup MCP tools. Report both result markers and preserve the marker supplied by the skill.")
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
        .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
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
    let output = redact(&fs::read_to_string(&stdout_path).unwrap_or_default());
    let errors = redact(&fs::read_to_string(&stderr_path).unwrap_or_default());
    let events: Vec<Value> = output
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if status.is_none() {
        let _ = api_stop.send(());
        let _ = mcp_stop.send(());
        let _ = api_task.await;
        let _ = mcp_task.await;
        panic!("OpenCode exceeded its 90 second limit. stderr: {errors}");
    }

    let requests = backend.requests.lock().unwrap().clone();
    let mcp_calls = mcp_fixture.calls.lock().unwrap().clone();
    let tool_events: Vec<String> = events
        .iter()
        .filter(|event| event["type"] == "tool_use")
        .filter_map(|event| event["part"]["tool"].as_str().map(str::to_owned))
        .collect();
    let serialized_requests = requests
        .iter()
        .map(serialized_turn)
        .collect::<Vec<_>>()
        .join("\n");

    let status = status.unwrap();
    let observed_toolsets: Vec<Vec<String>> = requests
        .iter()
        .map(|request| {
            request
                .tools
                .iter()
                .map(|tool| tool.function.name.clone())
                .collect()
        })
        .collect();
    let observed_schemas: Vec<Value> = requests
        .iter()
        .map(|request| serde_json::to_value(&request.tools).unwrap_or(Value::Null))
        .collect();
    assert!(
        status.success(),
        "OpenCode failed ({status}). CGF requests: {}; CGF listener task finished: {}; MCP methods: {:?}; CGF toolsets: {observed_toolsets:?}. stdout: {output}\nstderr: {errors}",
        requests.len(),
        api_task.is_finished(),
        mcp_fixture.messages.lock().unwrap().iter().filter_map(|message| message["method"].as_str()).collect::<Vec<_>>()
    );
    assert_eq!(
        mcp_calls
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["rea_inspect", "web_lookup"],
        "OpenCode must execute the MCP fixture calls itself; CGF toolsets: {observed_toolsets:?}; schemas: {observed_schemas:?}; events: {tool_events:?}; stdout: {output}"
    );
    assert!(
        tool_events.iter().any(|name| name == "skill")
            && tool_events.iter().any(|name| name == "execute"),
        "OpenCode did not report skill and Code Mode execution: {tool_events:?}; stdout: {output}; stderr: {errors}"
    );
    assert!(
        tool_events
            .iter()
            .all(|name| name == "skill" || name == "execute"),
        "Denied built-in actions unexpectedly executed: {tool_events:?}"
    );
    assert!(
        requests
            .iter()
            .filter(|request| !request.tools.is_empty())
            .all(|request| request.reasoning_effort.as_deref() == Some("high")),
        "OpenCode changed or omitted the selected model's high reasoning effort"
    );
    assert!(
        requests.iter().any(|request| {
            request.tools.iter().any(|tool| {
                tool.function.name == "execute"
                    && tool
                        .function
                        .description
                        .to_ascii_lowercase()
                        .contains("code")
            })
        }),
        "OpenCode did not advertise Code Mode to the CGF model backend"
    );
    assert!(
        requests.iter().any(skill_tool_result_contains_marker),
        "The client's skill tool result did not carry skill instructions into a model turn"
    );
    assert!(
        serialized_requests.contains(REA_SENTINEL) && serialized_requests.contains(WEB_SENTINEL),
        "MCP results did not survive in the conversation passed back to CGF"
    );
    assert!(
        output.contains(SKILL_SENTINEL)
            && output.contains(REA_SENTINEL)
            && output.contains(WEB_SENTINEL),
        "Final response omitted loaded skill or MCP result context: {output}"
    );
    assert!(
        !events.iter().any(|event| event["type"] == "error"),
        "OpenCode emitted an error event: {output}\nstderr: {errors}"
    );
    assert!(
        mcp_fixture
            .messages
            .lock()
            .unwrap()
            .iter()
            .any(|message| message["method"] == "tools/list"),
        "OpenCode did not discover tool schemas from its configured MCP server"
    );
    assert!(
        !output.contains(KEY) && !errors.contains(KEY),
        "The fixture credential appeared in client output"
    );

    api_stop.send(()).unwrap();
    mcp_stop.send(()).unwrap();
    api_task.await.unwrap().unwrap();
    mcp_task.await.unwrap().unwrap();
}
