//! Validated metrics explicitly reported by a supported inference provider.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Provider family that supplied the optional inference timing metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMetricSource {
    LmStudioNative,
    LlamaCppNative,
}

/// Timings/rates reported by a native provider response.
///
/// These values are provider-reported observations, not independently measured
/// benchmarks or attestations of model weights.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderMetrics {
    pub format_version: u16,
    pub source: ProviderMetricSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_load_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_to_first_token_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_processing_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_per_second: Option<f64>,
}

impl ProviderMetrics {
    /// Create an empty version-1 metrics record for a known provider.
    pub fn new(source: ProviderMetricSource) -> Self {
        Self {
            format_version: 1,
            source,
            model_load_ms: None,
            time_to_first_token_ms: None,
            prompt_processing_ms: None,
            generation_ms: None,
            output_tokens_per_second: None,
        }
    }

    /// Check the version and accepted numeric range before data is exposed.
    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1 {
            bail!("Unsupported provider metrics format version")
        }
        let durations = [
            self.model_load_ms,
            self.time_to_first_token_ms,
            self.prompt_processing_ms,
            self.generation_ms,
        ];
        if durations
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite() || !(0.0..=86_400_000.0).contains(&value))
            || self.output_tokens_per_second.is_some_and(|value| {
                !value.is_finite() || !(0.0..=1_000_000_000.0).contains(&value)
            })
        {
            bail!("Provider metrics contain an out-of-range value")
        }
        if durations.into_iter().all(|value| value.is_none())
            && self.output_tokens_per_second.is_none()
        {
            bail!("Provider metrics must contain at least one measurement")
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_version_source_and_bounds() {
        let mut metrics = ProviderMetrics::new(ProviderMetricSource::LmStudioNative);
        metrics.time_to_first_token_ms = Some(0.0);
        metrics.model_load_ms = Some(86_400_000.0);
        metrics.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&metrics).unwrap()["source"],
            "lm_studio_native"
        );

        metrics.format_version = 2;
        assert!(metrics.validate().is_err());
        metrics.format_version = 1;
        metrics.time_to_first_token_ms = Some(f64::INFINITY);
        assert!(metrics.validate().is_err());
        metrics.time_to_first_token_ms = Some(-0.1);
        assert!(metrics.validate().is_err());
        metrics.time_to_first_token_ms = Some(1.0);
        metrics.output_tokens_per_second = Some(1_000_000_000.1);
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn requires_at_least_one_metric_and_rejects_unknown_fields_and_sources() {
        let metrics = ProviderMetrics::new(ProviderMetricSource::LlamaCppNative);
        assert!(metrics.validate().is_err());
        assert!(serde_json::from_str::<ProviderMetrics>(
            r#"{"format_version":1,"source":"other","model_load_ms":1.0}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ProviderMetrics>(
            r#"{"format_version":1,"source":"llama_cpp_native","unexpected":1}"#
        )
        .is_err());
    }
}
