use anyhow::{bail, Result};
use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub home: PathBuf,
    pub backend: String,
    pub model: String,
    pub api_base: String,
    pub max_calls: usize,
    pub max_cycles: usize,
    pub max_tokens: usize,
    pub timeout_seconds: u64,
    pub max_wall_seconds: u64,
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
        let backend = backend
            .or_else(|| env::var("COGFABRIC_BACKEND").ok())
            .unwrap_or_else(|| "demo".into());
        if backend != "demo" && backend != "openai-local" {
            bail!("Backend must be 'demo' or 'openai-local'")
        }
        Ok(Self {
            home,
            backend,
            model: env::var("COGFABRIC_MODEL").unwrap_or_else(|_| "local-model".into()),
            api_base: env::var("COGFABRIC_API_BASE")
                .unwrap_or_else(|_| "http://127.0.0.1:1234/v1".into()),
            max_calls: 6,
            max_cycles: 4,
            max_tokens: 1600,
            timeout_seconds: 45,
            max_wall_seconds: 120,
        })
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
