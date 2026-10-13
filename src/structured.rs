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
    Ok(serde_json::from_value(crate::strict_json::value(
        json.as_bytes(),
        max_bytes,
    )?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advisory_values_reject_nested_duplicates_inside_valid_fences() {
        for raw in [
            r#"{"metadata":{"choice":1,"choice":2}}"#,
            r#"{"items":[{"name":"a","\u006eame":"b"}]}"#,
        ] {
            assert!(parse::<serde_json::Value>(raw, 1024).is_err());
            assert!(parse::<serde_json::Value>(&format!("```json\n{raw}\n```"), 1024).is_err());
        }
        let raw = r#"{"metadata":{"choice":1},"items":[true,null,2.5]}"#;
        assert_eq!(
            parse::<serde_json::Value>(raw, 1024).unwrap(),
            serde_json::from_str::<serde_json::Value>(raw).unwrap()
        );
    }
}
