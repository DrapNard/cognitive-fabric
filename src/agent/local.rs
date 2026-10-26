//! Native conversation adapter; HTTP JSON is confined to the provider boundary.
use super::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};
pub struct LocalChat(pub Config);
impl ChatBackend for LocalChat {
    fn pin(self: Arc<Self>) -> Result<Option<Arc<dyn ChatBackend>>> {
        Ok(Some(self))
    }
    fn inference_target(&self) -> Result<Option<crate::backend::InferenceTarget>> {
        if self.0.backend != "openai-local" {
            return Ok(None);
        }
        crate::backend::configured_target(&self.0, None)
    }
    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        self.0.input_token_admission
    }
    fn complete_for<'a>(
        &'a self,
        task: &'a str,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async move {
            if self.0.input_token_admission.is_none() {
                return self.complete(request, tokens).await;
            }
            if self.0.backend != "openai-local" {
                bail!("Token-admitted agent turns require an OpenAI-compatible local backend")
            }
            validate_token_request(&self.0, request)?;
            let temperature = request.temperature.unwrap_or(self.0.temperature);
            let (generation, finish_reason) = crate::token_admission::generate_with_finish(
                &self.0,
                task,
                serde_json::to_value(&request.messages)?,
                tokens,
                temperature,
            )
            .await?;
            Ok(Completion {
                content: Some(generation.text),
                tool_calls: Vec::new(),
                model: generation.model,
                finish_reason: finish_reason.into(),
                input_tokens: generation.input_tokens,
                output_tokens: generation.output_tokens,
            })
        })
    }
    fn complete<'a>(
        &'a self,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async move {
            if self.0.input_token_admission.is_some() {
                bail!("Token-admitted chat requires an explicit task identity")
            }
            if self.0.backend != "openai-local" {
                bail!("Native agent turns require an OpenAI-compatible local backend")
            }
            let endpoint = crate::backend::local_endpoint(&self.0.api_base)?;
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(self.0.timeout_seconds))
                .build()?;
            let mut body = json!({"model":self.0.model,"messages":request.messages,"max_tokens":tokens,"temperature":request.temperature.unwrap_or(self.0.temperature),"stream":false});
            if !request.tools.is_empty() {
                body["tools"] = json!(request.tools);
                body["tool_choice"] = request.tool_choice.clone().unwrap_or(json!("auto"));
            }
            if let Some(parallel) = request.parallel_tool_calls {
                body["parallel_tool_calls"] = json!(parallel);
            }
            if let Some(effort) = request
                .reasoning_effort
                .as_ref()
                .or(self.0.reasoning_effort.as_ref())
            {
                body["reasoning_effort"] = json!(self.0.provider_effort(effort)?);
            }
            let credential = crate::credentials::resolve(&self.0)?;
            let mut http = client.post(endpoint).json(&body);
            if let Some(c) = &credential {
                http = c.apply(http)?;
            }
            let mut response = http.send().await?;
            if !response.status().is_success() {
                bail!("Agent inference provider returned a non-success status")
            }
            let mut bytes = Vec::new();
            if response.content_length().is_some_and(|n| n > 256 * 1024) {
                bail!("Agent provider response exceeds limits")
            }
            while let Some(chunk) = response.chunk().await? {
                if chunk.len() > (256 * 1024usize).saturating_sub(bytes.len()) {
                    bail!("Agent provider response exceeds limits")
                }
                bytes.extend_from_slice(&chunk);
            }
            if let Some(c) = &credential {
                c.reject_echo(&bytes)?;
            }
            let response: Value = crate::strict_json::value(&bytes, 256 * 1024)
                .map_err(|_| anyhow::anyhow!("Invalid agent provider JSON"))?;
            if let Some(c) = &credential {
                c.reject_echo(&serde_json::to_vec(&response)?)?;
            }
            let choices = response["choices"]
                .as_array()
                .filter(|v| v.len() == 1)
                .ok_or_else(|| anyhow::anyhow!("Expected one agent completion"))?;
            let choice = &choices[0];
            if choice["message"]["role"] != "assistant" {
                bail!("Invalid provider message role")
            }
            let content = match &choice["message"]["content"] {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => bail!("Unsupported provider content"),
            };
            Ok(Completion {
                content,
                tool_calls: match &choice["message"]["tool_calls"] {
                    Value::Null => Vec::new(),
                    v => serde_json::from_value(v.clone())
                        .map_err(|_| anyhow::anyhow!("Invalid provider tool calls"))?,
                },
                model: response["model"].as_str().unwrap_or(&self.0.model).into(),
                finish_reason: choice["finish_reason"].as_str().unwrap_or("").into(),
                input_tokens: response["usage"]["prompt_tokens"].as_u64(),
                output_tokens: response["usage"]["completion_tokens"].as_u64(),
            })
        })
    }
}

