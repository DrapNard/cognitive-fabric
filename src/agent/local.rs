//! Native conversation adapter; HTTP JSON is confined to the provider boundary.
use super::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};
pub struct LocalChat(pub Config);
impl ChatBackend for LocalChat {
    fn complete<'a>(
        &'a self,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async move {
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
            if let Some(effort) = &self.0.reasoning_effort {
                body["reasoning_effort"] = json!(effort);
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
            let response: Value = serde_json::from_slice(&bytes)
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
