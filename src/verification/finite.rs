//! Linear finite-domain assignment verification and bounded deterministic search.
use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const MAX_SEARCH_NODES: usize = 4096;

pub(crate) struct SearchOutcome {
    pub(crate) result: SearchResult,
    pub(crate) explored_nodes: usize,
}

pub(crate) enum SearchResult {
    Witness(String),
    Unsatisfiable,
    LimitReached,
    Cancelled,
}

struct SearchContext<'a> {
    criterion: &'a FiniteDomain,
    domain_bounds: BTreeMap<&'a str, (i64, i64)>,
    domains: BTreeMap<&'a str, &'a [i64]>,
}

impl SearchContext<'_> {
    fn visit(
        &self,
        depth: usize,
        values: &mut BTreeMap<String, i64>,
        max_nodes: usize,
        explored_nodes: &mut usize,
        should_stop: &mut impl FnMut() -> bool,
    ) -> Result<SearchResult> {
        if should_stop() {
            return Ok(SearchResult::Cancelled);
        }
        if *explored_nodes >= max_nodes {
            return Ok(SearchResult::LimitReached);
        }
        *explored_nodes += 1;

        for constraint in &self.criterion.constraints {
            if !FiniteDomain::partial_constraint_possible(
                constraint,
                values,
                &self.domain_bounds,
                &self.domains,
            ) {
                return Ok(SearchResult::Unsatisfiable);
            }
        }

        if depth == self.criterion.variables.len() {
            let answer = serde_json::to_string(values)?;
            // Keep the search result behind the same strict public witness checker.
            self.criterion.check(&answer)?;
            return Ok(SearchResult::Witness(answer));
        }

        let variable = &self.criterion.variables[depth];
        for value in &variable.domain {
            values.insert(variable.id.clone(), *value);
            let child = self.visit(depth + 1, values, max_nodes, explored_nodes, should_stop)?;
            values.remove(&variable.id);
            match child {
                SearchResult::Unsatisfiable => {}
                other => return Ok(other),
            }
        }
        Ok(SearchResult::Unsatisfiable)
    }
}

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

    fn partial_constraint_possible(
        constraint: &Constraint,
        values: &BTreeMap<String, i64>,
        domain_bounds: &BTreeMap<&str, (i64, i64)>,
        domains: &BTreeMap<&str, &[i64]>,
    ) -> bool {
        match constraint {
            Constraint::Linear {
                terms,
                relation,
                rhs,
            } => {
                let mut minimum = 0_i128;
                let mut maximum = 0_i128;
                for term in terms {
                    let coefficient = i128::from(term.coefficient);
                    if let Some(value) = values.get(&term.variable) {
                        let contribution = i128::from(*value) * coefficient;
                        minimum += contribution;
                        maximum += contribution;
                    } else {
                        let (domain_min, domain_max) = domain_bounds[term.variable.as_str()];
                        let (low, high) = if coefficient >= 0 {
                            (domain_min, domain_max)
                        } else {
                            (domain_max, domain_min)
                        };
                        minimum += i128::from(low) * coefficient;
                        maximum += i128::from(high) * coefficient;
                    }
                }
                let rhs = i128::from(*rhs);
                match relation {
                    Relation::Eq => minimum <= rhs && rhs <= maximum,
                    Relation::Ne => minimum != rhs || maximum != rhs,
                    Relation::Le => minimum <= rhs,
                    Relation::Ge => maximum >= rhs,
                }
            }
            Constraint::AllDifferent { variables } => {
                let mut assigned = BTreeSet::new();
                if !variables
                    .iter()
                    .filter_map(|variable| values.get(variable))
                    .all(|value| assigned.insert(*value))
                {
                    return false;
                }
                // A remaining variable needs an unused value. Also, the union
                // of all remaining domains must contain at least as many unused
                // values as there are remaining variables. This necessary
                // condition never treats an incomplete match as a witness.
                let remaining = variables.len() - assigned.len();
                let mut union = BTreeSet::new();
                for variable in variables.iter().filter(|id| !values.contains_key(*id)) {
                    let mut available = false;
                    for value in domains[variable.as_str()] {
                        if !assigned.contains(value) {
                            available = true;
                            if union.len() < remaining {
                                union.insert(*value);
                            }
                            if union.len() == remaining {
                                break;
                            }
                        }
                    }
                    if !available {
                        return false;
                    }
                }
                union.len() >= remaining
            }
        }
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

    /// Enumerates assignments in declaration order under a strict partial-assignment node cap.
    /// The empty root counts as one node. A stop callback is polled before the budget check at
    /// every node, including the root.
    pub(super) fn search(
        &self,
        max_nodes: usize,
        mut should_stop: impl FnMut() -> bool,
    ) -> Result<SearchOutcome> {
        if !(1..=MAX_SEARCH_NODES).contains(&max_nodes) {
            bail!("Finite-domain search node limit must be between 1 and {MAX_SEARCH_NODES}")
        }

        let context = SearchContext {
            criterion: self,
            domains: self
                .variables
                .iter()
                .map(|variable| (variable.id.as_str(), variable.domain.as_slice()))
                .collect(),
            domain_bounds: self
                .variables
                .iter()
                .map(|variable| {
                    let minimum = *variable
                        .domain
                        .iter()
                        .min()
                        .expect("validated nonempty domain");
                    let maximum = *variable
                        .domain
                        .iter()
                        .max()
                        .expect("validated nonempty domain");
                    (variable.id.as_str(), (minimum, maximum))
                })
                .collect(),
        };

        let mut explored_nodes = 0;
        let result = context.visit(
            0,
            &mut BTreeMap::new(),
            max_nodes,
            &mut explored_nodes,
            &mut should_stop,
        )?;
        let result = match result {
            SearchResult::Cancelled => SearchResult::Cancelled,
            _completed if should_stop() => SearchResult::Cancelled,
            completed => completed,
        };
        Ok(SearchOutcome {
            result,
            explored_nodes,
        })
    }
}

#[cfg(test)]
mod tests;