pub(super) fn validate_token_request(cfg: &Config, request: &Turn) -> Result<()> {
    if cfg.input_token_admission.is_none() {
        return Ok(());
    }
    if !request.tools.is_empty()
        || request
            .tool_choice
            .as_ref()
            .is_some_and(|choice| choice != "none")
        || request
            .messages
            .iter()
            .any(|message| !message.tool_calls.is_empty() || message.tool_call_id.is_some())
    {
        bail!("Token-admitted chat supports tool-free conversations only")
    }
    if request
        .reasoning_effort
        .as_ref()
        .or(cfg.reasoning_effort.as_ref())
        .is_some_and(|effort| effort != "none")
    {
        bail!("Token-admitted chat supports reasoning effort none only")
    }
    Ok(())
}

#[cfg(test)]
mod duplicate_provider_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn provider_duplicate_members_fail_without_returning_ambiguous_content() {
        for duplicate in [true, false] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    assert!(bytes.len() < 65536);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|n| n.parse::<usize>().ok())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            let request: Value =
                                serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                            assert!(request["messages"].is_array());
                            break;
                        }
                    }
                }
                let body = if duplicate {
                    r#"{"model":"fixture","choices":[{"message":{"role":"assistant","content":"safe","content":"ambiguous"},"finish_reason":"stop"}]}"#
                } else {
                    r#"{"model":"fixture","choices":[{"message":{"role":"assistant","content":"safe"},"finish_reason":"stop"}]}"#
                };
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
            });
            let backend = LocalChat(Config {
                backend: "openai-local".into(),
                api_base: endpoint,
                ..Config::default()
            });
            let turn = Turn {
                messages: vec![Message {
                    role: "user".into(),
                    content: Some("Read".into()),
                    tool_calls: vec![],
                    tool_call_id: None,
                }],
                tools: vec![],
                tool_choice: None,
                max_tokens: None,
                temperature: None,
                parallel_tool_calls: None,
                reasoning_effort: None,
            };
            let result = tokio::time::timeout(Duration::from_secs(3), backend.complete(&turn, 128))
                .await
                .unwrap();
            if duplicate {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    "Invalid agent provider JSON"
                );
            } else {
                assert_eq!(result.unwrap().content.as_deref(), Some("safe"));
            }
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn per_turn_effort_overrides_config_and_omission_uses_config_without_changing_model_or_budget(
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let provider = tokio::spawn(async move {
            for expected_effort in ["high", "low", "none", "high"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    assert!(bytes.len() < 65536);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|n| n.parse::<usize>().ok())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            let request: Value =
                                serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                            assert_eq!(request["model"], "same-configured-model");
                            assert_eq!(request["max_tokens"], 128);
                            assert_eq!(request["reasoning_effort"], expected_effort);
                            assert_eq!(request["tools"][0]["function"]["name"], "rea_inspect");
                            assert_eq!(
                                request["tools"][0]["function"]["parameters"]["$defs"]["target"]
                                    ["type"],
                                "string"
                            );
                            assert_eq!(
                                request["messages"][0]["content"],
                                "Client skill instructions"
                            );
                            assert_eq!(request["messages"][3]["content"], "{\"isError\":true,\"content\":[{\"type\":\"text\",\"text\":\"Web lookup unavailable\"}]}");
                            assert_eq!(request["messages"][3]["tool_call_id"], "web_previous");
                            assert_eq!(request["parallel_tool_calls"], false);
                            break;
                        }
                    }
                }
                let body = r#"{"model":"same-configured-model","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#;
                socket
                    .write_all(
                        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let backend = LocalChat(Config {
            backend: "openai-local".into(),
            api_base: endpoint,
            model: "same-configured-model".into(),
            reasoning_effort: Some("low".into()),
            ..Config::default()
        });

        for per_turn_effort in [Some("high"), None, Some("none"), Some("high")] {
            let turn: Turn = serde_json::from_value(json!({
                "messages":[
                    {"role":"system","content":"Client skill instructions"},
                    {"role":"user","content":"Inspect with REA after checking the web"},
                    {"role":"assistant","tool_calls":[{"id":"web_previous","type":"function","function":{"name":"removed_web_tool","arguments":"{}"}}]},
                    {"role":"tool","tool_call_id":"web_previous","content":"{\"isError\":true,\"content\":[{\"type\":\"text\",\"text\":\"Web lookup unavailable\"}]}"}
                ],
                "tools":[{"type":"function","function":{
                    "name":"rea_inspect","description":"Client-owned MCP inspection",
                    "parameters":{"type":"object","$defs":{"target":{"type":"string"}},"properties":{"target":{"$ref":"#/$defs/target"}}}
                }}],
                "parallel_tool_calls":false,
                "reasoning_effort":per_turn_effort,
            }))
            .unwrap();
            let completion =
                tokio::time::timeout(Duration::from_secs(3), backend.complete(&turn, 128))
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(completion.model, "same-configured-model");
        }
        provider.await.unwrap();
    }
}
