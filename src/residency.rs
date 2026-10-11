//! Operator-only LM Studio native residency observations; no model mutation.
use crate::{backend, credentials::Bearer, security};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

pub const CREDENTIAL_ENV: &str = "COGFABRIC_RESIDENCY_API_KEY";

#[derive(Debug, Deserialize, Serialize)]
pub struct Instance {
    pub id: String,
    pub config: InstanceConfig,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct InstanceConfig {
    pub context_length: u64,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Model {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub loaded_instances: Vec<Instance>,
}
#[derive(Deserialize)]
struct Inventory {
    models: Vec<Model>,
}

fn identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn parse(bytes: &[u8]) -> Result<Vec<Model>> {
    let inventory: Inventory = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid native model inventory"))?;
    if inventory.models.len() > 256 {
        bail!("Native model inventory exceeds bounds")
    }
    let mut keys = BTreeSet::new();
    let mut instances = BTreeSet::new();
    for model in &inventory.models {
        if !identity(&model.key)
            || !keys.insert(&model.key)
            || !matches!(model.kind.as_str(), "llm" | "embedding" | "decision")
            || model.loaded_instances.len() > 32
        {
            bail!("Invalid native model identity or bounds")
        }
        for instance in &model.loaded_instances {
            if !identity(&instance.id)
                || !instances.insert(&instance.id)
                || !(1..=16_777_216).contains(&instance.config.context_length)
            {
                bail!("Invalid native model instance or context bounds")
            }
        }
    }
    if instances.len() > 1024 {
        bail!("Native loaded instance capacity exceeded")
    }
    Ok(inventory.models)
}

fn endpoint(base: &str) -> Result<url::Url> {
    // Reuse literal-loopback validation but require an explicit native API path.
    let mut url = backend::local_endpoint(base)?;
    if url.path() != "/api/v1/chat/completions" {
        bail!("Residency requires an explicit /api/v1 native API base")
    }
    url.set_path("/api/v1/models");
    Ok(url)
}

pub async fn status(base: &str, authenticate: bool) -> Result<Value> {
    security::authorize("human-cli", "model.residency.inspect")?;
    let endpoint = endpoint(base)?;
    let credential = if authenticate {
        Some(Bearer::new(std::env::var(CREDENTIAL_ENV).map_err(
            |_| anyhow::anyhow!("Dedicated residency credential is unavailable"),
        )?)?)
    } else {
        None
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()?;
    let mut request = client.get(endpoint.clone());
    if let Some(credential) = &credential {
        request = credential.apply(request)?;
    }
    let start = Instant::now();
    let mut response = request.send().await?;
    if !response.status().is_success() {
        bail!("Native model inventory returned a non-success status")
    }
    if response.content_length().is_some_and(|n| n > 262144) {
        bail!("Native inventory response exceeds limit")
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 262144 {
            bail!("Native inventory response exceeds limit")
        }
        bytes.extend_from_slice(&chunk);
    }
    if let Some(credential) = &credential {
        credential.reject_echo(&bytes)?;
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Invalid native inventory JSON"))?;
    if let Some(credential) = &credential {
        credential.reject_echo(&serde_json::to_vec(&value)?)?;
    }
    let models = parse(&serde_json::to_vec(&value)?)?;
    let loaded = models
        .iter()
        .map(|m| m.loaded_instances.len())
        .sum::<usize>();
    Ok(
        json!({"format_version":1,"adapter":"lmstudio-native-v1","endpoint":endpoint.as_str(),
        "models":models,"loaded_instances":loaded,"observation_ms":start.elapsed().as_secs_f64()*1000.0,
        "server_reported":true,"weights_attested":false,"generation_tested":false,
        "ownership_established":false,"model_mutation_performed":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_inventory_distinguishes_downloaded_models_and_loaded_instances() {
        let fixture = json!({"models":[{"key":"small","type":"llm","loaded_instances":[{"id":"small-instance","config":{"context_length":2048,"parallel":1}}]},
            {"key":"embedding","type":"embedding","loaded_instances":[],"publisher":"fixture"}]});
        let models = parse(&serde_json::to_vec(&fixture).unwrap()).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].loaded_instances[0].id, "small-instance");
        assert!(models[1].loaded_instances.is_empty());
        for mutation in 0..5 {
            let mut invalid = fixture.clone();
            match mutation {
                0 => invalid["models"][1]["key"] = json!("small"),
                1 => {
                    invalid["models"][1]["loaded_instances"] =
                        invalid["models"][0]["loaded_instances"].clone()
                }
                2 => {
                    invalid["models"][0]["loaded_instances"][0]["config"]["context_length"] =
                        json!(0)
                }
                3 => invalid["models"][0]["type"] = json!("arbitrary-executable"),
                _ => invalid["models"][0]["key"] = json!("x".repeat(257)),
            }
            assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        assert!(parse(br#"{"models":[{"key":"small","type":"llm"}]}"#).is_err());
        assert!(security::authorize("jev-plus", "model.residency.inspect").is_err());
    }
    #[test]
    fn native_endpoint_never_inherits_inference_paths_or_remote_access() {
        assert_eq!(
            endpoint("http://localhost:1234/api/v1/").unwrap().as_str(),
            "http://127.0.0.1:1234/api/v1/models"
        );
        for base in [
            "http://127.0.0.1:1234/v1",
            "https://127.0.0.1/api/v1",
            "http://example.com/api/v1",
            "http://user@127.0.0.1/api/v1",
            "http://127.0.0.1/api/v1?token=x",
        ] {
            assert!(endpoint(base).is_err());
        }
    }
}
