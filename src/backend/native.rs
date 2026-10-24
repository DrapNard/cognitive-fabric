//! LM Studio's native, instance-checked chat adapter.
//!
//! The adapter has no provider integrations or tool execution path. The loaded
//! instance ID is pinned by the host and checked against every native response;
//! it is not an attestation of model weights or provider identity.
use super::{
    system_prompt, Generation, InferenceBackend, InferenceTarget, NativeModelTarget,
    ProviderMetricSource, ProviderMetrics,
};
use crate::{config::Config, credentials::Bearer};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};
use url::Url;

const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_PROMPT_BYTES: usize = 32_000;
const MAX_OUTPUT_TOKENS: usize = 16_384;
const MAX_OUTPUT_CHARS: usize = 24_000;
const RESIDENCY_CREDENTIAL_ENV: &str = "COGFABRIC_RESIDENCY_API_KEY";

#[derive(Clone)]
pub struct NativeBackend {
    config: Config,
    targets: BTreeMap<String, NativeModelTarget>,
    authenticate: bool,
    dispatch: Arc<DispatchState>,
}

#[derive(Default)]
struct DispatchState {
    started: AtomicU64,
    confirmed: AtomicU64,
}

impl NativeBackend {
    /// Pin the configured model key and exact loaded native instance.
    pub fn new(
        config: Config,
        model_instance_id: impl Into<String>,
        authenticate: bool,
    ) -> Result<Self> {
        let model = config.model.clone();
        Self::with_target(config, model, model_instance_id, authenticate)
    }

    /// Construct an adapter for an explicit loaded model key and instance.
    /// Role profiles may adjust generation settings, but cannot select another
    /// model because this adapter is bound to one loaded instance.
    pub fn with_target(
        config: Config,
        model: impl Into<String>,
        model_instance_id: impl Into<String>,
        authenticate: bool,
    ) -> Result<Self> {
        config.validate()?;
        let model = model.into();
        let target = NativeModelTarget {
            api_base: config.api_base.clone(),
            model,
            model_instance_id: model_instance_id.into(),
        }
        .canonical()?;
        let targets = crate::fabric::protocol::ROLES
            .iter()
            .map(|role| ((*role).to_string(), target.clone()))
            .collect();
        Self::with_role_targets(config, targets, authenticate)
    }

    /// Construct one backend whose roles each use an explicitly pinned native
    /// instance. Every role must be mapped so fallback cannot select another
    /// role's model instance.
    pub fn with_role_targets(
        config: Config,
        targets: BTreeMap<String, NativeModelTarget>,
        authenticate: bool,
    ) -> Result<Self> {
        config.validate()?;
        if config.input_token_admission.is_some() {
            bail!("Input-token admission is unsupported by the native LM Studio adapter")
        }
        if targets.len() != crate::fabric::protocol::ROLES.len() {
            bail!("Native adapter requires one target for every supported role")
        }
        for role in &crate::fabric::protocol::ROLES {
            let target = targets
                .get(*role)
                .ok_or_else(|| anyhow::anyhow!("Native adapter role target is missing"))?
                .canonical()?;
            if config
                .role_profiles
                .get(*role)
                .is_some_and(|profile| profile.model != target.model)
            {
                bail!("Native adapter role profile does not match its pinned model key")
            }
            let expected_base = NativeModelTarget {
                api_base: config.api_base.clone(),
                model: target.model.clone(),
                model_instance_id: target.model_instance_id.clone(),
            }
            .canonical()?
            .api_base;
            if target.api_base != expected_base {
                bail!("Native adapter role targets must share the configured endpoint")
            }
        }
        Ok(Self {
            config,
            targets: targets
                .into_iter()
                .map(|(role, target)| Ok((role, target.canonical()?)))
                .collect::<Result<_>>()?,
            authenticate,
            dispatch: Arc::new(DispatchState::default()),
        })
    }

    pub fn target(&self, role: &str) -> Result<&NativeModelTarget> {
        self.targets
            .get(role)
            .ok_or_else(|| anyhow::anyhow!("Unsupported model role"))
    }

    /// Return true once any request may have reached the provider without a
    /// complete, validated response. A count difference preserves parallel
    /// dispatches and dropped request futures instead of clearing a shared flag.
    pub fn has_uncertain_dispatch(&self) -> bool {
        self.dispatch.started.load(Ordering::SeqCst)
            > self.dispatch.confirmed.load(Ordering::SeqCst)
    }

