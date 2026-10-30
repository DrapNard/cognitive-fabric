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

/// Operator-declared client tool route, pinned to its exact canonical schema.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientToolRoute {
    pub schema_sha256: String,
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub independent: bool,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(skip)]
    pub home: PathBuf,
    pub format_version: u16,
    pub backend: String,
    pub inference_process: bool,
    pub arithmetic_process: bool,
    pub sql_process: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_process_limits: Option<crate::process_limits::Limits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_token_admission: Option<crate::token_admission::Settings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_admission: Option<crate::resource_scheduler::Configuration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtask_scheduling: Option<crate::scheduling::Policy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtask_parallelism: Option<usize>,
    /// Explicit ceiling for concurrently admitted, capability-distinct
    /// workflow experts. Omission preserves sequential fallback behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_parallelism: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advice_guard: Option<crate::reasoning::advice_guard::Settings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_stopping: Option<crate::reasoning::progress::Settings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finite_search: Option<crate::reasoning::finite_search::Settings>,
    pub inference_auth_scope: Option<String>,
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
    /// Allow the client-facing `cogfabric-agent` endpoint to opt into the
    /// bounded Jev+ / Jev-R collective path. Disabled by default for backwards
    /// compatibility and conservative resource use.
    #[serde(skip_serializing_if = "is_false")]
    pub agent_collective: bool,
    /// Restrict client collective inference to interpretation and presentation.
    #[serde(skip_serializing_if = "is_false")]
    pub native_only_client: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub client_tool_routes: BTreeMap<String, ClientToolRoute>,
    /// Optional immutable operator-published project context applied to new
    /// client collective sessions (`project-id@version`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_project_profile: Option<String>,
    #[serde(skip_serializing_if = "default_interface_tokens")]
    pub interface_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
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
            inference_process: false,
            arithmetic_process: false,
            sql_process: false,
            worker_process_limits: None,
            input_token_admission: None,
            resource_admission: None,
            subtask_scheduling: None,
            subtask_parallelism: None,
            expert_parallelism: None,
            advice_guard: None,
            progress_stopping: None,
            finite_search: None,
            inference_auth_scope: None,
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
            agent_collective: false,
            native_only_client: false,
            client_tool_routes: BTreeMap::new(),
            agent_project_profile: None,
            interface_tokens: 128,
            reasoning_effort: None,
            synthesize_results: false,
            max_artifact_bytes: 256_000_000,
            max_database_bytes: crate::storage::DEFAULT_DATABASE_BYTES,
        }
    }
}

impl Config {
    /// Construct a complete replacement role map before publishing any settings.
    /// This configures model identities only; it never loads or trains weights.
    pub fn language_network(
        &self,
        language: String,
        reasoning: String,
        overrides: BTreeMap<String, String>,
        parallelism: usize,
        expert_parallelism: usize,
    ) -> Result<Self> {
        if !(1..=4).contains(&parallelism) || !(1..=4).contains(&expert_parallelism) {
            bail!("Network parallelism must be between 1 and 4")
        }
        if overrides.len() > 5
            || overrides.keys().any(|role| {
                !matches!(
                    role.as_str(),
                    "planner" | "solver" | "explorer" | "critic" | "reflector"
                )
            })
        {
            bail!("Unsupported reasoning role override; verification remains independent")
        }
        let mut configured = self.clone();
        configured.backend = "openai-local".into();
        configured.model_registration = None;
        configured.model = reasoning;
        configured.role_profiles.clear();
        configured.role_profiles.insert(
            "language".into(),
            RoleProfile {
                model: language,
                temperature: self.temperature,
            },
        );
        for (role, model) in overrides {
            configured.role_profiles.insert(
                role,
                RoleProfile {
                    model,
                    temperature: self.temperature,
                },
            );
        }
        configured.interpret_goals = true;
        configured.agent_collective = true;
        configured.native_only_client = false;
        configured.subtask_parallelism = Some(parallelism);
        configured.expert_parallelism = (expert_parallelism > 1).then_some(expert_parallelism);
        configured.validate()?;
        Ok(configured)
    }

    /// Explicit language-only routing marker, carried in typed role profiles.
    pub fn language_model(&self) -> Option<&str> {
        self.role_profiles
            .get("language")
            .map(|profile| profile.model.as_str())
    }

    pub fn require_role_model(&self, role: &str, model: &str) -> Result<()> {
        crate::fabric::protocol::actor(role)?;
        if let Some(language) = self.language_model() {
            let boundary = matches!(role, "interface" | "synthesizer");
            if boundary != (model == language) {
                bail!("Language-only model routing forbids this role/model assignment")
            }
        }
        Ok(())
    }

