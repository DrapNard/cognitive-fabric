//! Bounded, tool-free client for the local Cognitive Fabric chat API.
//!
//! This deliberately uses the same loopback URL validation, Tokio runtime,
//! reqwest transport defaults and strict JSON parser as the inference adapters.

use anyhow::{bail, Context, Result};
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    Client, RequestBuilder,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use url::Url;

/// Default model; configurable through Options.
pub const MODEL_ALIAS: &str = "cogfabric-agent";
/// Legacy default capacities, retained for source compatibility; Limits may override them.
pub const MAX_HTTP_RESPONSE_BYTES: usize = 256 * 1024;
pub const MAX_TRANSCRIPT_BYTES: usize = 32 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// Configurable client limits. Server-side budgets remain independent.
#[derive(Clone, Debug)]
pub struct Limits {
    pub http_response_bytes: usize,
    pub transcript_bytes: usize,
    pub message_bytes: usize,
    pub messages: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            http_response_bytes: MAX_HTTP_RESPONSE_BYTES,
            transcript_bytes: MAX_TRANSCRIPT_BYTES,
            message_bytes: MAX_MESSAGE_BYTES,
            messages: 32,
        }
    }
}

impl Limits {
    pub fn validate(&self) -> Result<()> {
        if !(1024..=4 * 1024 * 1024).contains(&self.http_response_bytes)
            || !(256..=256 * 1024).contains(&self.message_bytes)
            || !(1024..=512 * 1024).contains(&self.transcript_bytes)
            || !(2..=256).contains(&self.messages)
            || self.message_bytes > self.transcript_bytes
            || self.message_bytes > self.http_response_bytes
        {
            bail!("Invalid chat byte/message limits or inconsistent message capacity")
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub model: String,
    pub reasoning_effort: Option<String>,
    /// Name of the environment variable holding the secret, never its value.
    pub api_key_env: String,
    pub limits: Limits,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            model: MODEL_ALIAS.into(),
            reasoning_effort: None,
            api_key_env: "COGFABRIC_SERVER_API_KEY".into(),
            limits: Limits::default(),
        }
    }
}

/// A text-only message sent to the native conversation endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// Server bearer credentials are intentionally not printable or serializable.
struct ServerKey(String);

impl ServerKey {
    fn from_environment(name: &str) -> Result<Self> {
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .enumerate()
                .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
        {
            bail!("Invalid API key environment-variable name")
        }
        let value = std::env::var(name)
            .with_context(|| format!("Required API key environment variable is missing: {name}"))?;
        if value.is_empty()
            || value.len() > 256
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
        {
            bail!("API key environment variable has an invalid format")
        }
        Ok(Self(value))
    }

    fn apply(&self, request: RequestBuilder) -> Result<RequestBuilder> {
        let mut header = HeaderValue::from_str(&format!("Bearer {}", self.0))
            .map_err(|_| anyhow::anyhow!("Invalid server authorization header"))?;
        header.set_sensitive(true);
        Ok(request.header(AUTHORIZATION, header))
    }

    fn reject_echo(&self, bytes: &[u8]) -> Result<()> {
        if bytes
            .windows(self.0.len())
            .any(|window| window == self.0.as_bytes())
        {
            bail!("Server response contained protected credential material")
        }
        Ok(())
    }
}

pub struct ClientApi {
    client: Client,
    endpoint: Url,
    key: ServerKey,
    max_output_tokens: usize,
    options: Options,
}

impl ClientApi {
    /// Connects only to an HTTP loopback endpoint and refuses redirects/proxies.
    pub fn new(server_base: &str, timeout: Duration, max_output_tokens: usize) -> Result<Self> {
        Self::with_options(server_base, timeout, max_output_tokens, Options::default())
    }