    fn chat_endpoint(&self, target: &NativeModelTarget) -> Result<Url> {
        let target = target.canonical()?;
        let mut endpoint = Url::parse(&target.api_base)?;
        endpoint.set_path("/api/v1/chat");
        Ok(endpoint)
    }

    fn credential(&self) -> Result<Option<Bearer>> {
        if !self.authenticate {
            return Ok(None);
        }
        let value = std::env::var(RESIDENCY_CREDENTIAL_ENV)
            .map_err(|_| anyhow::anyhow!("Dedicated residency credential is unavailable"))?;
        Ok(Some(Bearer::new(value)?))
    }

    async fn generate_pinned(
        &self,
        role: &str,
        prompt: &str,
        max_tokens: usize,
    ) -> Result<Generation> {
        let start = Instant::now();
        crate::fabric::protocol::actor(role)?;
        if prompt.len() > MAX_PROMPT_BYTES || max_tokens == 0 || max_tokens > MAX_OUTPUT_TOKENS {
            bail!("Inference request exceeds limits")
        }

        let target = self.target(role)?;
        let system = system_prompt(role)?;
        let profile = self.config.role_profiles.get(role);
        let temperature = profile
            .map(|profile| profile.temperature)
            .unwrap_or(self.config.temperature);
        let mut payload = json!({
            "model": target.model,
            "input": prompt,
            "system_prompt": system,
            "stream": false,
            "temperature": temperature,
            "max_output_tokens": max_tokens,
            "store": false,
            "integrations": []
        });
        if let Some(effort) = &self.config.reasoning_effort {
            payload["reasoning"] = json!(if effort == "none" { "off" } else { effort });
        }

        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(self.config.timeout_seconds))
            .build()?;
        let credential = self.credential()?;
        let mut request = client.post(self.chat_endpoint(target)?).json(&payload);
        if let Some(credential) = &credential {
            request = credential.apply(request)?;
        }
        self.dispatch.started.fetch_add(1, Ordering::SeqCst);
        let mut response = request.send().await?;
        if !response.status().is_success() {
            bail!("Native inference endpoint returned a non-success status")
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            bail!("Native inference response exceeds byte limit")
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(body.len()) {
                bail!("Native inference response exceeds byte limit")
            }
            body.extend_from_slice(&chunk);
        }
        if let Some(credential) = &credential {
            credential.reject_echo(&body)?;
        }
        let value = crate::strict_json::value(&body, MAX_RESPONSE_BYTES)
            .map_err(|_| anyhow::anyhow!("Invalid native inference JSON response"))?;
        if let Some(credential) = &credential {
            credential.reject_echo(&serde_json::to_vec(&value)?)?;
        }
        let response: NativeResponse = serde_json::from_value(value)
            .map_err(|_| anyhow::anyhow!("Invalid native inference response shape"))?;
        if response.model_instance_id != target.model_instance_id {
            bail!("Native inference response came from a different model instance")
        }
        if response.response_id.is_some() {
            bail!("Native inference server stored a response despite storage being disabled")
        }

        let mut messages = Vec::new();
        for item in response.output {
            match item.get("type").and_then(Value::as_str) {
                Some("message") => {
                    let message: MessageOutput = serde_json::from_value(item)
                        .map_err(|_| anyhow::anyhow!("Invalid native message output"))?;
                    if message.kind != "message" {
                        bail!("Invalid native message output")
                    }
                    if !message.content.trim().is_empty() {
                        messages.push(message.content);
                    }
                }
                Some("reasoning") => {
                    // Parse the documented shape, then discard it. Reasoning is
                    // never returned as an answer or included in error text.
                    let reasoning: ReasoningOutput = serde_json::from_value(item)
                        .map_err(|_| anyhow::anyhow!("Invalid native reasoning output"))?;
                    if reasoning.kind != "reasoning" {
                        bail!("Invalid native reasoning output")
                    }
                }
                Some("tool_call") | Some("invalid_tool_call") => {
                    bail!("Native inference returned a tool output; tools are disabled")
                }
                _ => bail!("Native inference returned an unsupported output form"),
            }
        }
        let text = messages.join("\n");
        if text.trim().is_empty() {
            bail!("Native model returned no answer")
        }
        if response.stats.as_ref().is_some_and(|stats| {
            [
                stats.tokens_per_second,
                stats.time_to_first_token_seconds,
                stats.model_load_time_seconds,
            ]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite() || value < 0.0)
        }) {
            bail!("Native inference returned invalid usage metrics")
        }
        let provider_metrics = response
            .stats
            .as_ref()
            .map(lm_studio_provider_metrics)
            .transpose()?
            .flatten();
        if response
            .stats
            .as_ref()
            .and_then(|stats| stats.total_output_tokens)
            .is_some_and(|tokens| tokens > max_tokens as u64)
        {
            bail!("Native inference exceeded the requested output token limit")
        }

        let generation = Generation {
            text: text.chars().take(MAX_OUTPUT_CHARS).collect(),
            // The configured model key is trusted adapter metadata. Provider
            // text cannot replace the selected model or attest its weights.
            model: target.model.clone(),
            input_tokens: response.stats.as_ref().and_then(|stats| stats.input_tokens),
            output_tokens: response
                .stats
                .as_ref()
                .and_then(|stats| stats.total_output_tokens),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            queue_wait_ms: None,
            provider_metrics,
        };
        self.dispatch.confirmed.fetch_add(1, Ordering::SeqCst);
        Ok(generation)
    }
}

