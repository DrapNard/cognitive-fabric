//! Conservative, bounded contradiction detection over explicit scalar claims.
use super::{Constraint, ConstraintOperator, ProjectMemoryRecord};
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::Value;
use std::cmp::Ordering;

const MAX_CONFLICTS: usize = 256;

#[derive(Clone, Debug, Serialize)]
pub struct MemoryConflict {
    pub subject: String,
    pub predicate: String,
    pub left_claim: String,
    pub right_claim: String,
    pub reason: &'static str,
}

impl ProjectMemoryRecord {
    /// Detect explicit pairwise contradictions, not semantic truth or completeness.
    /// Incomparable numeric representations and unsupported relationships are skipped.
    pub fn conflicts(&self) -> Result<Vec<MemoryConflict>> {
        self.validate()?;
        let mut conflicts = Vec::new();
        for fact in &self.facts {
            for constraint in &self.constraints {
                if fact.subject == constraint.subject
                    && fact.predicate == constraint.predicate
                    && satisfies(&fact.value, constraint) == Some(false)
                {
                    append(
                        &mut conflicts,
                        &fact.subject,
                        &fact.predicate,
                        &fact.id,
                        &constraint.id,
                        "fact_violates_constraint",
                    )?;
                }
            }
        }
        for (index, left) in self.constraints.iter().enumerate() {
            for right in &self.constraints[index + 1..] {
                if left.subject == right.subject
                    && left.predicate == right.predicate
                    && incompatible(left, right)
                {
                    append(
                        &mut conflicts,
                        &left.subject,
                        &left.predicate,
                        &left.id,
                        &right.id,
                        "incompatible_constraints",
                    )?;
                }
            }
        }
        Ok(conflicts)
    }

    pub(crate) fn require_consistent_claims(&self) -> Result<()> {
        if !self.conflicts()?.is_empty() {
            bail!("Structured project memory contains contradictory claims; inspect project-memory-conflicts and publish a corrected revision")
        }
        Ok(())
    }
}

fn append(
    conflicts: &mut Vec<MemoryConflict>,
    subject: &str,
    predicate: &str,
    left: &str,
    right: &str,
    reason: &'static str,
) -> Result<()> {
    if conflicts.len() >= MAX_CONFLICTS {
        bail!("Structured project memory conflict report exceeds its bound")
    }
    conflicts.push(MemoryConflict {
        subject: subject.into(),
        predicate: predicate.into(),
        left_claim: left.into(),
        right_claim: right.into(),
        reason,
    });
    Ok(())
}

fn incompatible(left: &Constraint, right: &Constraint) -> bool {
    use ConstraintOperator::*;
    match (left.operator, right.operator) {
        (Equals, _) => satisfies(&left.value, right) == Some(false),
        (_, Equals) => satisfies(&right.value, left) == Some(false),
        (AtLeast, AtMost) => numeric_order(&left.value, &right.value) == Some(Ordering::Greater),
        (AtMost, AtLeast) => numeric_order(&right.value, &left.value) == Some(Ordering::Greater),
        (Contains, Excludes) => required_contains_excluded(&left.value, &right.value),
        (Excludes, Contains) => required_contains_excluded(&right.value, &left.value),
        _ => false,
    }
}

fn required_contains_excluded(required: &Value, excluded: &Value) -> bool {
    match (required.as_str(), excluded.as_str()) {
        (Some(required), Some(excluded)) => required.contains(excluded),
        _ => false,
    }
}

fn satisfies(value: &Value, constraint: &Constraint) -> Option<bool> {
    use ConstraintOperator::*;
    match constraint.operator {
        Equals | NotEquals => {
            let equal = if value.is_number() && constraint.value.is_number() {
                numeric_order(value, &constraint.value)? == Ordering::Equal
            } else {
                value == &constraint.value
            };
            Some(if constraint.operator == Equals {
                equal
            } else {
                !equal
            })
        }
        AtLeast => Some(numeric_order(value, &constraint.value)? != Ordering::Less),
        AtMost => Some(numeric_order(value, &constraint.value)? != Ordering::Greater),
        Contains => Some(value.as_str()?.contains(constraint.value.as_str()?)),
        Excludes => Some(!value.as_str()?.contains(constraint.value.as_str()?)),
    }
}

// Avoid rounding large integers through f64. Mixed integer/float comparisons are
// deliberately unknown rather than accepting a lossy proof of contradiction.
fn numeric_order(left: &Value, right: &Value) -> Option<Ordering> {
    let (left, right) = (left.as_number()?, right.as_number()?);
    let integer = |number: &serde_json::Number| {
        number
            .as_i64()
            .map(i128::from)
            .or_else(|| number.as_u64().map(i128::from))
    };
    match (integer(left), integer(right)) {
        (Some(left), Some(right)) => Some(left.cmp(&right)),
        (None, None) => left.as_f64()?.partial_cmp(&right.as_f64()?),
        _ => None,
    }
}
