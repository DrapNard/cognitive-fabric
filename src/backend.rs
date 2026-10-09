use crate::config::Config;
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::time::Duration;
use url::Url;

const MAX_RESPONSE_BYTES: usize = 256 * 1024;

fn local_endpoint(base: &str) -> Result<Url> {
    let mut url = Url::parse(base).context("Invalid model endpoint URL")?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        // Resolve no hostnames: a configured DNS resolver is outside this boundary.
        Some(url::Host::Domain("localhost")) => {
            url.set_host(Some("127.0.0.1"))?;
            true
        }
        _ => false,
    };
    if url.scheme() != "http"
        || !loopback
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        bail!("Only local HTTP inference endpoints without credentials or queries are permitted")
    }
    url.set_path(&format!(
        "{}/chat/completions",
        url.path().trim_end_matches('/')
    ));
    Ok(url)
}

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
    if prompt.len() > 32_000 || max_tokens == 0 || max_tokens > 16_384 {
        bail!("Inference request exceeds limits")
    }
    if cfg.backend == "demo" {
        return Ok(match role {
            "planner" => "Demo plan: collect evidence, choose method, verify outcome.",
            "critic" => "Demo backend cannot independently verify generative reasoning.",
            _ => "No generative model configured. Connect a local OpenAI-compatible endpoint.",
        }
        .into());
    }
    if cfg.backend != "openai-local" {
        bail!("Unsupported inference backend")
    }
    let endpoint = local_endpoint(&cfg.api_base)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(cfg.timeout_seconds))
        .build()?;
    let mut response = client
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
    if !response.status().is_success() {
        bail!("Inference endpoint returned a non-success status")
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        bail!("Inference response exceeds byte limit")
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(body.len()) {
            bail!("Inference response exceeds byte limit")
        }
        body.extend_from_slice(&chunk);
    }
    let response: serde_json::Value = serde_json::from_slice(&body)?;
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
    #[test]
    fn endpoints_remain_on_literal_loopback() {
        for base in [
            "http://127.0.0.1:1234/v1",
            "http://[::1]:8080/v1",
            "http://localhost:1234/v1",
        ] {
            let endpoint = local_endpoint(base).unwrap();
            assert!(endpoint.path().ends_with("/v1/chat/completions"));
            assert!(!matches!(endpoint.host(), Some(url::Host::Domain(_))));
        }
        for base in [
            "https://127.0.0.1/v1",
            "http://example.com/v1",
            "http://192.168.1.1/v1",
            "http://user@localhost/v1",
            "http://localhost/v1?secret=x",
            "http://localhost/v1#fragment",
        ] {
            assert!(local_endpoint(base).is_err(), "{base}");
        }
    }

    async fn fixture_response(response: Vec<u8>) -> anyhow::Result<String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("HTTP fixture requires permission to bind loopback");
        let mut cfg = Config::load(None, Some("openai-local".into()))?;
        cfg.api_base = format!("http://{}/v1", listener.local_addr()?);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 1024];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0, "Fixture request ended early");
                request.extend_from_slice(&chunk[..count]);
                assert!(request.len() <= 8192, "Fixture request exceeds limit");
                if let Some(header_end) = request.windows(4).position(|s| s == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .expect("Fixture expects Content-Length")
                        .trim()
                        .parse()
                        .unwrap();
                    if request.len() >= header_end + 4 + length {
                        break;
                    }
                }
            }
            let _ = socket.write_all(&response).await;
        });
        let result = generate(&cfg, "solver", "test", 32).await;
        server.await?;
        result
    }

    #[tokio::test]
    async fn redirects_and_oversized_responses_are_rejected() {
        assert!(fixture_response(
            b"HTTP/1.1 302 Found\r\nLocation: http://example.com/\r\nContent-Length: 0\r\n\r\n"
                .to_vec()
        )
        .await
        .is_err());
        assert!(
            fixture_response(b"HTTP/1.1 200 OK\r\nContent-Length: 99999999\r\n\r\n".to_vec())
                .await
                .is_err()
        );
        let body = "x".repeat(MAX_RESPONSE_BYTES + 1);
        let response = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{body}");
        assert!(fixture_response(response.into_bytes()).await.is_err());
    }

    #[tokio::test]
    async fn compatible_http_response_is_parsed() {
        let body = r#"{"choices":[{"message":{"content":"test answer"}}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        assert_eq!(
            fixture_response(response.into_bytes()).await.unwrap(),
            "test answer"
        );
    }
    #[tokio::test]
    async fn demo_is_identified() {
        let cfg = Config::load(None, Some("demo".into())).unwrap();
        assert!(generate(&cfg, "solver", "anything", 30)
            .await
            .unwrap()
            .contains("No generative"));
    }
}
