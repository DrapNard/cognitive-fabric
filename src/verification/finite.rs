//! Linear finite-domain assignment verification; no search or generated execution.
use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FiniteDomain {
    variables: Vec<Variable>,
    constraints: Vec<Constraint>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variable {
    id: String,
    domain: Vec<i64>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Constraint {
    Linear {
        terms: Vec<Term>,
        relation: Relation,
        rhs: i64,
    },
    AllDifferent {
        variables: Vec<String>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Term {
    variable: String,
    coefficient: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Relation {
    Eq,
    Ne,
    Le,
    Ge,
}

impl FiniteDomain {
    pub(super) fn parse(raw: &str) -> Result<Self> {
        let value = crate::strict_json::value(raw.as_bytes(), 10_000)?;
        let criterion: Self = serde_json::from_value(value)?;
        if !(1..=32).contains(&criterion.variables.len())
            || !(1..=128).contains(&criterion.constraints.len())
        {
            bail!("Finite-domain criterion requires 1–32 variables and 1–128 constraints")
        }
        let mut names = BTreeSet::new();
        for variable in &criterion.variables {
            if variable.id.is_empty()
                || variable.id.len() > 32
                || !variable
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !names.insert(variable.id.as_str())
                || !(1..=64).contains(&variable.domain.len())
                || variable.domain.iter().collect::<BTreeSet<_>>().len() != variable.domain.len()
            {
                bail!("Invalid finite-domain variable identity or domain")
            }
        }
        for constraint in &criterion.constraints {
            let mut seen = BTreeSet::new();
            match constraint {
                Constraint::Linear { terms, .. } => {
                    if !(1..=16).contains(&terms.len()) {
                        bail!("Linear constraint requires 1–16 terms")
                    }
                    for term in terms {
                        if !names.contains(term.variable.as_str())
                            || !seen.insert(term.variable.as_str())
                            || term.coefficient == 0
                            || !(-1_000_000..=1_000_000).contains(&term.coefficient)
                        {
                            bail!("Invalid linear term identity or coefficient")
                        }
                    }
                }
                Constraint::AllDifferent { variables } => {
                    if !(2..=32).contains(&variables.len()) {
                        bail!("All-different requires 2–32 variables")
                    }
                    for variable in variables {
                        if !names.contains(variable.as_str()) || !seen.insert(variable.as_str()) {
                            bail!("Unknown or repeated all-different variable")
                        }
                    }
                }
            }
        }
        Ok(criterion)
    }
    fn assignment(&self, answer: &str) -> Result<BTreeMap<String, i64>> {
        let values: BTreeMap<String, i64> =
            serde_json::from_value(crate::strict_json::value(answer.as_bytes(), 8192)?)?;
        if values.len() != self.variables.len() {
            bail!("Assignment must contain exactly the declared variables")
        }
        for variable in &self.variables {
            let value = values
                .get(&variable.id)
                .ok_or_else(|| anyhow::anyhow!("Assignment omits a declared variable"))?;
            if !variable.domain.contains(value) {
                bail!("Assignment value is outside its declared domain")
            }
        }
        Ok(values)
    }

    fn constraint_accepts(constraint: &Constraint, values: &BTreeMap<String, i64>) -> bool {
        match constraint {
            Constraint::Linear {
                terms,
                relation,
                rhs,
            } => {
                // 16 terms × i64 × bounded coefficient fits i128, including i64::MIN.
                let sum: i128 = terms
                    .iter()
                    .map(|t| i128::from(values[&t.variable]) * i128::from(t.coefficient))
                    .sum();
                let rhs = i128::from(*rhs);
                match relation {
                    Relation::Eq => sum == rhs,
                    Relation::Ne => sum != rhs,
                    Relation::Le => sum <= rhs,
                    Relation::Ge => sum >= rhs,
                }
            }
            Constraint::AllDifferent { variables } => {
                variables
                    .iter()
                    .map(|v| values[v])
                    .collect::<BTreeSet<_>>()
                    .len()
                    == variables.len()
            }
        }
    }

    fn violation_count(&self, values: &BTreeMap<String, i64>) -> usize {
        self.constraints
            .iter()
            .filter(|constraint| !Self::constraint_accepts(constraint, values))
            .count()
    }

    pub(super) fn check(&self, answer: &str) -> Result<()> {
        let values = self.assignment(answer)?;
        for (index, constraint) in self.constraints.iter().enumerate() {
            if !Self::constraint_accepts(constraint, &values) {
                bail!("Assignment violates declared constraint {index}")
            }
        }
        Ok(())
    }

    pub(super) fn progress_score(&self, answer: &str) -> Option<u32> {
        let values = self.assignment(answer).ok()?;
        u32::try_from(self.violation_count(&values)).ok()
    }
}

#[cfg(test)]
mod tests;
