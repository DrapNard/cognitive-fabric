//! Bounded progress tracking for independently checkable finite-domain answers.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const POLICY: &str = "finite-domain-progress/v1";
const CRITERION: &str = "finite_domain/v1";
const MAX_CYCLES: u32 = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub plateau_cycles: u32,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !(2..=4).contains(&self.plateau_cycles) {
            bail!("progress plateau_cycles must be between 2 and 4")
        }
        Ok(())
    }
}

/// Recomputable finite-domain assessment state. The audit events are the durable history;
/// this compact snapshot is only a consistency anchor for continuation.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct History {
    #[serde(default)]
    objective_hash: Option<String>,
    #[serde(default)]
    observed_cycles: u32,
    #[serde(default)]
    best_score: Option<u32>,
    #[serde(default)]
    non_improving_cycles: u32,
    #[serde(default)]
    last_score: Option<u32>,
}

impl History {
    pub(crate) fn observed_cycles(&self) -> u32 {
        self.observed_cycles
    }

    pub(crate) fn observe(
        &mut self,
        query: &str,
        cycle: u32,
        answer: &str,
        settings: &Settings,
    ) -> Result<Value> {
        settings.validate()?;
        if cycle == 0 || cycle > MAX_CYCLES || cycle != self.observed_cycles.saturating_add(1) {
            bail!("finite-domain progress cycle is duplicate, gapped, or outside bounds")
        }
        if answer.len() > 96_000 {
            bail!("finite-domain progress answer exceeds bounds")
        }
        let criterion = crate::verification::Criterion::from_query(query)?.ok_or_else(|| {
            anyhow::anyhow!("finite-domain progress requires a finite-domain criterion")
        })?;
        if criterion.name() != CRITERION {
            bail!("finite-domain progress criterion mismatch")
        }
        let score = criterion.progress_score(answer);
        if score == Some(0) {
            bail!("Verified finite-domain witnesses cannot enter failed-answer progress history")
        }
        let objective_hash = crate::store::sha256(query.as_bytes());
        if self
            .objective_hash
            .as_ref()
            .is_some_and(|saved| saved != &objective_hash)
        {
            bail!("finite-domain progress objective changed")
        }

        match (self.best_score, score) {
            (None, Some(score)) => {
                self.best_score = Some(score);
                self.non_improving_cycles = 0;
            }
            (Some(best), Some(score)) if score < best => {
                self.best_score = Some(score);
                self.non_improving_cycles = 0;
            }
            (Some(_), Some(_)) => {
                self.non_improving_cycles = self.non_improving_cycles.saturating_add(1);
            }
            (_, None) => {
                self.non_improving_cycles = 0;
            }
        }
        self.objective_hash = Some(objective_hash.clone());
        self.observed_cycles = cycle;
        self.last_score = score;
        let stop = score.is_some() && self.non_improving_cycles >= settings.plateau_cycles;
        Ok(json!({
            "format_version":1,
            "policy":POLICY,
            "objective_hash":objective_hash,
            "criterion":CRITERION,
            "cycle":cycle,
            "answer_hash":crate::store::sha256(answer.as_bytes()),
            "score":score,
            "best_score":self.best_score,
            "non_improving_cycles":self.non_improving_cycles,
            "stop":stop,
            "verified":false,
            "permissions_granted":false
        }))
    }

