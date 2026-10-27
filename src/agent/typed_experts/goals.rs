//! Original-goal adapters for host-pinned typed client experts.
//!
//! These adapters are trusted host code, separate from manifest-backed Expert
//! Network providers. Requests and acceptance criteria come only from the
//! caller's exact typed query or an explicitly labeled language translation.
//! A translated criterion cannot prove fidelity to the original natural-language
//! request, even when its computation and final payload pass compiled checks.

use crate::{
    fabric::modules::{Capability, Request, Response},
    math,
    reasoning::modules,
    verification::Criterion,
};
use anyhow::{bail, Result};

const MAX_STRUCTURED_OUTPUT_BYTES: usize = 8_192;
const MAX_SQL_OUTPUT_BYTES: usize = 16_384;

/// Validated typed work from exact input or a labeled language translation.
///
/// The candidate is present only when a deterministic host solver supplies a
/// candidate that a registered expert can diagnose. SQL and finite-domain work
/// require the registered expert to produce a result. The original criterion is
/// retained for deterministic validation after dispatch. A translated criterion
/// does not prove fidelity to the user's original meaning.
pub(in crate::agent) struct Work {
    criterion_query: String,
    source_query_sha256: String,
    translated: bool,
    request: Request,
    candidate: Option<String>,
    criterion: Option<Criterion>,
    checker: &'static str,
}

impl Work {
    /// Cheap route hint for callers that need to inspect pinned availability
    /// before parsing a potentially larger typed request.
    pub(in crate::agent) fn capability_hint(query: &str) -> Option<Capability> {
        if math::extract(query).is_some() {
            return Some(Capability::MathEvaluate);
        }
        if query.starts_with("sql:") {
            return Some(Capability::SqlQuery);
        }
        if query.starts_with("sort:") {
            return Some(Capability::SortingVerify);
        }
        if query.starts_with("order:") {
            return Some(Capability::OrderingVerify);
        }
        if query.starts_with("path:") {
            return Some(Capability::PathVerify);
        }
        if query.starts_with("csp:") {
            return Some(Capability::ConstraintsSolve);
        }
        None
    }

    pub(in crate::agent) fn has_explicit_prefix(query: &str) -> bool {
        query.split_once(':').is_some_and(|(prefix, _)| {
            matches!(
                prefix.trim().to_ascii_lowercase().as_str(),
                "calc" | "calculate" | "math" | "sql" | "sort" | "order" | "path" | "csp"
            )
        })
    }

    pub(in crate::agent) fn from_translation(
        original: &str,
        translation: &super::translation::Translation,
    ) -> Result<Self> {
        if Self::has_explicit_prefix(original) {
            bail!("Language translation cannot replace an explicit typed goal")
        }
        let query = translation.canonical_query()?;
        let mut work = Self::from_query(&query)?.ok_or_else(|| {
            anyhow::anyhow!("Language translation has no supported typed request")
        })?;
        if work.capability() != translation.capability() {
            bail!("Language translation changed its declared typed capability")
        }
        work.source_query_sha256 = crate::store::sha256(original.as_bytes());
        work.translated = true;
        Ok(work)
    }

