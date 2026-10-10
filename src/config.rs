use anyhow::{bail, Result};
use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleProfile {
    pub model: String,
    pub temperature: f64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(skip)]
    pub home: PathBuf,
    pub format_version: u16,
    pub backend: String,
    pub model: String,
    pub model_registration: Option<crate::models::Selection>,
    pub api_base: String,
    pub max_calls: usize,
    pub max_cycles: usize,
    pub max_tokens: usize,
    pub max_input_bytes: usize,
    pub max_tool_calls: u32,
    pub timeout_seconds: u64,
    pub max_wall_seconds: u64,
    pub temperature: f64,
    pub role_profiles: BTreeMap<String, RoleProfile>,
    pub interpret_goals: bool,
    pub synthesize_results: bool,
    pub max_artifact_bytes: u64,
    pub max_database_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            home: PathBuf::new(),
            format_version: 1,
            backend: "demo".into(),
            model: "local-model".into(),
            model_registration: None,
            api_base: "http://127.0.0.1:1234/v1".into(),
            max_calls: 8,
            max_cycles: 4,
            max_tokens: 1600,
            max_input_bytes: 128_000,
            max_tool_calls: 4,
            timeout_seconds: 45,
            max_wall_seconds: 120,
            temperature: 0.15,
            role_profiles: BTreeMap::new(),
            interpret_goals: true,
            synthesize_results: false,
            max_artifact_bytes: 256_000_000,
            max_database_bytes: crate::storage::DEFAULT_DATABASE_BYTES,
        }
    }
}

impl Config {
    pub fn load(home: Option<PathBuf>, backend: Option<String>) -> Result<Self> {
        let default_home = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local/share/cogfabric-rust");
        let home = home
            .or_else(|| env::var_os("COGFABRIC_RUST_HOME").map(PathBuf::from))
            .unwrap_or(default_home);
        let file = home.join("config.json");
        if home.is_symlink() {
            bail!("State directory cannot be a symlink")
        }
        let mut cfg = match fs::symlink_metadata(&file) {
            Ok(meta) => {
                if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 64_000 {
                    bail!("Configuration must be a bounded regular file")
                }
                serde_json::from_slice::<Config>(&crate::security::read_regular_bytes(
                    &file, 64_000,
                )?)?
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => return Err(err.into()),
        };
        cfg.home = home;
        if let Some(value) = backend.or_else(|| env::var("COGFABRIC_BACKEND").ok()) {
            cfg.backend = value;
        }
        if let Ok(value) = env::var("COGFABRIC_MODEL") {
            cfg.model = value;
        }
        if let Ok(value) = env::var("COGFABRIC_API_BASE") {
            cfg.api_base = value;
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(selection) = &self.model_registration {
            selection.validate()?;
        }
        if self.format_version != 1
            || !matches!(self.backend.as_str(), "demo" | "openai-local")
            || self.model.trim().is_empty()
            || self.model.len() > 256
            || !self.temperature.is_finite()
            || !(0.0..=2.0).contains(&self.temperature)
            || self.role_profiles.len() > 7
            || self.max_calls > 64
            || self.max_cycles > 16
            || self.max_tokens > 16_384
            || self.max_input_bytes > 1_000_000
            || self.max_tool_calls > 64
            || self.max_artifact_bytes > 2_000_000_000
            || !(64_000_000..=2_000_000_000).contains(&self.max_database_bytes)
            || !(1..=300).contains(&self.timeout_seconds)
            || !(1..=3600).contains(&self.max_wall_seconds)
        {
            bail!("Invalid inference configuration or resource ceiling")
        }
        crate::backend::local_endpoint(&self.api_base)?;
        for (role, profile) in &self.role_profiles {
            if !matches!(
                role.as_str(),
                "planner"
                    | "solver"
                    | "critic"
                    | "explorer"
                    | "reflector"
                    | "interface"
                    | "synthesizer"
            ) || profile.model.trim().is_empty()
                || profile.model.len() > 256
                || !profile.temperature.is_finite()
                || !(0.0..=2.0).contains(&profile.temperature)
            {
                bail!("Invalid role profile")
            }
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        self.prepare()?;
        let temporary = self
            .home
            .join(format!("config-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
            fs::rename(&temporary, self.home.join("config.json"))?;
            fs::File::open(&self.home)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
    pub fn prepare(&self) -> Result<()> {
        if self.home.exists() && self.home.is_symlink() {
            bail!("State directory cannot be a symlink")
        }
        fs::create_dir_all(&self.home)?;
        fs::set_permissions(&self.home, fs::Permissions::from_mode(0o700))?;
        Ok(())
    }
    pub fn db(&self) -> PathBuf {
        self.home.join("fabric.sqlite3")
    }
    pub fn socket(&self) -> PathBuf {
        self.home.join("fabric.sock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_profiles_roundtrip_with_owner_only_permissions() {
        let root = std::env::temp_dir().join(format!("fabric-config-{}", uuid::Uuid::new_v4()));
        let mut cfg = Config::load(Some(root.clone()), Some("demo".into())).unwrap();
        cfg.model = "shared-fixture/v1".into();
        cfg.role_profiles.insert(
            "critic".into(),
            RoleProfile {
                model: "shared-fixture/v1".into(),
                temperature: 0.0,
            },
        );
        cfg.max_tokens = 1024;
        cfg.save().unwrap();
        let loaded = Config::load(Some(root.clone()), Some("demo".into())).unwrap();
        assert_eq!(loaded.max_tokens, 1024);
        assert_eq!(loaded.role_profiles["critic"].model, "shared-fixture/v1");
        assert_eq!(
            std::fs::metadata(root.join("config.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_settings_and_symlinked_files_fail_closed() {
        let root = std::env::temp_dir().join(format!("fabric-config-{}", uuid::Uuid::new_v4()));
        let mut cfg = Config::load(Some(root.clone()), Some("demo".into())).unwrap();
        cfg.api_base = "http://example.com/v1".into();
        assert!(cfg.save().is_err());
        cfg.api_base = "http://127.0.0.1:1234/v1".into();
        cfg.max_tokens = usize::MAX;
        assert!(cfg.save().is_err());
        cfg.max_tokens = 1024;
        cfg.role_profiles.insert(
            "shell".into(),
            RoleProfile {
                model: "fixture".into(),
                temperature: 0.0,
            },
        );
        assert!(cfg.save().is_err());
        cfg.role_profiles.clear();
        cfg.save().unwrap();
        std::fs::remove_file(root.join("config.json")).unwrap();
        std::os::unix::fs::symlink("/dev/null", root.join("config.json")).unwrap();
        assert!(Config::load(Some(root.clone()), None).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
