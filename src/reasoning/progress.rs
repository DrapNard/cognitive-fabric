//! Bounded progress tracking for independently checkable finite-domain answers.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const POLICY: &str = "finite-domain-progress/v1";
const CRITERION: &str = "finite_domain/v1";
const PATH_POLICY: &str = "shortest-path-progress/v1";
const PATH_CRITERION: &str = "shortest_path/v1";
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

/// Separate versioned state for shortest-path progress. The original finite-
/// domain checkpoint shape and policy remain unchanged.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PathHistory {
    format_version: u32,
    policy: String,
    criterion: String,
    objective_hash: Option<String>,
    observed_cycles: u32,
    best_score: Option<u32>,
    non_improving_cycles: u32,
    last_score: Option<u32>,
}

impl Default for PathHistory {
    fn default() -> Self {
        Self {
            format_version: 1,
            policy: PATH_POLICY.into(),
            criterion: PATH_CRITERION.into(),
            objective_hash: None,
            observed_cycles: 0,
            best_score: None,
            non_improving_cycles: 0,
            last_score: None,
        }
    }
}

impl PathHistory {
    pub(crate) fn observed_cycles(&self) -> u32 {
        self.observed_cycles
    }

    /// Score a candidate only after the compiled checker has rejected it.
    /// Unparseable or incomparable answers consume a cycle and clear the
    /// consecutive plateau; they never become successful evidence.
    pub(crate) fn observe(
        &mut self,
        query: &str,
        cycle: u32,
        answer: &str,
        settings: &Settings,
    ) -> Result<Value> {
        self.validate_scope(query, cycle, answer, settings)?;
        let criterion = crate::verification::Criterion::from_query(query)?.ok_or_else(|| {
            anyhow::anyhow!("Shortest-path progress requires a structured criterion")
        })?;
        if criterion.name() != PATH_CRITERION {
            bail!("Shortest-path progress criterion mismatch")
        }
        if criterion.check(answer).is_ok() {
            bail!("Accepted shortest-path witnesses cannot enter failed-answer progress history")
        }
        let score = criterion.progress_score(answer).filter(|score| *score > 0);
        let objective_hash = crate::store::sha256(query.as_bytes());
        self.require_objective(&objective_hash)?;
        self.apply(cycle, objective_hash.clone(), score);
        Ok(self.audit(&objective_hash, cycle, answer, score, false, settings))
    }

    /// A compiled shortest-path witness vetoed by the optional host provider is
    /// unscored. It consumes the cycle and clears the plateau without inventing
    /// a failed answer or a success claim.
    pub(crate) fn observe_provider_veto(
        &mut self,
        query: &str,
        cycle: u32,
        answer: &str,
        settings: &Settings,
    ) -> Result<Value> {
        self.validate_scope(query, cycle, answer, settings)?;
        let criterion = crate::verification::Criterion::from_query(query)?.ok_or_else(|| {
            anyhow::anyhow!("Shortest-path progress requires a structured criterion")
        })?;
        if criterion.name() != PATH_CRITERION || criterion.check(answer).is_err() {
            bail!("Provider-veto progress requires a compiled shortest-path witness")
        }
        let objective_hash = crate::store::sha256(query.as_bytes());
        self.require_objective(&objective_hash)?;
        self.apply(cycle, objective_hash.clone(), None);
        Ok(self.audit(&objective_hash, cycle, answer, None, true, settings))
    }

    fn validate_scope(
        &self,
        query: &str,
        cycle: u32,
        answer: &str,
        settings: &Settings,
    ) -> Result<()> {
        settings.validate()?;
        if self.format_version != 1
            || self.policy != PATH_POLICY
            || self.criterion != PATH_CRITERION
        {
            bail!("Unsupported shortest-path progress state")
        }
        if cycle == 0 || cycle > MAX_CYCLES || cycle != self.observed_cycles.saturating_add(1) {
            bail!("Shortest-path progress cycle is duplicate, gapped, or outside bounds")
        }
        if answer.len() > 96_000 {
            bail!("Shortest-path progress answer exceeds bounds")
        }
        if !query.starts_with("path:") || query.len() > 12000 {
            bail!("Shortest-path progress objective is not a bounded path criterion")
        }
        Ok(())
    }

    fn require_objective(&self, objective_hash: &str) -> Result<()> {
        if self
            .objective_hash
            .as_ref()
            .is_some_and(|saved| saved != objective_hash)
        {
            bail!("Shortest-path progress objective changed")
        }
        Ok(())
    }

    fn apply(&mut self, cycle: u32, objective_hash: String, score: Option<u32>) {
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
        self.objective_hash = Some(objective_hash);
        self.observed_cycles = cycle;
        self.last_score = score;
    }