    /// Build typed work only for supported exact prefixes or bounded math syntax.
    /// Malformed recognized goals fail closed; unrelated language remains for the
    /// ordinary language-interface path.
    pub(in crate::agent) fn from_query(query: &str) -> Result<Option<Self>> {
        if let Some(expression) = math::extract(query) {
            let candidate = math::calculate(expression)?;
            return Ok(Some(Self {
                criterion_query: query.to_owned(),
                source_query_sha256: crate::store::sha256(query.as_bytes()),
                translated: false,
                request: Request::Arithmetic {
                    expression: expression.to_owned(),
                },
                candidate: Some(candidate),
                criterion: None,
                checker: "compiled_math_parser",
            }));
        }

        if let Some(raw) = query.strip_prefix("sql:") {
            if raw.len() > 32_000 {
                bail!("SQL typed goal exceeds its byte limit")
            }
            // Reject duplicate JSON keys before the domain parser can collapse them.
            crate::strict_json::value(raw.as_bytes(), 32_000)?;
            let request = modules::sql_request(raw)?;
            if request.capability() != Capability::SqlQuery {
                bail!("SQL adapter produced an unexpected typed capability")
            }
            return Ok(Some(Self {
                criterion_query: query.to_owned(),
                source_query_sha256: crate::store::sha256(query.as_bytes()),
                translated: false,
                request,
                candidate: None,
                criterion: None,
                checker: "bounded_sql_reexecution",
            }));
        }

        if query.starts_with("sort:")
            || query.starts_with("order:")
            || query.starts_with("path:")
            || query.starts_with("csp:")
        {
            let raw = query
                .split_once(':')
                .map(|(_, raw)| raw)
                .unwrap_or_default();
            if raw.len() > 10_000 {
                bail!("Structured typed goal exceeds its byte limit")
            }
            // Criterion parsers validate domain bounds; strict_json additionally
            // rejects duplicate keys in the original request.
            crate::strict_json::value(raw.as_bytes(), 10_000)?;
            let criterion = Criterion::from_query(query)?
                .ok_or_else(|| anyhow::anyhow!("Recognized typed goal has no criterion"))?;

            if matches!(&criterion, Criterion::FiniteDomain(_)) {
                let request = modules::finite_domain_request(query)?
                    .ok_or_else(|| anyhow::anyhow!("Finite-domain goal has no typed request"))?;
                if request.capability() != Capability::ConstraintsSolve {
                    bail!("Finite-domain adapter produced an unexpected typed capability")
                }
                return Ok(Some(Self {
                    criterion_query: query.to_owned(),
                    source_query_sha256: crate::store::sha256(query.as_bytes()),
                    translated: false,
                    request,
                    candidate: None,
                    criterion: Some(criterion),
                    checker: "original_finite_domain_criterion",
                }));
            }

            let candidate = criterion
                .deterministic_answer()?
                .ok_or_else(|| anyhow::anyhow!("Structured goal has no deterministic candidate"))?;
            if candidate.len() > MAX_STRUCTURED_OUTPUT_BYTES {
                bail!("Deterministic structured candidate exceeds its byte limit")
            }
            let request = modules::candidate_verification_request(query, &candidate)?
                .ok_or_else(|| anyhow::anyhow!("Structured goal has no typed verifier request"))?;
            if request.capability() == Capability::ConstraintsSolve
                || request.capability() != criterion_capability(&criterion)
            {
                bail!("Structured adapter produced an unexpected typed capability")
            }
            return Ok(Some(Self {
                criterion_query: query.to_owned(),
                source_query_sha256: crate::store::sha256(query.as_bytes()),
                translated: false,
                request,
                candidate: Some(candidate),
                criterion: Some(criterion),
                checker: "original_structured_criterion",
            }));
        }

        Ok(None)
    }

    pub(in crate::agent) fn capability(&self) -> Capability {
        self.request.capability()
    }

    pub(in crate::agent) fn request(&self) -> &Request {
        &self.request
    }

    pub(in crate::agent) fn query_digest(&self) -> String {
        self.source_query_sha256.clone()
    }

    pub(in crate::agent) fn criterion_digest(&self) -> String {
        crate::store::sha256(self.criterion_query.as_bytes())
    }

    pub(in crate::agent) fn translated(&self) -> bool {
        self.translated
    }

