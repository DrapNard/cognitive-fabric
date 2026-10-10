//! Bounded operator diagnostics; model advertisement is not inference validation.
use crate::{backend, config::Config, security, store::Store};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Duration};

#[derive(Deserialize)]
struct Models {
    data: Vec<Model>,
}
#[derive(Deserialize)]
struct Model {
    id: String,
}

async fn advertised(cfg: &Config) -> Result<BTreeSet<String>> {
    let mut endpoint = backend::local_endpoint(&cfg.api_base)?;
    let path = endpoint
        .path()
        .strip_suffix("chat/completions")
        .unwrap()
        .to_string();
    endpoint.set_path(&format!("{path}models"));
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()?;
    let mut response = client.get(endpoint).send().await?;
    if !response.status().is_success() {
        bail!("Model discovery returned a non-success status")
    }
    if response.content_length().is_some_and(|n| n > 65536) {
        bail!("Model discovery response exceeds limit")
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 65536 {
            bail!("Model discovery response exceeds limit")
        }
        bytes.extend_from_slice(&chunk);
    }
    let models: Models = serde_json::from_slice(&bytes)?;
    if models.data.len() > 256
        || models.data.iter().any(|m| {
            m.id.trim().is_empty() || m.id.len() > 256 || m.id.chars().any(char::is_control)
        })
    {
        bail!("Invalid model discovery bounds")
    }
    let count = models.data.len();
    let ids = models
        .data
        .into_iter()
        .map(|m| m.id)
        .collect::<BTreeSet<_>>();
    if ids.len() != count {
        bail!("Duplicate advertised model identity")
    }
    Ok(ids)
}

pub async fn diagnose(cfg: &Config, store: &Store, probe_inference: bool) -> Result<Value> {
    cfg.validate()?;
    let registration = store.pin_model(cfg);
    let storage = store.storage_stats()?;
    let schema: u32 = store
        .lock()?
        .pragma_query_value(None, "user_version", |row| row.get(0))?;
    let probe = if !probe_inference {
        json!({"status":"not_requested"})
    } else if cfg.backend == "demo" {
        json!({"status":"not_applicable","reason":"Demo backend does not use an inference server"})
    } else {
        security::authorize("human-cli", "inference.diagnose")?;
        match advertised(cfg).await {
            Ok(ids) => {
                let required = std::iter::once(&cfg.model)
                    .chain(cfg.role_profiles.values().map(|profile| &profile.model))
                    .collect::<BTreeSet<_>>();
                let missing = required
                    .into_iter()
                    .filter(|id| !ids.contains(*id))
                    .collect::<Vec<_>>();
                json!({"status":"responded","advertised_models":ids,"missing_configured_models":missing,"all_configured_models_advertised":missing.is_empty()})
            }
            Err(error) => {
                json!({"status":"failed","error":error.to_string().chars().take(300).collect::<String>()})
            }
        }
    };
    Ok(
        json!({"format_version":1,"backend":cfg.backend,"schema_version":schema,"storage":storage,
        "registered_selection_valid":registration.is_ok(),"registration_error":registration.err().map(|e| e.to_string()),
        "inference_discovery":probe,"generation_tested":false,"weights_attested":false,
        "limitations":["Model advertisement does not prove loaded weights or successful generation","Database statistics are not an integrity or backup test","Socket health requires service-status"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn offline_diagnostics_create_no_tasks_or_events() {
        let store = Store::in_memory().unwrap();
        let report = diagnose(&Config::default(), &store, false).await.unwrap();
        assert_eq!(report["inference_discovery"]["status"], "not_requested");
        assert_eq!(report["generation_tested"], false);
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            0
        );
        assert!(security::authorize("jev-r", "inference.diagnose").is_err());
    }
    #[tokio::test]
    async fn model_discovery_is_bounded_and_never_follows_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for (status, body, expected) in [
            ("200 OK", r#"{"data":[{"id":"fixture"}]}"#, "responded"),
            (
                "200 OK",
                r#"{"data":[{"id":"fixture"},{"id":"fixture"}]}"#,
                "failed",
            ),
            ("302 Found", "", "failed"),
            ("200 OK", "not JSON", "failed"),
            ("200 OK", "oversize", "failed"),
            ("200 OK", r#"{"data":[{"id":"other"}]}"#, "responded"),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let cfg = Config {
                backend: "openai-local".into(),
                model: "fixture".into(),
                api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
                ..Config::default()
            };
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                let n = stream.read(&mut request).await.unwrap();
                assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /v1/models "));
                let length = if body == "oversize" {
                    65537
                } else {
                    body.len()
                };
                let response = format!("HTTP/1.1 {status}\r\nLocation: http://192.0.2.1/\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}");
                stream.write_all(response.as_bytes()).await.unwrap();
            });
            let report = diagnose(&cfg, &Store::in_memory().unwrap(), true)
                .await
                .unwrap();
            assert_eq!(report["inference_discovery"]["status"], expected);
            if expected == "responded" {
                assert_eq!(
                    report["inference_discovery"]["all_configured_models_advertised"],
                    body.contains("fixture")
                );
            }
            server.await.unwrap();
        }
    }
}