    fn audit(
        &self,
        objective_hash: &str,
        cycle: u32,
        answer: &str,
        score: Option<u32>,
        provider_veto: bool,
        settings: &Settings,
    ) -> Value {
        let mut audit = json!({
            "format_version":1,
            "policy":PATH_POLICY,
            "criterion":PATH_CRITERION,
            "objective_hash":objective_hash,
            "cycle":cycle,
            "answer_hash":crate::store::sha256(answer.as_bytes()),
            "score":score,
            "best_score":self.best_score,
            "non_improving_cycles":self.non_improving_cycles,
            "stop":score.is_some() && self.non_improving_cycles >= settings.plateau_cycles,
            "scored":score.is_some(),
            "verified":false,
            "permissions_granted":false
        });
        if provider_veto {
            audit["provider_veto"] = json!(true);
            audit["stop"] = json!(false);
        }
        audit
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;

    const QUERY: &str = r#"csp:{"variables":[{"id":"x","domain":[1,2]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":2}]}"#;
    const PATH_QUERY: &str =
        r#"path:{"nodes":["a","b","c"],"edges":[["a","b"],["b","c"]],"start":"a","goal":"c"}"#;

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

    #[test]
    fn shortest_path_history_improves_then_stops_only_after_a_plateau() {
        let settings = Settings { plateau_cycles: 2 };
        let mut history = PathHistory::default();
        let first = history.observe(PATH_QUERY, 1, "[]", &settings).unwrap();
        assert_eq!(first["score"], 133);
        assert_eq!(first["scored"], true);
        assert_eq!(first["stop"], false);

        let improved = history
            .observe(PATH_QUERY, 2, r#"["a","b","c","c"]"#, &settings)
            .unwrap();
        assert_eq!(improved["score"], 66);
        assert_eq!(improved["best_score"], 66);
        assert_eq!(improved["non_improving_cycles"], 0);
        assert_eq!(improved["stop"], false);

        let equal = history
            .observe(PATH_QUERY, 3, r#"["a","b","c","c"]"#, &settings)
            .unwrap();
        assert_eq!(equal["non_improving_cycles"], 1);
        assert_eq!(equal["stop"], false);
        let plateau = history
            .observe(PATH_QUERY, 4, r#"["a","b","c","c"]"#, &settings)
            .unwrap();
        assert_eq!(plateau["non_improving_cycles"], 2);
        assert_eq!(plateau["stop"], true);
    }

    #[test]
    fn incomparable_path_resets_the_streak_and_provider_veto_is_unscored() {
        let settings = Settings { plateau_cycles: 2 };
        let mut history = PathHistory::default();
        history.observe(PATH_QUERY, 1, "[]", &settings).unwrap();
        history
            .observe(PATH_QUERY, 2, r#"["a","b","c","c"]"#, &settings)
            .unwrap();
        let unscored = history
            .observe(PATH_QUERY, 3, r#"["unknown"]"#, &settings)
            .unwrap();
        assert_eq!(unscored["score"], Value::Null);
        assert_eq!(unscored["scored"], false);
        assert_eq!(unscored["non_improving_cycles"], 0);
        assert_eq!(unscored["stop"], false);
        assert_eq!(history.best_score, Some(66));
        assert_eq!(history.last_score, None);

        let veto = history
            .observe_provider_veto(PATH_QUERY, 4, r#"["a","b","c"]"#, &settings)
            .unwrap();
        assert_eq!(veto["score"], Value::Null);
        assert_eq!(veto["scored"], false);
        assert_eq!(veto["provider_veto"], true);
        assert_eq!(veto["verified"], false);
        assert_eq!(veto["stop"], false);
        assert_eq!(history.observed_cycles(), 4);
        assert_eq!(history.best_score, Some(66));
        assert_eq!(history.last_score, None);
        assert_eq!(history.non_improving_cycles, 0);
    }

    #[test]
    fn accepted_path_cannot_be_recorded_as_a_failed_answer_and_bad_scope_is_atomic() {
        let settings = Settings { plateau_cycles: 2 };
        let mut history = PathHistory::default();
        assert!(history
            .observe(PATH_QUERY, 1, r#"["a","b","c"]"#, &settings)
            .is_err());
        assert_eq!(history.observed_cycles(), 0);
        assert!(history
            .observe_provider_veto(PATH_QUERY, 1, "[]", &settings)
            .is_err());
        assert_eq!(history.observed_cycles(), 0);

        let audit = history.observe(PATH_QUERY, 1, "[]", &settings).unwrap();
        assert_eq!(audit["format_version"], 1);
        assert_eq!(audit["policy"], PATH_POLICY);
        assert_eq!(audit["criterion"], PATH_CRITERION);
        assert_eq!(
            audit["objective_hash"],
            crate::store::sha256(PATH_QUERY.as_bytes())
        );
        assert_eq!(audit["answer_hash"], crate::store::sha256(b"[]"));
        assert_eq!(audit["cycle"], 1);
        assert_eq!(audit["verified"], false);
        assert_eq!(audit["permissions_granted"], false);
        let saved = history.clone();
        assert!(history
            .observe(
                "path:{\"nodes\":[\"a\"],\"edges\":[],\"start\":\"a\",\"goal\":\"a\"}",
                2,
                "[]",
                &settings
            )
            .is_err());
        assert_eq!(history, saved);
    }
}
