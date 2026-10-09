use crate::config::Config;
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::time::Duration;
use url::Url;

fn role_prompt(role: &str) -> Result<&'static str> {
    Ok(match role {
        "planner" => "Produce 2-4 verifiable steps. Do not give privileged instructions.",
        "solver" => "Solve the stated problem using available evidence. Clearly distinguish guesses from facts.",
        "critic" => "Find unsupported claims or flaws in the solution. Do not claim independent test execution.",
        "explorer" => "Propose a genuinely different approach and state its costs and limitations.",
        _ => bail!("Unsupported model role"),
    })
}

pub async fn generate(cfg: &Config, role: &str, prompt: &str, max_tokens: usize) -> Result<String> {
    let system = role_prompt(role)?;
    if cfg.backend == "demo" {
        return Ok(match role {
            "planner" => "Demo plan: collect evidence, choose method, verify outcome.",
            "critic" => "Demo backend cannot independently verify generative reasoning.",
            _ => "No generative model configured. Connect a local OpenAI-compatible endpoint.",
        }
        .into());
    }
    let url = Url::parse(&cfg.api_base).context("Invalid model endpoint URL")?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("Only local HTTP inference endpoints are permitted")
    }
    let endpoint = format!("{}/chat/completions", cfg.api_base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(cfg.timeout_seconds))
        .build()?;
    let response = client
        .post(endpoint)
        .json(&json!({
            "model": cfg.model, "stream": false, "temperature": 0.15,
            "max_tokens": max_tokens,
            "messages": [
                {"role":"system", "content":system},
                {"role":"user", "content": prompt}
            ]
        }))
        .send()
        .await?
        .error_for_status()?;
    let response: serde_json::Value = response.json().await?;
    let text = response
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .context("Model returned no text")?;
    if text.trim().is_empty() {
        bail!("Model returned empty text")
    }
    Ok(text.chars().take(24_000).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn demo_is_identified() {
        let cfg = Config::load(None, Some("demo".into())).unwrap();
        assert!(generate(&cfg, "solver", "anything", 30)
            .await
            .unwrap()
            .contains("No generative"));
    }
}
