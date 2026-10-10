//! Immutable local inference manifests, never executable plugins or weight attestation.
use crate::{
    backend,
    config::Config,
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE model_manifests (
 id TEXT NOT NULL, version TEXT NOT NULL, manifest TEXT NOT NULL,
 checksum TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1,
 PRIMARY KEY(id,version)
);
CREATE TABLE model_audit (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL,
 version TEXT NOT NULL, action TEXT NOT NULL, created REAL NOT NULL
);
PRAGMA user_version=5;
"#;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub id: String,
    pub version: String,
}
impl Selection {
    pub fn validate(&self) -> Result<()> {
        if !(3..=64).contains(&self.id.len())
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
            || self.version.len() > 64
            || semver::Version::parse(&self.version)?.to_string() != self.version
        {
            bail!("Invalid model registration identity")
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub model: String,
    pub api_base: String,
    pub source: String,
    pub license: String,
    pub context_tokens: u32,
    pub max_output_tokens: u32,
    /// Operator declaration only; the HTTP server does not attest this hash.
    pub weights_sha256: Option<String>,
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        Selection {
            id: self.id.clone(),
            version: self.version.clone(),
        }
        .validate()?;
        security::validate_provenance(&self.source, &self.license)?;
        backend::local_endpoint(&self.api_base)?;
        if self.model.trim().is_empty()
            || self.model.len() > 256
            || self.api_base.len() > 1000
            || !(256..=2_000_000).contains(&self.context_tokens)
            || !(1..=16384).contains(&self.max_output_tokens)
            || self.max_output_tokens > self.context_tokens
            || self.weights_sha256.as_ref().is_some_and(|hash| {
                hash.len() != 64
                    || !hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            bail!("Invalid model limits or declared weight identity")
        }
        Ok(())
    }
}
#[derive(Serialize)]
pub struct Registered {
    pub manifest: Manifest,
    pub checksum: String,
    pub enabled: bool,
}
impl Store {
    pub fn register_model(&self, path: &Path) -> Result<Registered> {
        security::authorize("human-cli", "model.register")?;
        let bytes = security::read_regular_bytes(path, 16000)?;
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        manifest.validate()?;
        let raw = serde_json::to_string(&manifest)?;
        let checksum = sha256(raw.as_bytes());
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM model_manifests WHERE id=?1 AND version=?2)",
            params![manifest.id, manifest.version],
            |row| row.get(0),
        )?;
        let count: i64 =
            tx.query_row("SELECT count(*) FROM model_manifests", [], |row| row.get(0))?;
        if exists || count >= 256 {
            bail!("Model version already exists or registry is full")
        }
        tx.execute(
            "INSERT INTO model_manifests VALUES (?1,?2,?3,?4,1)",
            params![manifest.id, manifest.version, raw, checksum],
        )?;
        tx.execute("INSERT INTO model_audit(id,version,action,created) VALUES (?1,?2,'register',unixepoch())",params![manifest.id,manifest.version])?;
        tx.commit()?;
        Ok(Registered {
            manifest,
            checksum,
            enabled: true,
        })
    }
    pub fn models(&self) -> Result<Vec<Registered>> {
        let db = self.lock()?;
        let rows = db.prepare("SELECT manifest,checksum,enabled FROM model_manifests ORDER BY id,version LIMIT 257")?
            .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,bool>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() > 256 {
            bail!("Model registry exceeds limit")
        }
        rows.into_iter()
            .map(|(raw, checksum, enabled)| {
                if raw.len() > 16000 || sha256(raw.as_bytes()) != checksum {
                    bail!("Model manifest integrity check failed")
                }
                let manifest: Manifest = serde_json::from_str(&raw)?;
                manifest.validate()?;
                Ok(Registered {
                    manifest,
                    checksum,
                    enabled,
                })
            })
            .collect()
    }
    pub fn model(&self, selection: &Selection) -> Result<Registered> {
        selection.validate()?;
        self.models()?
            .into_iter()
            .find(|entry| {
                entry.manifest.id == selection.id && entry.manifest.version == selection.version
            })
            .ok_or_else(|| anyhow::anyhow!("Unknown model registration"))
    }
    pub fn set_model_enabled(&self, selection: &Selection, enabled: bool) -> Result<()> {
        security::authorize("human-cli", "model.lifecycle")?;
        selection.validate()?;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<bool> = tx
            .query_row(
                "SELECT enabled FROM model_manifests WHERE id=?1 AND version=?2",
                params![selection.id, selection.version],
                |row| row.get(0),
            )
            .optional()?;
        if current.is_none() {
            bail!("Unknown model registration")
        }
        tx.execute(
            "UPDATE model_manifests SET enabled=?3 WHERE id=?1 AND version=?2",
            params![selection.id, selection.version, enabled],
        )?;
        tx.execute(
            "INSERT INTO model_audit(id,version,action,created) VALUES (?1,?2,?3,unixepoch())",
            params![
                selection.id,
                selection.version,
                if enabled { "enable" } else { "disable" }
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn model_configuration(&self, cfg: &Config, selection: &Selection) -> Result<Config> {
        security::authorize("human-cli", "model.select")?;
        let entry = self.model(selection)?;
        if !entry.enabled {
            bail!("Selected model registration is disabled")
        }
        let mut configured = cfg.clone();
        configured.backend = "openai-local".into();
        configured.model = entry.manifest.model;
        configured.api_base = entry.manifest.api_base;
        configured.max_tokens = configured
            .max_tokens
            .min(entry.manifest.max_output_tokens as usize);
        configured.role_profiles.clear();
        configured.model_registration = Some(selection.clone());
        configured.validate()?;
        Ok(configured)
    }
    pub(crate) fn pin_model(&self, cfg: &Config) -> Result<Option<Registered>> {
        let Some(selection) = &cfg.model_registration else {
            return Ok(None);
        };
        let entry = self.model(selection)?;
        if !entry.enabled
            || cfg.backend != "openai-local"
            || cfg.model != entry.manifest.model
            || cfg.api_base != entry.manifest.api_base
            || !cfg.role_profiles.is_empty()
            || cfg.max_tokens > entry.manifest.max_output_tokens as usize
        {
            bail!("Configured inference differs from enabled registered model; select the model again")
        }
        Ok(Some(entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (std::path::PathBuf, Manifest) {
        let path =
            std::env::temp_dir().join(format!("cogfabric-model-{}.json", uuid::Uuid::new_v4()));
        let manifest = Manifest {
            id: "local.small".into(),
            version: "1.0.0".into(),
            model: "small-model".into(),
            api_base: "http://127.0.0.1:1234/v1".into(),
            source: "operator-local".into(),
            license: "Apache-2.0".into(),
            context_tokens: 4096,
            max_output_tokens: 1024,
            weights_sha256: None,
        };
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        (path, manifest)
    }
    #[test]
    fn immutable_registration_selection_and_disable_preserve_snapshots() {
        let store = Store::in_memory().unwrap();
        let (path, manifest) = fixture();
        store.register_model(&path).unwrap();
        assert!(store.register_model(&path).is_err());
        let selection = Selection {
            id: manifest.id,
            version: manifest.version,
        };
        let cfg = store
            .model_configuration(&Config::default(), &selection)
            .unwrap();
        assert_eq!(cfg.max_tokens, 1024);
        let snapshot = store.pin_model(&cfg).unwrap().unwrap();
        store.set_model_enabled(&selection, false).unwrap();
        assert!(store.pin_model(&cfg).is_err());
        assert!(store.model_configuration(&cfg, &selection).is_err());
        assert!(snapshot.enabled);
        store.set_model_enabled(&selection, true).unwrap();
        assert!(store.pin_model(&cfg).unwrap().is_some());
        let mut altered = cfg.clone();
        altered.model = "another-model".into();
        assert!(store.pin_model(&altered).is_err());
        store
            .lock()
            .unwrap()
            .execute("UPDATE model_manifests SET manifest='{}'", [])
            .unwrap();
        assert!(store.models().is_err());
        assert!(security::authorize("jev-plus", "model.lifecycle").is_err());
        assert!(security::authorize("jev-r", "model.register").is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn manifests_reject_remote_endpoints_untrusted_fields_and_invalid_limits() {
        let store = Store::in_memory().unwrap();
        let (path, mut manifest) = fixture();
        manifest.api_base = "http://example.com/v1".into();
        assert!(manifest.validate().is_err());
        manifest.api_base = "http://127.0.0.1:1234/v1".into();
        manifest.max_output_tokens = 5000;
        assert!(manifest.validate().is_err());
        manifest.max_output_tokens = 1024;
        manifest.weights_sha256 = Some("not-a-hash".into());
        assert!(manifest.validate().is_err());
        let mut raw = serde_json::to_value(&manifest).unwrap();
        raw["shell_command"] = serde_json::json!("run this");
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(store.register_model(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn selected_registration_drives_the_actual_http_request() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0u8; 1024];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                assert!(request.len() < 16000);
                if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&request[..end]).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        let body: serde_json::Value =
                            serde_json::from_slice(&request[end + 4..end + 4 + length]).unwrap();
                        assert_eq!(body["model"], "small-model");
                        break;
                    }
                }
            }
            let body=serde_json::json!({"model":"small-model","choices":[{"message":{"content":"selected backend answer"}}]}).to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let (path, mut manifest) = fixture();
        manifest.api_base = format!("http://{address}/v1");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let store = std::sync::Arc::new(Store::in_memory().unwrap());
        crate::store::bootstrap(&store).unwrap();
        store.register_model(&path).unwrap();
        let base = Config {
            interpret_goals: false,
            ..Default::default()
        };
        let cfg = store
            .model_configuration(
                &base,
                &Selection {
                    id: manifest.id,
                    version: manifest.version,
                },
            )
            .unwrap();
        let engine = crate::engine::Engine::new(cfg, store);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            engine.run("Explain configuration", "fast"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.answer, "selected backend answer");
        assert_eq!(result.status, "unverified");
        server.await.unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn registered_configuration_is_recorded_without_claiming_weight_attestation() {
        let root = std::env::temp_dir().join(format!(
            "cogfabric-model-migration-{}",
            uuid::Uuid::new_v4()
        ));
        let path = root.join("state.db");
        let store = Store::open(&path).unwrap();
        let existing = store.create_task("preserved", "fast").unwrap();
        store
            .finish_task(&existing, "unverified", "existing result")
            .unwrap();
        store
            .lock()
            .unwrap()
            .execute_batch(
                "DROP TABLE model_audit; DROP TABLE model_manifests; DROP TABLE retention_audit; DROP INDEX event_conversation_task; PRAGMA user_version=4;",
            )
            .unwrap();
        drop(store);
        let store = std::sync::Arc::new(Store::open(&path).unwrap());
        crate::store::bootstrap(&store).unwrap();
        assert_eq!(
            store.task(&existing).unwrap()["task"]["answer"],
            "existing result"
        );
        let (file, manifest) = fixture();
        store.register_model(&file).unwrap();
        let cfg = store
            .model_configuration(
                &Config::default(),
                &Selection {
                    id: manifest.id,
                    version: manifest.version,
                },
            )
            .unwrap();
        let engine = crate::engine::Engine::new(cfg, store.clone());
        let result = engine.run("calc: 4 * 9", "fast").await.unwrap();
        assert_eq!(result.answer, "36");
        assert_eq!(result.calls, 0);
        let trace = store.task(&result.task_id).unwrap();
        let event = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "model_registration")
            .unwrap();
        assert_eq!(
            event["payload"]["registration"]["manifest"]["model"],
            "small-model"
        );
        assert_eq!(event["payload"]["weight_identity_verified"], false);
        drop(engine);
        drop(store);
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
