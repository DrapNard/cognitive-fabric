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

pub const MODEL_ALIAS: &str = "cogfabric-agent";
pub const MAX_HTTP_RESPONSE_BYTES: usize = 256 * 1024;
pub const MAX_TRANSCRIPT_BYTES: usize = 32 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;

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
    fn from_environment() -> Result<Self> {
        let value = std::env::var("COGFABRIC_SERVER_API_KEY")
            .context("COGFABRIC_SERVER_API_KEY is required")?;
        if value.is_empty()
            || value.len() > 256
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
        {
            bail!("COGFABRIC_SERVER_API_KEY has an invalid format")
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
}

impl ClientApi {
    /// Connects only to an HTTP loopback endpoint and refuses redirects/proxies.
    pub fn new(server_base: &str, timeout: Duration, max_output_tokens: usize) -> Result<Self> {
        if !(1..=120).contains(&timeout.as_secs()) || !(16..=512).contains(&max_output_tokens) {
            bail!("Chat client limits are outside the supported bounds")
        }
        let endpoint = crate::backend::local_endpoint(server_base)?;
        let key = ServerKey::from_environment()?;
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
        })
    }

    /// Makes one bounded completion request without advertising or accepting tools.
    pub async fn complete(&self, messages: &[Message], temperature: f64) -> Result<String> {
        if messages.is_empty()
            || messages.len() > 32
            || messages.iter().any(|message| {
                message.content.is_empty() || message.content.len() > MAX_MESSAGE_BYTES
            })
            || serde_json::to_vec(messages)?.len() > MAX_TRANSCRIPT_BYTES
            || !temperature.is_finite()
            || !(0.0..=2.0).contains(&temperature)
        {
            bail!("Chat transcript or generation settings exceed supported limits")
        }

        let request_body = json!({
            "model": MODEL_ALIAS,
            "messages": messages,
            "max_tokens": self.max_output_tokens,
            "temperature": temperature,
            "stream": false,
            "store": false
        });
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
            .is_some_and(|length| length > MAX_HTTP_RESPONSE_BYTES as u64)
        {
            bail!("Chat server response exceeds the byte limit")
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > MAX_HTTP_RESPONSE_BYTES.saturating_sub(body.len()) {
                bail!("Chat server response exceeds the byte limit")
            }
            body.extend_from_slice(&chunk);
        }
        self.key.reject_echo(&body)?;
        let value = crate::strict_json::value(&body, MAX_HTTP_RESPONSE_BYTES)
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
            .filter(|text| !text.trim().is_empty() && text.len() <= MAX_MESSAGE_BYTES)
            .ok_or_else(|| anyhow::anyhow!("Chat server returned no bounded text reply"))?;
        Ok(content.to_owned())
    }
}
