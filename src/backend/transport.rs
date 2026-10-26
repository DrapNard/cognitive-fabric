//! Shared inference transport; authorization and deadlines stay request-local.
use anyhow::Result;
use std::{sync::OnceLock, time::Duration};

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Reuse connections across independent role calls without serializing them.
/// No credentials, task state or model selection are installed as client defaults.
/// Callers must set their original timeout or enforce an absolute deadline.
pub(crate) fn client() -> Result<reqwest::Client> {
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let candidate = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(4)
        .pool_idle_timeout(Duration::from_secs(30))
        .build()?;
    // Concurrent first callers may build candidates; only one pool is retained.
    let _ = CLIENT.set(candidate.clone());
    Ok(CLIENT.get().cloned().unwrap_or(candidate))
}