fn lm_studio_provider_metrics(stats: &NativeStats) -> Result<Option<ProviderMetrics>> {
    let mut metrics = ProviderMetrics::new(ProviderMetricSource::LmStudioNative);
    metrics.output_tokens_per_second = stats.tokens_per_second;
    metrics.time_to_first_token_ms = stats
        .time_to_first_token_seconds
        .map(|value| value * 1000.0);
    metrics.model_load_ms = stats.model_load_time_seconds.map(|value| value * 1000.0);
    if metrics.output_tokens_per_second.is_none()
        && metrics.time_to_first_token_ms.is_none()
        && metrics.model_load_ms.is_none()
    {
        return Ok(None);
    }
    metrics.validate()?;
    Ok(Some(metrics))
}

impl InferenceBackend for NativeBackend {
    fn has_uncertain_dispatch(&self) -> bool {
        NativeBackend::has_uncertain_dispatch(self)
    }

    fn inference_target(&self, role: &str) -> Result<Option<InferenceTarget>> {
        crate::fabric::protocol::actor(role)?;
        Ok(Some(InferenceTarget::Native(
            self.target(role)?.clone().canonical()?,
        )))
    }

    fn pin(&self) -> Result<Option<Arc<dyn InferenceBackend>>> {
        Ok(Some(Arc::new(self.clone())))
    }

    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(self.generate_pinned(role, prompt, max_tokens))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeResponse {
    model_instance_id: String,
    output: Vec<Value>,
    #[serde(default)]
    stats: Option<NativeStats>,
    #[serde(default)]
    response_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageOutput {
    #[serde(rename = "type")]
    kind: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReasoningOutput {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "content")]
    _content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeStats {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    total_output_tokens: Option<u64>,
    #[serde(default)]
    #[serde(rename = "reasoning_output_tokens")]
    _reasoning_output_tokens: Option<u64>,
    #[serde(default)]
    tokens_per_second: Option<f64>,
    #[serde(default)]
    time_to_first_token_seconds: Option<f64>,
    #[serde(default)]
    model_load_time_seconds: Option<f64>,
}

#[cfg(test)]
mod metric_tests {
    use super::*;

    #[test]
    fn native_metrics_preserve_absence_and_convert_seconds_to_milliseconds() {
        let empty: NativeStats = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(lm_studio_provider_metrics(&empty).unwrap().is_none());
        let stats: NativeStats = serde_json::from_value(serde_json::json!({
            "tokens_per_second": 12.5,
            "time_to_first_token_seconds": 0.125,
            "model_load_time_seconds": 2.0
        }))
        .unwrap();
        let metrics = lm_studio_provider_metrics(&stats).unwrap().unwrap();
        assert_eq!(metrics.time_to_first_token_ms, Some(125.0));
        assert_eq!(metrics.model_load_ms, Some(2000.0));
        assert_eq!(metrics.output_tokens_per_second, Some(12.5));
        assert_eq!(metrics.generation_ms, None);
        for value in [-1.0, f64::INFINITY, f64::NAN, 86401.0] {
            let mut invalid: NativeStats = serde_json::from_value(serde_json::json!({})).unwrap();
            invalid.model_load_time_seconds = Some(value);
            assert!(lm_studio_provider_metrics(&invalid).is_err());
        }
    }
}
