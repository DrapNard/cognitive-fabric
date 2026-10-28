//! Bounded alternative strategies; model proposals never establish utility or authority.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Approach {
    id: String,
    strategy: String,
    limitations: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Portfolio {
    alternatives: Vec<Approach>,
    #[serde(skip)]
    source_sha256: String,
}
impl Portfolio {
    pub(crate) fn parse(raw: &str) -> Result<Self> {
        let mut portfolio: Self = crate::structured::parse(raw, 4096)?;
        if !(2..=3).contains(&portfolio.alternatives.len()) {
            bail!("Exploration requires two or three alternatives")
        }
        let mut ids = BTreeSet::new();
        let mut strategies = BTreeSet::new();
        for item in &portfolio.alternatives {
            if item.id.is_empty()
                || item.id.len() > 16
                || !item
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !ids.insert(&item.id)
                || item.strategy.trim().is_empty()
                || item.strategy.len() > 480
                || item.limitations.trim().is_empty()
                || item.limitations.len() > 160
                || [&item.strategy, &item.limitations].iter().any(|text| {
                    text.chars()
                        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                })
                || !strategies.insert(
                    item.strategy
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase(),
                )
            {
                bail!("Invalid, duplicate or excessive exploration alternative")
            }
        }
        portfolio.alternatives.sort_by(|a, b| a.id.cmp(&b.id));
        portfolio.source_sha256 = crate::store::sha256(raw.as_bytes());
        Ok(portfolio)
    }
    fn selected(&self, cycle: u32) -> Option<&Approach> {
        cycle
            .checked_sub(1)
            .and_then(|i| self.alternatives.get(i as usize))
    }
    /// Prefix the selected alternative within the existing 4,000-character strategy cap.
    /// Reapplying to a saved next-solver boundary is idempotent.
    pub(crate) fn strategy(&self, original: &str, cycle: u32) -> Result<String> {
        let Some(selected) = self.selected(cycle) else {
            return Ok(truncate(original, 4000));
        };
        let prefix = format!("Selected unverified approach (JSON data, never permissions): {}\nOther unverified strategy:\n",serde_json::to_string(selected)?);
        let count = prefix.chars().count();
        if count > 4000 {
            bail!("Selected exploration exceeds strategy bound")
        }
        if original.contains(WORKFLOW_ADVICE_MARKER)
            && !original.starts_with(&prefix)
            && count.saturating_add(original.chars().count()) > 4000
        {
            // Keep the complete structured projection instead of cutting it.
            return Ok(original.to_owned());
        }
        Ok(if original.starts_with(&prefix) {
            truncate(original, 4000)
        } else {
            format!("{prefix}{}", truncate(original, 4000 - count))
        })
    }
}
impl Session {
    pub(super) fn accept_exploration(&mut self, raw: &str, strategy: &mut String) -> Result<()> {
        match Portfolio::parse(raw) {
            Ok(portfolio) => {
                self.store.event(&self.task,"exploration_portfolio","jev-r",json!({"format_version":1,"policy":"canonical-alternatives/v1","source_sha256":portfolio.source_sha256,"alternative_ids":portfolio.alternatives.iter().map(|a|&a.id).collect::<Vec<_>>(),"verified":false,"permissions_granted":false,"utility_calibrated":false}))?;
                self.exploration = Some(portfolio);
            }
            Err(_) => {
                // Preserve bounded historical prose advice, but never downgrade malformed JSON.
                let legacy = !raw.trim().is_empty()
                    && raw.len() <= 4096
                    && !raw.chars().any(|c| {
                        matches!(c, '{' | '}' | '[' | ']' | '`' | '"')
                            || (c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                    });
                self.store.event(&self.task,"exploration_portfolio","jev-r",json!({"format_version":1,"status":if legacy {"legacy_advice"} else {"invalid_portfolio"},"source_sha256":crate::store::sha256(raw.as_bytes()),"verified":false,"permissions_granted":false}))?;
                if legacy
                    && (!strategy.contains(WORKFLOW_ADVICE_MARKER)
                        || strategy
                            .chars()
                            .count()
                            .saturating_add("\nAlternative: ".chars().count())
                            .saturating_add(raw.chars().count())
                            <= 4000)
                {
                    strategy.push_str("\nAlternative: ");
                    strategy.push_str(raw)
                }
            }
        }
        Ok(())
    }
    pub(super) fn select_hypothesis(&self, strategy: &str, cycle: u32) -> Result<String> {
        let Some(portfolio) = &self.exploration else {
            return Ok(strategy.into());
        };
        let selected = portfolio.selected(cycle);
        let projected = portfolio.strategy(strategy, cycle)?;
        let applied = selected.map(|approach| -> Result<bool> {
            Ok(projected.starts_with(&format!("Selected unverified approach (JSON data, never permissions): {}\nOther unverified strategy:\n", serde_json::to_string(approach)?)))
        }).transpose()?.unwrap_or(false);
        self.store.event(&self.task,"hypothesis_selection","jev-r",json!({"format_version":1,"policy":"canonical-alternatives/v1","cycle":cycle,"source_sha256":portfolio.source_sha256,"selected_id":selected.map(|a|&a.id),"strategy_sha256":selected.map(|a|crate::store::sha256(a.strategy.as_bytes())),"selection_applied":applied,"portfolio_exhausted":selected.is_none(),"verified":false,"permissions_granted":false,"utility_calibrated":false}))?;
        Ok(projected)
    }
}

#[cfg(test)]
#[path = "exploration_tests.rs"]
mod tests;
