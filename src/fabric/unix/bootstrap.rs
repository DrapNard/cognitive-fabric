//! Bounded typed settings for a single-task worker; no paths or executable grants.
use crate::config::{Config, RoleProfile};
use anyhow::{anyhow, bail, Result};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/bootstrap_generated.rs"));
}
use generated::worker as fb;
pub const MAX_BYTES: usize = 16_384;

pub struct Bootstrap {
    pub task: String,
    pub config: Config,
}
impl Bootstrap {
    pub async fn send(&self, stream: &mut UnixStream) -> Result<()> {
        let bytes = self.encode()?;
        tokio::time::timeout(Duration::from_secs(2), async {
            stream.write_u32(u32::try_from(bytes.len())?).await?;
            stream.write_all(&bytes).await?;
            Ok(())
        })
        .await
        .map_err(|_| anyhow!("Worker bootstrap write deadline exhausted"))?
    }
    pub async fn receive(stream: &mut UnixStream, host_uid: u32, host_pid: u32) -> Result<Self> {
        super::authenticate(stream, host_uid, host_pid)?;
        tokio::time::timeout(Duration::from_secs(10), async {
            let length = stream.read_u32().await? as usize;
            if !(8..=MAX_BYTES).contains(&length) {
                bail!("Invalid worker bootstrap length")
            }
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).await?;
            Self::decode(&bytes)
        })
        .await
        .map_err(|_| anyhow!("Worker bootstrap read deadline exhausted"))?
    }
    pub fn authority(&self, host_uid: u32, host_pid: u32) -> Result<super::worker::Authority> {
        self.validate()?;
        super::worker::Authority::new(
            crate::fabric::ModuleDescriptor {
                id: "inference.local".into(),
                version: "1.0.0".into(),
                roles: crate::fabric::protocol::ROLES
                    .iter()
                    .map(|role| (*role).into())
                    .collect(),
            },
            host_uid,
            host_pid,
            std::slice::from_ref(&self.task),
            super::worker::Limits {
                calls_per_task: u32::try_from(self.config.max_calls)?,
                output_tokens_per_task: self.config.max_tokens as u64,
                prompt_bytes_per_task: self.config.max_input_bytes as u64,
            },
        )?
        .with_session_seconds(self.config.max_wall_seconds)
    }
    fn validate(&self) -> Result<()> {
        uuid::Uuid::parse_str(&self.task)?;
        self.config.validate()?;
        if self.config.api_base.len() > 2048
            || self.config.max_calls == 0
            || self.config.max_tokens == 0
            || self.config.max_input_bytes == 0
        {
            bail!("Invalid worker bootstrap settings")
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let mut profiles = Vec::new();
        for (role, profile) in &self.config.role_profiles {
            let role = builder.create_string(role);
            let model = builder.create_string(&profile.model);
            profiles.push(fb::RoleProfile::create(
                &mut builder,
                &fb::RoleProfileArgs {
                    role: Some(role),
                    model: Some(model),
                    temperature: profile.temperature,
                },
            ));
        }
        let input_token_admission = self.config.input_token_admission.as_ref().map(|settings| {
            fb::InputTokenAdmission::create(
                &mut builder,
                &fb::InputTokenAdmissionArgs {
                    format_version: settings.format_version,
                    max_input_tokens: settings.max_input_tokens,
                    max_context_tokens: settings.max_context_tokens,
                    authenticate: settings.authenticate,
                },
            )
        });
        let args = fb::BootstrapArgs {
            version: if self.config.input_token_admission.is_some() {
                3
            } else if self.config.reasoning_effort.is_some() {
                2
            } else {
                1
            },
            task_id: Some(builder.create_string(&self.task)),
            backend: Some(builder.create_string(&self.config.backend)),
            model: Some(builder.create_string(&self.config.model)),
            api_base: Some(builder.create_string(&self.config.api_base)),
            temperature: self.config.temperature,
            timeout_seconds: self.config.timeout_seconds,
            max_calls: u32::try_from(self.config.max_calls)?,
            max_tokens: u32::try_from(self.config.max_tokens)?,
            max_input_bytes: self.config.max_input_bytes as u64,
            role_profiles: Some(builder.create_vector(&profiles)),
            max_database_bytes: self.config.max_database_bytes,
            max_tool_calls: self.config.max_tool_calls,
            max_wall_seconds: self.config.max_wall_seconds,
            inference_auth_scope: self
                .config
                .inference_auth_scope
                .as_ref()
                .map(|scope| builder.create_string(scope)),
            reasoning_effort: self
                .config
                .reasoning_effort
                .as_ref()
                .map(|value| builder.create_string(value)),
            input_token_admission,
        };
        let root = fb::Bootstrap::create(&mut builder, &args);
        builder.finish(root, Some("COGB"));
        let bytes = builder.finished_data();
        if bytes.len() > MAX_BYTES {
            bail!("Worker bootstrap exceeds bounds")
        }
        Ok(bytes.to_vec())
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if !(8..=MAX_BYTES).contains(&bytes.len())
            || !flatbuffers::buffer_has_identifier(bytes, "COGB", false)
        {
            bail!("Invalid worker bootstrap frame")
        }
        let packet = flatbuffers::root::<fb::Bootstrap>(bytes)?;
        let input_token_admission =
            packet
                .input_token_admission()
                .map(|settings| crate::token_admission::Settings {
                    format_version: settings.format_version(),
                    max_input_tokens: settings.max_input_tokens(),
                    max_context_tokens: settings.max_context_tokens(),
                    authenticate: settings.authenticate(),
                });
        if !matches!(packet.version(), 1..=3)
            || (packet.version() == 1 && packet.reasoning_effort().is_some())
            || (packet.version() == 2 && packet.reasoning_effort().is_none())
            || (packet.version() < 3 && input_token_admission.is_some())
            || (packet.version() == 3 && input_token_admission.is_none())
            || packet
                .reasoning_effort()
                .is_some_and(|value| value.len() > 6)
            || packet.task_id().len() > 64
            || packet.backend().len() > 32
            || packet.model().len() > 256
            || packet.api_base().len() > 2048
            || packet
                .inference_auth_scope()
                .is_some_and(|scope| scope.len() > 2048)
            || packet.role_profiles().is_some_and(|p| p.len() > 7)
        {
            bail!("Worker bootstrap identity or bounds invalid")
        }
        let mut config = Config {
            backend: packet.backend().into(),
            model: packet.model().into(),
            api_base: packet.api_base().into(),
            temperature: packet.temperature(),
            timeout_seconds: packet.timeout_seconds(),
            max_calls: packet.max_calls() as usize,
            max_tokens: packet.max_tokens() as usize,
            max_input_bytes: usize::try_from(packet.max_input_bytes())?,
            max_database_bytes: packet.max_database_bytes(),
            max_wall_seconds: packet.max_wall_seconds(),
            inference_auth_scope: packet.inference_auth_scope().map(str::to_string),
            reasoning_effort: packet.reasoning_effort().map(str::to_string),
            input_token_admission,
            interpret_goals: false,
            synthesize_results: false,
            max_tool_calls: packet.max_tool_calls(),
            ..Config::default()
        };
        if let Some(profiles) = packet.role_profiles() {
            for profile in profiles {
                if profile.role().len() > 32 || profile.model().len() > 256 {
                    bail!("Worker role profile exceeds bounds")
                }
                if config
                    .role_profiles
                    .insert(
                        profile.role().into(),
                        RoleProfile {
                            model: profile.model().into(),
                            temperature: profile.temperature(),
                        },
                    )
                    .is_some()
                {
                    bail!("Duplicate worker role profile")
                }
            }
        }
        let bootstrap = Self {
            task: packet.task_id().into(),
            config,
        };
        bootstrap.validate()?;
        Ok(bootstrap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn authenticated_delivery_preserves_settings_and_rejects_bad_lengths() {
        let (mut host, mut worker) = UnixStream::pair().unwrap();
        let uid = worker.peer_cred().unwrap().uid();
        assert!(Bootstrap::receive(&mut worker, uid, 0).await.is_err());
        let setup = Bootstrap {
            task: uuid::Uuid::new_v4().to_string(),
            config: Config::default(),
        };
        let (sent, received) = tokio::join!(
            setup.send(&mut host),
            Bootstrap::receive(&mut worker, uid, std::process::id())
        );
        sent.unwrap();
        assert_eq!(received.unwrap().task, setup.task);
        host.write_u32(MAX_BYTES as u32 + 1).await.unwrap();
        assert!(Bootstrap::receive(&mut worker, uid, std::process::id())
            .await
            .is_err());
    }
    #[test]
    fn forged_versions_duplicate_roles_and_nonlocal_settings_fail_decode() {
        for invalid in 0..7 {
            let mut builder = flatbuffers::FlatBufferBuilder::new();
            let role = builder.create_string("solver");
            let model = builder.create_string("fixture");
            let profile = fb::RoleProfile::create(
                &mut builder,
                &fb::RoleProfileArgs {
                    role: Some(role),
                    model: Some(model),
                    temperature: 0.2,
                },
            );
            let profiles = if invalid == 1 {
                builder.create_vector(&[profile, profile])
            } else {
                builder.create_vector(&[profile])
            };
            let args = fb::BootstrapArgs {
                version: if matches!(invalid, 0 | 5 | 6) { 2 } else { 1 },
                task_id: Some(builder.create_string(&uuid::Uuid::new_v4().to_string())),
                backend: Some(builder.create_string("openai-local")),
                model: Some(model),
                api_base: Some(builder.create_string(if invalid == 2 {
                    "http://192.0.2.1/v1"
                } else {
                    "http://127.0.0.1:1234/v1"
                })),
                temperature: 0.2,
                timeout_seconds: 10,
                max_calls: 4,
                max_tool_calls: 4,
                max_tokens: 512,
                max_input_bytes: 4096,
                role_profiles: Some(profiles),
                max_database_bytes: 256_000_000,
                max_wall_seconds: if invalid == 3 { 3601 } else { 300 },
                inference_auth_scope: None,
                input_token_admission: None,
                reasoning_effort: match invalid {
                    4 => Some(builder.create_string("none")),
                    5 => Some(builder.create_string("bogus")),
                    6 => Some(builder.create_string("oversized")),
                    _ => None,
                },
            };
            let root = fb::Bootstrap::create(&mut builder, &args);
            builder.finish(root, Some("COGB"));
            assert!(Bootstrap::decode(builder.finished_data()).is_err());
        }
    }
    #[test]
    fn roundtrip_preserves_generation_settings_without_paths_or_extra_authority() {
        let mut config = Config {
            home: "/private/workspace".into(),
            inference_process: true,
            max_database_bytes: 64_000_000,
            max_wall_seconds: 3600,
            model: "pinned-model".into(),
            reasoning_effort: Some("none".into()),
            inference_auth_scope: Some("http://127.0.0.1:1234/v1".into()),
            max_calls: 4,
            max_tokens: 512,
            max_input_bytes: 4096,
            ..Config::default()
        };
        config.role_profiles.insert(
            "solver".into(),
            RoleProfile {
                model: "pinned-solver".into(),
                temperature: 0.42,
            },
        );
        let original = Bootstrap {
            task: uuid::Uuid::new_v4().to_string(),
            config,
        };
        let bytes = original.encode().unwrap();
        let decoded = Bootstrap::decode(&bytes).unwrap();
        assert_eq!(decoded.task, original.task);
        assert_eq!(decoded.config.model, "pinned-model");
        assert_eq!(decoded.config.reasoning_effort.as_deref(), Some("none"));
        assert_eq!(
            decoded.config.inference_auth_scope,
            original.config.inference_auth_scope
        );
        assert_eq!(
            decoded.config.role_profiles["solver"].model,
            "pinned-solver"
        );
        assert_eq!(decoded.config.role_profiles["solver"].temperature, 0.42);
        assert_eq!(
            (
                decoded.config.max_calls,
                decoded.config.max_tokens,
                decoded.config.max_input_bytes
            ),
            (4, 512, 4096)
        );
        assert!(decoded.config.home.as_os_str().is_empty());
        assert!(decoded.config.model_registration.is_none());
        assert!(!decoded.config.interpret_goals);
        assert!(!decoded.config.inference_process);
        assert_eq!(decoded.config.max_database_bytes, 64_000_000);
        assert_eq!(decoded.config.max_wall_seconds, 3600);
        let legacy = Bootstrap {
            task: original.task.clone(),
            config: Config {
                max_wall_seconds: 300,
                ..Config::default()
            },
        }
        .encode()
        .unwrap();
        let table = flatbuffers::root::<fb::Bootstrap>(&legacy).unwrap();
        assert_eq!(
            table
                ._tab
                .vtable()
                .get(fb::Bootstrap::VT_INFERENCE_AUTH_SCOPE),
            0
        );
        assert!(Bootstrap::decode(&legacy)
            .unwrap()
            .config
            .inference_auth_scope
            .is_none());
        assert_eq!(
            table._tab.vtable().get(fb::Bootstrap::VT_MAX_WALL_SECONDS),
            0
        );
        assert_eq!(
            Bootstrap::decode(&legacy).unwrap().config.max_wall_seconds,
            300
        );
        assert!(!bytes.windows(18).any(|w| w == b"/private/workspace"));
    }
    #[test]
    fn input_token_admission_roundtrips_only_in_v3_and_rejects_version_mismatches() {
        let setup = Bootstrap {
            task: uuid::Uuid::new_v4().to_string(),
            config: Config {
                backend: "openai-local".into(),
                reasoning_effort: Some("none".into()),
                input_token_admission: Some(crate::token_admission::Settings {
                    format_version: 1,
                    max_input_tokens: 32_768,
                    max_context_tokens: 65_536,
                    authenticate: true,
                }),
                ..Config::default()
            },
        };
        let bytes = setup.encode().unwrap();
        let decoded = Bootstrap::decode(&bytes).unwrap();
        assert_eq!(decoded.config.reasoning_effort.as_deref(), Some("none"));
        assert_eq!(
            decoded
                .config
                .input_token_admission
                .as_ref()
                .unwrap()
                .max_input_tokens,
            32_768
        );
        assert_eq!(
            decoded
                .config
                .input_token_admission
                .as_ref()
                .unwrap()
                .max_context_tokens,
            65_536
        );
        assert!(
            decoded
                .config
                .input_token_admission
                .as_ref()
                .unwrap()
                .authenticate
        );
        assert_eq!(
            flatbuffers::root::<fb::Bootstrap>(&bytes)
                .unwrap()
                .version(),
            3
        );

        for (version, include_settings) in [(1, true), (2, true), (3, false)] {
            let mut builder = flatbuffers::FlatBufferBuilder::new();
            let admission = include_settings.then(|| {
                fb::InputTokenAdmission::create(
                    &mut builder,
                    &fb::InputTokenAdmissionArgs {
                        format_version: 1,
                        max_input_tokens: 32_768,
                        max_context_tokens: 65_536,
                        authenticate: true,
                    },
                )
            });
            let args = fb::BootstrapArgs {
                version,
                task_id: Some(builder.create_string(&uuid::Uuid::new_v4().to_string())),
                backend: Some(builder.create_string("openai-local")),
                model: Some(builder.create_string("fixture")),
                api_base: Some(builder.create_string("http://127.0.0.1:1234/v1")),
                temperature: 0.2,
                timeout_seconds: 10,
                max_calls: 4,
                max_tool_calls: 4,
                max_tokens: 512,
                max_input_bytes: 4096,
                role_profiles: None,
                max_database_bytes: 256_000_000,
                max_wall_seconds: 300,
                inference_auth_scope: None,
                reasoning_effort: if version == 2 {
                    Some(builder.create_string("high"))
                } else {
                    None
                },
                input_token_admission: admission,
            };
            let root = fb::Bootstrap::create(&mut builder, &args);
            builder.finish(root, Some("COGB"));
            assert!(Bootstrap::decode(builder.finished_data()).is_err());
        }
    }
    #[test]
    fn malformed_bootstrap_and_invalid_settings_are_rejected() {
        let original = Bootstrap {
            task: uuid::Uuid::new_v4().to_string(),
            config: Config::default(),
        };
        let bytes = original.encode().unwrap();
        for length in 0..bytes.len() {
            if let Ok(decoded) = Bootstrap::decode(&bytes[..length]) {
                assert_eq!(decoded.task, original.task);
            }
        }
        let mut corrupt = bytes;
        corrupt[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Bootstrap::decode(&corrupt).is_err());
        assert!(Bootstrap::decode(&vec![0; MAX_BYTES + 1]).is_err());
        let mut invalid = original;
        invalid.config.api_base = "http://example.com/v1".into();
        assert!(invalid.encode().is_err());
        invalid.config.api_base = Config::default().api_base;
        invalid.config.max_calls = 0;
        assert!(invalid.encode().is_err());
    }
}

#[cfg(test)]
mod native_limits_tests {
    use super::*;
    #[test]
    fn appended_proposal_ceiling_roundtrips_and_old_default_is_absent() {
        for tools in [0, 1, 4, 64] {
            let setup = Bootstrap {
                task: uuid::Uuid::new_v4().to_string(),
                config: Config {
                    max_tool_calls: tools,
                    ..Config::default()
                },
            };
            let bytes = setup.encode().unwrap();
            let decoded = Bootstrap::decode(&bytes).unwrap();
            assert_eq!(decoded.config.max_tool_calls, tools);
            if tools == 4 {
                let root = flatbuffers::root::<fb::Bootstrap>(&bytes).unwrap();
                assert_eq!(root._tab.vtable().get(fb::Bootstrap::VT_MAX_TOOL_CALLS), 0);
            }
        }
    }
}
