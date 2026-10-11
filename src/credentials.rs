//! Explicit operator-only environment credentials, scoped to one local API base.
use crate::{backend::local_endpoint, config::Config};
use anyhow::{bail, Result};
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    RequestBuilder,
};
pub const ENVIRONMENT_VARIABLE: &str = "COGFABRIC_INFERENCE_API_KEY";

// Deliberately neither Debug nor Serialize. Configuration stores only the scope.
pub(crate) struct Bearer(String);
impl Bearer {
    fn new(value: String) -> Result<Self> {
        if value.is_empty()
            || value.len() > 4096
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
        {
            bail!("Invalid dedicated inference credential")
        }
        Ok(Self(value))
    }
    pub(crate) fn apply(&self, request: RequestBuilder) -> Result<RequestBuilder> {
        let mut header = HeaderValue::from_str(&format!("Bearer {}", self.0))
            .map_err(|_| anyhow::anyhow!("Invalid inference authorization header"))?;
        header.set_sensitive(true);
        Ok(request.header(AUTHORIZATION, header))
    }
    pub(crate) fn forward(&self, command: &mut tokio::process::Command) {
        command.env(ENVIRONMENT_VARIABLE, &self.0);
    }
    pub(crate) fn reject_echo(&self, body: &[u8]) -> Result<()> {
        if body
            .windows(self.0.len())
            .any(|window| window == self.0.as_bytes())
        {
            bail!("Inference response contains protected credential material")
        }
        Ok(())
    }
}
pub(crate) fn resolve(config: &Config) -> Result<Option<Bearer>> {
    resolve_from(config, || {
        std::env::var(ENVIRONMENT_VARIABLE)
            .map_err(|_| anyhow::anyhow!("Dedicated inference credential is unavailable"))
    })
}
fn resolve_from(config: &Config, read: impl FnOnce() -> Result<String>) -> Result<Option<Bearer>> {
    let Some(scope) = &config.inference_auth_scope else {
        return Ok(None);
    };
    // Expert/model replacement cannot forward the primary token to another API.
    if local_endpoint(scope)? != local_endpoint(&config.api_base)? {
        return Ok(None);
    }
    Ok(Some(Bearer::new(read()?)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_require_explicit_matching_scope_and_valid_bearer_bytes() {
        let mut cfg = Config::default();
        assert!(
            resolve_from(&cfg, || panic!("Must not read environment by default"))
                .unwrap()
                .is_none()
        );
        cfg.inference_auth_scope = Some("http://127.0.0.1:9000/v1".into());
        assert!(resolve_from(&cfg, || panic!(
            "Must not read credentials for another endpoint"
        ))
        .unwrap()
        .is_none());
        cfg.api_base = "http://127.0.0.1:9000/v1/".into();
        assert!(resolve_from(&cfg, || Ok("fixture-token-123456789".into()))
            .unwrap()
            .is_some());
        for invalid in [
            "".to_string(),
            "line\nbreak".into(),
            "space key".into(),
            "x".repeat(4097),
        ] {
            assert!(Bearer::new(invalid).is_err());
        }
        let token = Bearer::new("fixture-token-123456789".into()).unwrap();
        assert!(token.reject_echo(b"safe response").is_ok());
        let error = token
            .reject_echo(b"echo fixture-token-123456789")
            .unwrap_err()
            .to_string();
        assert!(!error.contains("fixture-token"));
    }
}