    pub fn with_options(
        server_base: &str,
        timeout: Duration,
        max_output_tokens: usize,
        options: Options,
    ) -> Result<Self> {
        options.limits.validate()?;
        validate_effort(options.reasoning_effort.as_deref())?;
        if timeout < Duration::from_secs(1)
            || timeout > Duration::from_secs(3600)
            || !(1..=16384).contains(&max_output_tokens)
            || options.model.is_empty()
            || options.model.len() > 128
            || !options
                .model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
        {
            bail!("Chat client timeout, tokens or model is outside supported bounds")
        }
        let endpoint = crate::backend::local_endpoint(server_base)?;
        let key = ServerKey::from_environment(&options.api_key_env)?;
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()?;
        Ok(Self {
            client,
            endpoint,
            key,
            max_output_tokens,
            options,
        })
    }

    /// Changes only the requested provider effort; model and resource ceilings stay pinned.
    pub fn set_reasoning_effort(&mut self, effort: Option<&str>) -> Result<()> {
        validate_effort(effort)?;
        self.options.reasoning_effort = effort.map(str::to_owned);
        Ok(())
    }

    /// Makes one bounded completion request without advertising or accepting tools.
    pub async fn complete(&self, messages: &[Message], temperature: f64) -> Result<String> {
        if messages.is_empty()
            || messages.len() > self.options.limits.messages
            || messages.iter().any(|message| {
                message.content.is_empty()
                    || message.content.len() > self.options.limits.message_bytes
            })
            || serde_json::to_vec(messages)?.len() > self.options.limits.transcript_bytes
            || !temperature.is_finite()
            || !(0.0..=2.0).contains(&temperature)
        {
            bail!("Chat transcript or generation settings exceed supported limits")
        }

        let mut request_body = json!({
            "model": self.options.model,
            "messages": messages,
            "max_tokens": self.max_output_tokens,
            "temperature": temperature,
            "stream": false,
            "store": false
        });
        if let Some(effort) = &self.options.reasoning_effort {
            request_body["reasoning_effort"] = json!(effort);
        }
        let request = self
            .client
            .post(self.endpoint.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&request_body);
        let response = self.key.apply(request)?.send().await?;
        if !response.status().is_success() {
            bail!(
                "Chat server returned HTTP status {}",
                response.status().as_u16()
            )
        }
        if response
            .content_length()
            .is_some_and(|length| length > self.options.limits.http_response_bytes as u64)
        {
            bail!("Chat server response exceeds the byte limit")
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len()
                > self
                    .options
                    .limits
                    .http_response_bytes
                    .saturating_sub(body.len())
            {
                bail!("Chat server response exceeds the byte limit")
            }
            body.extend_from_slice(&chunk);
        }
        self.key.reject_echo(&body)?;
        let value = crate::strict_json::value(&body, self.options.limits.http_response_bytes)
            .map_err(|_| anyhow::anyhow!("Chat server returned invalid JSON"))?;
        self.key.reject_echo(&serde_json::to_vec(&value)?)?;
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .filter(|choices| choices.len() == 1)
            .ok_or_else(|| anyhow::anyhow!("Chat server returned an invalid completion"))?;
        let choice = &choices[0];
        let message = choice
            .get("message")
            .ok_or_else(|| anyhow::anyhow!("Chat server returned an invalid completion"))?;
        if message.get("role").and_then(Value::as_str) != Some("assistant")
            || choice.get("finish_reason").and_then(Value::as_str) == Some("tool_calls")
        {
            bail!("Chat server returned an unsupported response")
        }
        match message.get("tool_calls") {
            None | Some(Value::Null) => {}
            Some(Value::Array(calls)) if calls.is_empty() => {}
            _ => bail!("Chat server returned an unsupported response"),
        }
        let content = message
            .get("content")
            .and_then(Value::as_str)
            .filter(|text| {
                !text.trim().is_empty() && text.len() <= self.options.limits.message_bytes
            })
            .ok_or_else(|| anyhow::anyhow!("Chat server returned no bounded text reply"))?;
        Ok(content.to_owned())
    }
}

fn validate_effort(effort: Option<&str>) -> Result<()> {
    if effort.is_some_and(|value| !crate::agent::valid_reasoning_effort(value)) {
        bail!("Reasoning effort must be none, low, medium or high")
    }
    Ok(())
}
