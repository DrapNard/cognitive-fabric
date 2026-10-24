//! HTTP transport only. Reasoning, policy and persistence live in `cogfabric`.
mod listener;
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Response, Sse,
    },
    routing::{get, post},
    Json, Router,
};
use cogfabric::{
    agent::{AgentRuntime, Message, Tool, Turn, TurnResult},
    engine::Engine,
};
use futures_util::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, watch, Semaphore};

pub const MODEL: &str = "cogfabric-agent";
#[derive(Clone)]
pub struct Server {
    agent: AgentRuntime,
    engine: Engine,
    token: [u8; 32],
    admission: Arc<Semaphore>,
    requests: Arc<Semaphore>,
    stop: Arc<watch::Sender<bool>>,
}
impl Server {
    pub fn new(agent: AgentRuntime, engine: Engine, token: &str) -> anyhow::Result<Self> {
        let agent = agent.with_resource_pool("human-cli", engine.configured_resource_pool()?)?;
        if !(32..=256).contains(&token.len())
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        {
            anyhow::bail!("Server API key must contain 32 to 256 bearer-safe characters")
        }
        Ok(Self {
            agent,
            engine,
            token: Sha256::digest(token.as_bytes()).into(),
            admission: Arc::new(Semaphore::new(8)),
            requests: Arc::new(Semaphore::new(16)),
            stop: Arc::new(watch::channel(false).0),
        })
    }
    pub fn router(self) -> Router {
        Router::new()
            .route("/v1/models", get(models))
            .route("/v1/chat/completions", post(chat))
            .fallback(|| async { error(StatusCode::NOT_FOUND, "Unknown API route") })
            .layer(DefaultBodyLimit::max(512 * 1024))
            .layer(middleware::from_fn_with_state(self.clone(), authorize))
            .with_state(self)
    }
}
pub fn validate_bind(addr: SocketAddr) -> anyhow::Result<()> {
    if !addr.ip().is_loopback() {
        anyhow::bail!("HTTP serving is restricted to numeric loopback addresses")
    }
    Ok(())
}
pub async fn serve_until(
    server: Server,
    listener: tokio::net::TcpListener,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    validate_bind(listener.local_addr()?)?;
    let stop = server.stop.clone();
    let served = axum::serve(listener::BoundedListener::new(listener), server.router())
        .with_graceful_shutdown(async move {
            shutdown.await;
            stop.send_replace(true);
        })
        .await;
    let cleanup = cogfabric::agent::process::drain_workers().await;
    let collective_cleanup = cogfabric::fabric::worker_cleanup::drain_workers().await;
    served?;
    cleanup?;
    collective_cleanup?;
    Ok(())
}
fn error(status: StatusCode, message: &str) -> Response {
    (status,Json(json!({"error":{"message":message,"type":"invalid_request_error","code":status.as_u16()}}))).into_response()
}
async fn authorize(State(server): State<Server>, request: Request, next: Next) -> Response {
    if request.headers().contains_key("origin") {
        return error(StatusCode::FORBIDDEN, "Browser origins are not enabled");
    }
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .filter(|s| s.len() <= 256);
    let digest: [u8; 32] = Sha256::digest(supplied.unwrap_or("").as_bytes()).into();
    // Fixed-size digest comparison avoids short-circuit secret comparisons.
    if supplied.is_none()
        || digest
            .iter()
            .zip(server.token)
            .fold(0u8, |n, (a, b)| n | (a ^ b))
            != 0
    {
        return error(
            StatusCode::UNAUTHORIZED,
            "A valid server bearer token is required",
        );
    }
    let _request = match server.requests.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "HTTP request capacity exhausted",
            )
        }
    };
    // Bound slow request bodies and pre-response processing. SSE bodies have their
    // own producer deadlines and disconnect cancellation after response headers.
    match tokio::time::timeout(Duration::from_secs(150), next.run(request)).await {
        Ok(r) => r,
        Err(_) => error(StatusCode::REQUEST_TIMEOUT, "Request deadline exhausted"),
    }
}
async fn models() -> Json<Value> {
    let models = [
        MODEL,
        "cogfabric-fast",
        "cogfabric-standard",
        "cogfabric-deep",
        "cogfabric-research",
    ]
    .iter()
    .map(|id| json!({"id":id,"object":"model","created":0,"owned_by":"cognitive-fabric"}))
    .collect::<Vec<_>>();
    Json(json!({"object":"list","data":models}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatRequest {
    model: String,
    messages: Vec<Value>,
    #[serde(default)]
    tools: Vec<Tool>,
    #[serde(default)]
    tool_choice: Option<Value>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    stream_options: Option<Value>,
    #[serde(default)]
    max_tokens: Option<usize>,
    #[serde(default)]
    max_completion_tokens: Option<usize>,
    #[serde(default)]
    temperature: Option<f64>,
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    n: Option<u32>,
    #[serde(default)]
    parallel_tool_calls: Option<bool>,
    #[serde(default)]
    store: Option<bool>,
}
fn normalize(raw: ChatRequest) -> anyhow::Result<(String, Turn, bool, bool)> {
    if raw.store == Some(true)
        || raw.n.is_some_and(|n| n != 1)
        || (raw.max_tokens.is_some() && raw.max_completion_tokens.is_some())
        || raw.messages.len() > 256
    {
        anyhow::bail!("Only one choice and one output-token limit are supported")
    }
    if raw.stream_options.as_ref().is_some_and(|v| {
        v.as_object().is_none_or(|o| {
            o.keys().any(|k| k != "include_usage")
                || o.get("include_usage").is_some_and(|n| !n.is_boolean())
        })
    }) {
        anyhow::bail!("Invalid stream options")
    }
    let usage = raw
        .stream_options
        .as_ref()
        .is_some_and(|v| v["include_usage"] == true);
    let mut messages = Vec::new();
    for mut m in raw.messages {
        if m.get("tool_calls").is_some_and(Value::is_null) {
            m.as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("Invalid message"))?
                .remove("tool_calls");
        }
        if let Some(parts) = m.get("content").and_then(Value::as_array) {
            let mut text = String::new();
            for part in parts {
                if !part.as_object().is_some_and(|object| object.len() == 2)
                    || part["type"] != "text"
                    || part["text"].as_str().is_none()
                {
                    anyhow::bail!("Only text conversation content is supported")
                }
                text.push_str(part["text"].as_str().unwrap());
            }
            m["content"] = json!(text);
        }
        messages.push(
            serde_json::from_value::<Message>(m)
                .map_err(|_| anyhow::anyhow!("Unsupported conversation message fields"))?,
        );
    }
    // This flag narrows how many proposals may be returned; it never enables execution.
    let request = Turn {
        messages,
        tools: raw.tools,
        tool_choice: raw.tool_choice,
        max_tokens: raw.max_tokens.or(raw.max_completion_tokens),
        temperature: raw.temperature,
        reasoning_effort: raw.reasoning_effort,
        parallel_tool_calls: raw.parallel_tool_calls,
    };
    request.validate()?;
    Ok((raw.model, request, raw.stream, usage && raw.stream))
}
fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn response(result: TurnResult, model: &str) -> Value {
    let c = result.completion;
    json!({"id":format!("chatcmpl-{}",result.task_id),"object":"chat.completion","created":timestamp(),"model":model,
        "choices":[{"index":0,"message":{"role":"assistant","content":c.content,"tool_calls":if c.tool_calls.is_empty(){Value::Null}else{json!(c.tool_calls)}},"finish_reason":c.finish_reason}],
        "usage":usage(c.input_tokens,c.output_tokens)})
}
fn usage(input: Option<u64>, output: Option<u64>) -> Value {
    match (input, output) {
        (Some(i), Some(o)) => {
            json!({"prompt_tokens":i,"completion_tokens":o,"total_tokens":i.saturating_add(o)})
        }
        _ => Value::Null,
    }
}
async fn execute(
    server: &Server,
    model: &str,
    request: Turn,
    cancel: impl std::future::Future<Output = ()>,
) -> anyhow::Result<TurnResult> {
    let mut stopped = server.stop.subscribe();
    let cancel = async move {
        if *stopped.borrow() {
            return;
        }
        tokio::select! { _ = cancel => (), _ = stopped.changed() => () }
    };
    if model == MODEL {
        return server.agent.run_until(request, cancel).await;
    }
    let mode = model
        .strip_prefix("cogfabric-")
        .filter(|m| matches!(*m, "fast" | "standard" | "deep" | "research"))
        .ok_or_else(|| anyhow::anyhow!("Unknown served model"))?;
    validate_collective_request(&request)?;
    let result = server
        .engine
        .run_until(
            request.messages[0].content.as_deref().unwrap_or(""),
            mode,
            cancel,
        )
        .await?;
    if !matches!(
        result.status.as_str(),
        "completed" | "unverified" | "demo" | "needs_clarification"
    ) {
        anyhow::bail!("Collective task did not produce an answer")
    }
    Ok(TurnResult {
        task_id: result.task_id,
        completion: cogfabric::agent::Completion {
            content: Some(result.answer),
            tool_calls: Vec::new(),
            model: model.into(),
            finish_reason: "stop".into(),
            input_tokens: result.resources.input_tokens,
            output_tokens: result.resources.output_tokens,
        },
    })
}
fn validate_collective_request(request: &Turn) -> anyhow::Result<()> {
    if !request.tools.is_empty()
        || request.messages.len() != 1
        || request.messages[0].role != "user"
        || request.messages[0]
            .content
            .as_deref()
            .is_none_or(|s| s.trim().is_empty() || s.len() > 12000)
        || request.max_tokens.is_some()
        || request.temperature.is_some()
        || request.reasoning_effort.is_some()
        || request.tool_choice.as_ref().is_some_and(|v| v != "none")
    {
        anyhow::bail!("Collective models accept one user goal and operator-configured budgets; use cogfabric-agent for conversations and tools")
    }
    Ok(())
}

async fn chat(State(server): State<Server>, headers: HeaderMap, bytes: Bytes) -> Response {
    if !headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(';').next() == Some("application/json"))
    {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected application/json",
        );
    }
    let raw = match cogfabric::strict_json::value(&bytes, 512 * 1024)
        .and_then(|value| Ok(serde_json::from_value::<ChatRequest>(value)?))
    {
        Ok(v) => v,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "Invalid or unsupported chat request",
            )
        }
    };
    let parallel = raw.parallel_tool_calls.unwrap_or(true);
    let (model, request, stream, include_usage) = match normalize(raw) {
        Ok(v) => v,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "Invalid conversation, tool definitions or request options",
            )
        }
    };
    if ![
        MODEL,
        "cogfabric-fast",
        "cogfabric-standard",
        "cogfabric-deep",
        "cogfabric-research",
    ]
    .contains(&model.as_str())
    {
        return error(StatusCode::NOT_FOUND, "Unknown served model");
    }
    if model == MODEL && server.agent.validate(&request).is_err() {
        return error(
            StatusCode::BAD_REQUEST,
            "Request exceeds core admission limits",
        );
    }
    if model != MODEL && validate_collective_request(&request).is_err() {
        return error(StatusCode::BAD_REQUEST,"Collective models accept one user goal with operator-configured budgets; use cogfabric-agent for conversations and tools");
    }
    let permit = match server.admission.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "Server request capacity exhausted",
            )
        }
    };
    if !stream {
        let _permit = permit;
        return match execute(&server, &model, request, std::future::pending()).await {
            Ok(r) if parallel || r.completion.tool_calls.len() <= 1 => {
                Json(response(r, &model)).into_response()
            }
            _ => error(
                StatusCode::BAD_GATEWAY,
                "Core inference did not produce a valid completion; check provider context length, model availability and operator limits",
            ),
        };
    }
    let (tx, rx) = mpsc::channel::<String>(4);
    tokio::spawn(async move {
        let _permit = permit;
        let result = execute(&server, &model, request, tx.closed()).await;
        let produce = async {
            match result {
                Ok(r) if parallel || r.completion.tool_calls.len() <= 1 => {
                    let full = response(r, &model);
                    let id = full["id"].clone();
                    let created = full["created"].clone();
                    let chunk = |delta: Value, finish: Value| {
                        json!({"id":id,"object":"chat.completion.chunk","created":created,"model":model,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]}).to_string()
                    };
                    if tx
                        .send(chunk(json!({"role":"assistant"}), Value::Null))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    if let Some(text) = full["choices"][0]["message"]["content"].as_str() {
                        // Buffered provider completion, incrementally framed for client compatibility.
                        for part in text.chars().collect::<Vec<_>>().chunks(1024) {
                            if tx
                                .send(chunk(
                                    json!({"content":part.iter().collect::<String>()}),
                                    Value::Null,
                                ))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    if let Some(calls) = full["choices"][0]["message"]["tool_calls"].as_array() {
                        for (index, call) in calls.iter().enumerate() {
                            let mut call = call.clone();
                            call["index"] = json!(index);
                            if tx
                                .send(chunk(json!({"tool_calls":[call]}), Value::Null))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    if tx
                        .send(chunk(
                            json!({}),
                            full["choices"][0]["finish_reason"].clone(),
                        ))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    if include_usage {
                        let _=tx.send(json!({"id":id,"object":"chat.completion.chunk","created":created,"model":model,"choices":[],"usage":full["usage"]}).to_string()).await;
                    }
                }
                _ => {
                    let _=tx.send(json!({"error":{"message":"Core inference did not produce a valid completion; check provider context length, model availability and operator limits","type":"server_error"}}).to_string()).await;
                }
            }
            let _ = tx.send("[DONE]".into()).await;
        };
        let _ = tokio::time::timeout(Duration::from_secs(30), produce).await;
    });
    let events = stream::unfold(rx, |mut rx| async {
        rx.recv()
            .await
            .map(|data| (Ok::<_, Infallible>(Event::default().data(data)), rx))
    });
    Sse::new(events)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(5)))
        .into_response()
}