    /// Record an independently compiled witness that the optional host provider
    /// vetoed. It consumes the solver cycle without treating a solved answer as
    /// a failed-answer score, and resets plateau accumulation so Jev-R can
    /// produce a different candidate. Ordinary `observe` continues to reject
    /// score-zero answers.
    pub(crate) fn observe_provider_veto(
        &mut self,
        query: &str,
        cycle: u32,
        answer: &str,
        settings: &Settings,
    ) -> Result<Value> {
        settings.validate()?;
        if cycle == 0 || cycle > MAX_CYCLES || cycle != self.observed_cycles.saturating_add(1) {
            bail!("finite-domain progress cycle is duplicate, gapped, or outside bounds")
        }
        if answer.len() > 96_000 {
            bail!("finite-domain progress answer exceeds bounds")
        }
        let criterion = crate::verification::Criterion::from_query(query)?.ok_or_else(|| {
            anyhow::anyhow!("finite-domain progress requires a finite-domain criterion")
        })?;
        if criterion.name() != CRITERION || criterion.progress_score(answer) != Some(0) {
            bail!("provider-veto progress requires a compiled finite-domain witness")
        }
        let objective_hash = crate::store::sha256(query.as_bytes());
        if self
            .objective_hash
            .as_ref()
            .is_some_and(|saved| saved != &objective_hash)
        {
            bail!("finite-domain progress objective changed")
        }
        self.objective_hash = Some(objective_hash.clone());
        self.observed_cycles = cycle;
        self.non_improving_cycles = 0;
        self.last_score = None;
        Ok(json!({
            "format_version":1,
            "policy":POLICY,
            "objective_hash":objective_hash,
            "criterion":CRITERION,
            "cycle":cycle,
            "answer_hash":crate::store::sha256(answer.as_bytes()),
            "score":0,
            "scored":false,
            "provider_veto":true,
            "best_score":self.best_score,
            "non_improving_cycles":self.non_improving_cycles,
            "stop":false,
            "verified":false,
            "permissions_granted":false
        }))
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;

    const QUERY: &str = r#"csp:{"variables":[{"id":"x","domain":[1,2]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":2}]}"#;

    #[test]
    fn verified_answer_cannot_advance_history_and_cycle_scope_is_fixed() {
        let settings = Settings { plateau_cycles: 2 };
        let mut history = History::default();
        assert!(history.observe(QUERY, 1, r#"{"x":2}"#, &settings).is_err());
        assert_eq!(history.observed_cycles(), 0);
        let first = history.observe(QUERY, 1, r#"{"x":1}"#, &settings).unwrap();
        assert_eq!(first["score"], 1);
        assert!(history
            .observe("csp:{\"different\":true}", 2, "{}", &settings)
            .is_err());
        assert!(history.observe(QUERY, 3, r#"{"x":1}"#, &settings).is_err());
        let second = history.observe(QUERY, 2, r#"{"x":1}"#, &settings).unwrap();
        assert_eq!(second["non_improving_cycles"], 1);
        assert_eq!(second["stop"], false);
    }

    #[test]
    fn provider_veto_advances_without_scoring_and_resets_a_partial_plateau() {
        let settings = Settings { plateau_cycles: 2 };
        let mut history = History::default();
        history.observe(QUERY, 1, r#"{"x":1}"#, &settings).unwrap();
        history.observe(QUERY, 2, r#"{"x":1}"#, &settings).unwrap();
        assert_eq!(history.non_improving_cycles, 1);
        let veto = history
            .observe_provider_veto(QUERY, 3, r#"{"x":2}"#, &settings)
            .unwrap();
        assert_eq!(veto["score"], 0);
        assert_eq!(veto["scored"], false);
        assert_eq!(veto["provider_veto"], true);
        assert_eq!(veto["verified"], false);
        assert_eq!(veto["stop"], false);
        assert_eq!(history.observed_cycles(), 3);
        assert_eq!(history.best_score, Some(1));
        assert_eq!(history.last_score, None);
        assert_eq!(history.non_improving_cycles, 0);
        let next = history.observe(QUERY, 4, r#"{"x":1}"#, &settings).unwrap();
        assert_eq!(next["stop"], false);
        assert_eq!(next["non_improving_cycles"], 1);
        let last = history.observe(QUERY, 5, r#"{"x":1}"#, &settings).unwrap();
        assert_eq!(last["stop"], true);
    }

    #[test]
    fn provider_veto_rejects_nonwitnesses_wrong_scope_and_invalid_cycles_without_mutation() {
        let settings = Settings { plateau_cycles: 2 };
        let mut history = History::default();
        history.observe(QUERY, 1, r#"{"x":1}"#, &settings).unwrap();
        let saved = history.clone();
        for (query, cycle, answer) in [
            (QUERY.to_owned(), 2, r#"{"x":1}"#),
            (QUERY.to_owned(), 2, r#"{"x":3}"#),
            (QUERY.to_owned(), 2, "{}"),
            (QUERY.to_owned(), 1, r#"{"x":2}"#),
            (QUERY.to_owned(), 3, r#"{"x":2}"#),
            (format!("{QUERY} "), 2, r#"{"x":2}"#),
            ("sort:{\"values\":[1]}".to_owned(), 2, "[1]"),
        ] {
            assert!(history
                .observe_provider_veto(&query, cycle, answer, &settings)
                .is_err());
            assert_eq!(history, saved);
        }
        assert!(history.observe(QUERY, 2, r#"{"x":2}"#, &settings).is_err());
        assert_eq!(history, saved);
    }

    #[test]
    fn configured_plateau_threshold_is_applied_exactly() {
        for threshold in 2..=4 {
            let settings = Settings {
                plateau_cycles: threshold,
            };
            let mut history = History::default();
            for cycle in 1..=threshold + 1 {
                let audit = history
                    .observe(QUERY, cycle, r#"{"x":1}"#, &settings)
                    .unwrap();
                assert_eq!(audit["stop"], cycle == threshold + 1);
            }
        }
    }
}