    pub fn model_settings(&self, role: &str) -> Result<(&str, f64)> {
        crate::fabric::protocol::actor(role)?;
        let profile = if matches!(role, "interface" | "synthesizer") {
            self.role_profiles
                .get("language")
                .or_else(|| self.role_profiles.get(role))
        } else {
            self.role_profiles.get(role)
        };
        let model = profile
            .map(|profile| profile.model.as_str())
            .unwrap_or(&self.model);
        self.require_role_model(role, model)?;
        Ok((
            model,
            profile
                .map(|profile| profile.temperature)
                .unwrap_or(self.temperature),
        ))
    }

    /// Language-only roles must not inherit reasoning effort from a task or
    /// provider default. Other roles preserve explicit request/config precedence.
    pub fn role_reasoning_effort<'a>(
        &'a self,
        role: &str,
        requested: Option<&'a str>,
    ) -> Option<&'a str> {
        if self.language_model().is_some() && matches!(role, "interface" | "synthesizer") {
            Some("none")
        } else {
            requested.or(self.reasoning_effort.as_deref())
        }
    }

    pub fn provider_effort<'a>(&self, effort: &'a str) -> Result<&'a str> {
        if !matches!(effort, "none" | "low" | "medium" | "high") {
            bail!("Unsupported provider reasoning effort")
        }
        Ok(effort)
    }

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
        if let Some(reference) = &self.agent_project_profile {
            crate::projects::Selection::parse(reference)?;
        }
        if self.input_token_admission.is_some() {
            crate::token_admission::validate_config(self)?;
        }
        if let Some(limits) = self.worker_process_limits {
            limits.validate()?;
        }
        if let Some(settings) = &self.advice_guard {
            settings.validate()?;
        }
        if let Some(settings) = &self.progress_stopping {
            settings.validate()?;
        }
        if let Some(settings) = &self.finite_search {
            settings.validate()?;
        }
        if let Some(configuration) = &self.resource_admission {
            configuration.validate()?;
        }
        if self
            .subtask_parallelism
            .is_some_and(|parallelism| !(1..=4).contains(&parallelism))
        {
            bail!("Subtask parallelism must be between 1 and 4")
        }
        if self
            .expert_parallelism
            .is_some_and(|parallelism| !(2..=4).contains(&parallelism))
        {
            bail!("Expert parallelism must be between 2 and 4")
        }
        if self
            .reasoning_effort
            .as_deref()
            .is_some_and(|value| !matches!(value, "none" | "low" | "medium" | "high"))
        {
            bail!("Unsupported provider reasoning effort")
        }
        if let Some(scope) = &self.inference_auth_scope {
            if scope.len() > 2048 {
                bail!("Inference credential scope exceeds bounds")
            }
            crate::backend::local_endpoint(scope)?;
        }
        if let Some(selection) = &self.model_registration {
            selection.validate()?;
        }
        if self.format_version != 1
            || !matches!(self.backend.as_str(), "demo" | "openai-local")
            || self.model.trim().is_empty()
            || self.model.len() > 256
            || !self.temperature.is_finite()
            || !(0.0..=2.0).contains(&self.temperature)
            || self.role_profiles.len() > 8
            || self.max_calls > 64
            || self.max_cycles > 16
            || self.max_tokens > 16_384
            || !(1..=4096).contains(&self.interface_tokens)
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
        if self.native_only_client && self.language_model().is_none() {
            bail!("Native-only client mode requires a configured language interface")
        }
        if self.client_tool_routes.len() > crate::agent::MAX_CLIENT_TOOLS {
            bail!("Client tool route count exceeds its bound")
        }
        let mut client_capabilities = std::collections::BTreeSet::new();
        for (name, route) in &self.client_tool_routes {
            if name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
                || route.schema_sha256.len() != 64
                || !route
                    .schema_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || route.capabilities.is_empty()
                || route.capabilities.len() > 8
            {
                bail!("Invalid client tool route identity, schema digest or capability count")
            }
            let mut unique = std::collections::BTreeSet::new();
            for capability in &route.capabilities {
                if !crate::registry::identifier(capability) || !unique.insert(capability) {
                    bail!("Invalid or duplicate client tool route capability")
                }
                client_capabilities.insert(capability);
            }
        }
        if client_capabilities.len() > 32 {
            bail!("Client tool route capability catalog exceeds its bound")
        }
        for (role, profile) in &self.role_profiles {
            if !matches!(
                role.as_str(),
                "language"
                    | "planner"
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
        if let Some(language) = self.language_model() {
            if language == self.model {
                bail!("Language-only routing requires a distinct default reasoning model")
            }
            for (role, profile) in &self.role_profiles {
                if role != "language" {
                    self.require_role_model(role, &profile.model)?;
                }
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

fn default_interface_tokens(value: &usize) -> bool {
    *value == 128
}

fn is_false(value: &bool) -> bool {
    !value
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_effort_is_opt_in_bounded_and_legacy_compatible() {
        let mut cfg = Config::default();
        let legacy = serde_json::to_value(&cfg).unwrap();
        assert!(legacy.get("reasoning_effort").is_none());
        assert!(serde_json::from_value::<Config>(legacy)
            .unwrap()
            .reasoning_effort
            .is_none());
        for effort in ["none", "low", "medium", "high"] {
            cfg.reasoning_effort = Some(effort.into());
            cfg.validate().unwrap();
            let restored: Config =
                serde_json::from_value(serde_json::to_value(&cfg).unwrap()).unwrap();
            assert_eq!(restored.reasoning_effort, cfg.reasoning_effort);
        }
        for effort in ["", "default", "arbitrary", "HIGH"] {
            cfg.reasoning_effort = Some(effort.into());
            assert!(cfg.validate().is_err());
        }
    }

    #[test]
    fn provider_effort_keeps_standard_wire_values_and_rejects_internal_toggle_names() {
        let cfg = Config::default();
        for effort in ["none", "low", "medium", "high"] {
            assert_eq!(cfg.provider_effort(effort).unwrap(), effort);
        }
        for unsupported in ["on", "off", "unknown"] {
            assert!(cfg.provider_effort(unsupported).is_err());
        }
    }

    #[test]
    fn client_collective_setting_defaults_off_and_roundtrips() {
        let mut cfg = Config::default();
        let legacy = serde_json::to_value(&cfg).unwrap();
        assert!(legacy.get("agent_collective").is_none());
        assert!(
            !serde_json::from_value::<Config>(legacy)
                .unwrap()
                .agent_collective
        );

        cfg.agent_collective = true;
        cfg.validate().unwrap();
        let encoded = serde_json::to_value(&cfg).unwrap();
        assert_eq!(encoded["agent_collective"], true);
        assert!(
            serde_json::from_value::<Config>(encoded)
                .unwrap()
                .agent_collective
        );
    }

    #[test]
    fn client_project_profile_is_optional_canonical_and_roundtrips() {
        let mut cfg = Config::default();
        let legacy = serde_json::to_value(&cfg).unwrap();
        assert!(legacy.get("agent_project_profile").is_none());
        assert_eq!(
            serde_json::from_value::<Config>(legacy)
                .unwrap()
                .agent_project_profile,
            None
        );
        cfg.agent_project_profile = Some("demo.project@1".into());
        cfg.validate().unwrap();
        let encoded = serde_json::to_value(&cfg).unwrap();
        assert_eq!(encoded["agent_project_profile"], "demo.project@1");
        assert_eq!(
            serde_json::from_value::<Config>(encoded)
                .unwrap()
                .agent_project_profile
                .as_deref(),
            Some("demo.project@1")
        );
        for invalid in ["demo.project", "demo.project@0", "demo.project@01"] {
            cfg.agent_project_profile = Some(invalid.into());
            assert!(cfg.validate().is_err());
        }
    }
    #[test]
    fn interface_reservation_defaults_preserve_legacy_settings_and_remain_bounded() {
        let mut cfg = Config::default();
        let legacy = serde_json::to_value(&cfg).unwrap();
        assert!(legacy.get("interface_tokens").is_none());
        assert_eq!(
            serde_json::from_value::<Config>(legacy)
                .unwrap()
                .interface_tokens,
            128
        );
        cfg.interface_tokens = 384;
        let raw = serde_json::to_value(&cfg).unwrap();
        assert_eq!(raw["interface_tokens"], 384);
        assert_eq!(
            serde_json::from_value::<Config>(raw)
                .unwrap()
                .interface_tokens,
            384
        );
        for invalid in [0, 4097, usize::MAX] {
            cfg.interface_tokens = invalid;
            assert!(cfg.validate().is_err());
        }
    }

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

#[cfg(all(test, target_os = "linux"))]
mod worker_limit_tests {
    use super::*;
    #[test]
    fn process_limits_are_optional_strict_and_preserved_in_configuration() {
        let mut cfg = Config::default();
        assert!(serde_json::to_value(&cfg)
            .unwrap()
            .get("worker_process_limits")
            .is_none());
        cfg.worker_process_limits = Some(crate::process_limits::Limits {
            address_space_bytes: 536870912,
            cpu_seconds: 10,
        });
        cfg.validate().unwrap();
        let decoded: Config = serde_json::from_value(serde_json::to_value(&cfg).unwrap()).unwrap();
        assert_eq!(decoded.worker_process_limits, cfg.worker_process_limits);
        cfg.worker_process_limits.as_mut().unwrap().cpu_seconds = 0;
        assert!(cfg.validate().is_err());
    }
}
