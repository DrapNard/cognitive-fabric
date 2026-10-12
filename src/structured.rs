//! Bounded advisory JSON parsing; never extract objects from surrounding prose.
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;

pub(crate) fn parse<T: DeserializeOwned>(raw: &str, max_bytes: usize) -> Result<T> {
    if raw.len() > max_bytes {
        bail!("Structured response exceeds byte limit")
    }
    let trimmed = raw.trim();
    let json = if let Some(body) = trimmed
        .strip_prefix("```json\n")
        .or_else(|| trimmed.strip_prefix("```json\r\n"))
    {
        body.strip_suffix("```")
            .context("Incomplete structured JSON fence")?
    } else {
        trimmed
    };
    Ok(serde_json::from_str(json)?)
}
