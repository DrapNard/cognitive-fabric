//! Opt-in admission limits for deterministic finite-domain search.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub max_nodes: usize,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !(1..=4096).contains(&self.max_nodes) {
            bail!("finite_search max_nodes must be between 1 and 4096")
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_budget_is_bounded_and_configuration_is_opt_in() {
        let mut cfg = crate::config::Config::default();
        let legacy = serde_json::to_value(&cfg).unwrap();
        assert!(legacy.get("finite_search").is_none());
        for max_nodes in [1, 4096] {
            let settings = Settings { max_nodes };
            settings.validate().unwrap();
            cfg.finite_search = Some(settings);
            cfg.validate().unwrap();
            let encoded = serde_json::to_value(&cfg).unwrap();
            let decoded: crate::config::Config = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded.finite_search.unwrap().max_nodes, max_nodes);
        }
        for max_nodes in [0, 4097, usize::MAX] {
            assert!(Settings { max_nodes }.validate().is_err());
        }
    }
}
