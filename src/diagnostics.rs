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
        json!({"format_version":1,"backend":cfg.backend,"schema_version":schema,"storage":storage,"process_memory":crate::telemetry::process_memory(),
        "registered_selection_valid":registration.is_ok(),"registration_error":registration.err().map(|e| e.to_string()),
        "inference_discovery":probe,"generation_tested":false,"weights_attested":false,
        "limitations":["Model advertisement does not prove loaded weights or successful generation","Database statistics are not an integrity or backup test","Socket health requires service-status","Process memory excludes external inference servers and GPU memory"]}),
    )
}

/// Inspect operator-registered endpoints without loading models or creating tasks.
pub async fn registered_endpoints(store: &Store) -> Result<Value> {
    security::authorize("human-cli", "inference.diagnose")?;
    let mut endpoints = std::collections::BTreeMap::<String, Vec<crate::models::Registered>>::new();
    let mut disabled = 0;
    for model in store.models()? {
        if model.enabled {
            endpoints
                .entry(model.manifest.api_base.clone())
                .or_default()
                .push(model);
        } else {
            disabled += 1;
        }
    }
    if endpoints.len() > 8 {
        bail!("Registered endpoint diagnostics allows at most eight enabled endpoints")
    }
    let mut pending = tokio::task::JoinSet::new();
    for (endpoint, registrations) in endpoints {
        pending.spawn(async move {
            let cfg = Config { api_base:endpoint.clone(), ..Config::default() };
            let started = tokio::time::Instant::now();
            let response = advertised(&cfg).await;
            let required = registrations.iter().map(|model| model.manifest.model.clone()).collect::<BTreeSet<_>>();
            let identities = registrations.iter().map(|model| json!({"id":model.manifest.id,"version":model.manifest.version,"checksum":model.checksum,"requested_model":model.manifest.model})).collect::<Vec<_>>();
            match response {
                Ok(ids) => {
                    let missing = required.difference(&ids).cloned().collect::<Vec<_>>();
                    json!({"api_base":endpoint,"status":"responded","registrations":identities,"advertised_models":ids,"missing_registered_models":missing,"all_registered_models_advertised":missing.is_empty(),"elapsed_ms":started.elapsed().as_secs_f64()*1000.0})
                }
                Err(error) => json!({"api_base":endpoint,"status":"failed","registrations":identities,"error":error.to_string().chars().take(300).collect::<String>(),"all_registered_models_advertised":false,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0}),
            }
        });
    }
    let mut reports = Vec::new();
    while let Some(report) = pending.join_next().await {
        reports.push(report?);
    }
    reports.sort_by(|left, right| left["api_base"].as_str().cmp(&right["api_base"].as_str()));
    let healthy = reports
        .iter()
        .all(|report| report["all_registered_models_advertised"] == true);
    Ok(
        json!({"status":if reports.is_empty() {"not_applicable"} else if healthy {"responded"} else {"failed"},"endpoints":reports,"disabled_registrations_skipped":disabled,"generation_tested":false,"weights_attested":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn registered_fixture(store: &Store, id: &str, model: &str, endpoint: &str) {
        let file = std::env::temp_dir().join(format!(
            "fabric-diagnostic-model-{}.json",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&file,json!({"id":id,"version":"1.0.0","model":model,"api_base":endpoint,"source":"fixture","license":"CC0","context_tokens":4096,"max_output_tokens":512}).to_string()).unwrap();
        store.register_model(&file).unwrap();
        std::fs::remove_file(file).unwrap();
    }
    #[tokio::test]
    async fn registered_discovery_deduplicates_endpoints_and_reports_missing_models() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let store = Store::in_memory().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        registered_fixture(&store, "domain.present", "fixture", &endpoint);
        registered_fixture(&store, "domain.missing", "missing-model", &endpoint);
        registered_fixture(
            &store,
            "domain.disabled",
            "disabled-model",
            "http://127.0.0.1:9/v1",
        );
        store
            .set_model_enabled(
                &crate::models::Selection {
                    id: "domain.disabled".into(),
                    version: "1.0.0".into(),
                },
                false,
            )
            .unwrap();
        let provider = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let count = socket.read(&mut request).await.unwrap();
            assert!(std::str::from_utf8(&request[..count])
                .unwrap()
                .starts_with("GET /v1/models "));
            let body = r#"{"data":[{"id":"fixture"}]}"#;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        });
        let report = registered_endpoints(&store).await.unwrap();
        provider.await.unwrap();
        assert_eq!(report["status"], "failed");
        assert_eq!(report["endpoints"].as_array().unwrap().len(), 1);
        assert_eq!(
            report["endpoints"][0]["registrations"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            report["endpoints"][0]["missing_registered_models"],
            json!(["missing-model"])
        );
        assert_eq!(report["disabled_registrations_skipped"], 1);
        assert_eq!(report["generation_tested"], false);
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM tasks", [], |row| row.get::<_, u64>(0))
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn registered_discovery_bounds_distinct_endpoints_before_network() {
        let store = Store::in_memory().unwrap();
        assert_eq!(
            registered_endpoints(&store).await.unwrap()["status"],
            "not_applicable"
        );
        for index in 0..9 {
            registered_fixture(
                &store,
                &format!("domain.model{index}"),
                "fixture",
                &format!("http://127.0.0.1:{}/v1", 12000 + index),
            );
        }
        let error = registered_endpoints(&store).await.unwrap_err().to_string();
        assert!(error.contains("eight"));
    }

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