    pub(in crate::agent) fn checker(&self) -> &'static str {
        self.checker
    }

    /// Describe the exact response shape the language interface must preserve
    /// when it turns a verified typed result into the final answer.
    pub(in crate::agent) fn output_contract(&self) -> &'static str {
        match self.capability() {
            Capability::MathEvaluate => {
                "Return exactly the supplied numeric result, with no prose, code fences, or added claims."
            }
            Capability::SqlQuery => {
                "Return only JSON equal to the supplied result in structure, array order, and value types; no prose, code fences, or added claims."
            }
            Capability::SortingVerify
            | Capability::OrderingVerify
            | Capability::PathVerify
            | Capability::ConstraintsSolve => {
                "Return only a JSON answer accepted by the original structured criterion; no prose, code fences, or added claims."
            }
        }
    }

    /// Check the language interface's final presentation against the trusted
    /// typed result. This does not rerun expert dispatch or grant authority to
    /// the presentation model.
    pub(in crate::agent) fn verify_presentation(
        &self,
        verified_value: &str,
        answer: &str,
    ) -> Result<()> {
        match self.capability() {
            Capability::MathEvaluate => {
                if answer.trim() != verified_value {
                    bail!("Language presentation changed the verified numeric result")
                }
            }
            Capability::SqlQuery => {
                if answer.len() > MAX_SQL_OUTPUT_BYTES
                    || verified_value.len() > MAX_SQL_OUTPUT_BYTES
                {
                    bail!("SQL presentation exceeds its byte limit")
                }
                let presented = crate::strict_json::value(answer.as_bytes(), MAX_SQL_OUTPUT_BYTES)?;
                let verified =
                    crate::strict_json::value(verified_value.as_bytes(), MAX_SQL_OUTPUT_BYTES)?;
                if presented != verified {
                    bail!("Language presentation changed the verified SQL result")
                }
            }
            Capability::SortingVerify
            | Capability::OrderingVerify
            | Capability::PathVerify
            | Capability::ConstraintsSolve => {
                if answer.len() > MAX_STRUCTURED_OUTPUT_BYTES {
                    bail!("Structured presentation exceeds its byte limit")
                }
                self.criterion
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Original structured criterion is missing"))?
                    .check(answer)?;
            }
        }
        Ok(())
    }

    /// Validate a trusted expert's response against the retained typed criterion.
    /// No diagnostic flag or assignment is accepted without the independent
    /// compiled host checker.
    pub(in crate::agent) async fn verify_response(&self, response: Response) -> Result<String> {
        match (self.capability(), response) {
            (Capability::MathEvaluate, Response::Arithmetic { value }) => {
                let candidate = self
                    .candidate
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Original arithmetic result is missing"))?;
                if value != candidate || value.len() > 256 {
                    bail!("Typed arithmetic expert failed independent verification")
                }
                Ok(value)
            }
            (Capability::SqlQuery, response @ Response::Sql { .. }) => {
                let raw = self
                    .criterion_query
                    .strip_prefix("sql:")
                    .ok_or_else(|| anyhow::anyhow!("Original SQL request is missing"))?;
                let actual = modules::sql_response(response)?;
                if actual.len() > MAX_SQL_OUTPUT_BYTES {
                    bail!("Typed SQL result exceeds its byte limit")
                }
                let expected = crate::sql::execute(raw.to_owned()).await?;
                if expected.len() > MAX_SQL_OUTPUT_BYTES {
                    bail!("Compiled SQL result exceeds its byte limit")
                }
                let actual: serde_json::Value =
                    crate::strict_json::value(actual.as_bytes(), MAX_SQL_OUTPUT_BYTES)?;
                let expected: serde_json::Value =
                    crate::strict_json::value(expected.as_bytes(), MAX_SQL_OUTPUT_BYTES)?;
                if actual != expected {
                    bail!("Typed SQL expert failed bounded re-execution verification")
                }
                Ok(expected.to_string())
            }
            (Capability::SortingVerify, Response::SortDiagnostic { claimed_valid })
            | (Capability::OrderingVerify, Response::OrderingDiagnostic { claimed_valid })
            | (Capability::PathVerify, Response::PathDiagnostic { claimed_valid }) => {
                if !claimed_valid {
                    bail!("Typed structured expert rejected its deterministic candidate")
                }
                let candidate = self
                    .candidate
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Original structured candidate is missing"))?;
                if candidate.len() > MAX_STRUCTURED_OUTPUT_BYTES {
                    bail!("Typed structured candidate exceeds its byte limit")
                }
                self.criterion
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Original structured criterion is missing"))?
                    .check(candidate)?;
                Ok(candidate.to_owned())
            }
            (Capability::ConstraintsSolve, Response::FiniteAssignment { values }) => {
                let assignment = serde_json::to_string(&values)?;
                if assignment.len() > MAX_STRUCTURED_OUTPUT_BYTES {
                    bail!("Typed finite-domain assignment exceeds its byte limit")
                }
                self.criterion
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Original finite-domain criterion is missing"))?
                    .check(&assignment)?;
                Ok(assignment)
            }
            _ => bail!("Typed expert returned an unexpected response kind"),
        }
    }
}

fn criterion_capability(criterion: &Criterion) -> Capability {
    match criterion {
        Criterion::Sorting(_) => Capability::SortingVerify,
        Criterion::Ordering(_) => Capability::OrderingVerify,
        Criterion::ShortestPath(_) => Capability::PathVerify,
        Criterion::FiniteDomain(_) => Capability::ConstraintsSolve,
    }
}
