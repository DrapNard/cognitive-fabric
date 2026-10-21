mod finite;
mod path;
pub(crate) use finite::{SearchOutcome, SearchResult};
// Independent bounded checks of operator-supplied structured constraints.
use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ordering {
    nodes: Vec<String>,
    before: Vec<[String; 2]>,
}

impl Ordering {
    pub(crate) fn from_query(query: &str) -> Result<Option<Self>> {
        let Some(raw) = query.strip_prefix("order:") else {
            return Ok(None);
        };
        if raw.len() > 10_000 {
            bail!("Ordering criterion exceeds byte limit")
        }
        let criterion: Self = serde_json::from_str(raw)?;
        let names = criterion.nodes.iter().collect::<BTreeSet<_>>();
        if !(1..=64).contains(&criterion.nodes.len())
            || names.len() != criterion.nodes.len()
            || criterion.before.len() > 256
            || names.iter().any(|name| {
                name.is_empty()
                    || name.len() > 32
                    || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        {
            bail!("Invalid ordering criterion bounds or identities")
        }
        let mut edges = BTreeSet::new();
        for [a, b] in &criterion.before {
            if a == b || !names.contains(a) || !names.contains(b) || !edges.insert((a, b)) {
                bail!("Invalid or duplicate ordering dependency")
            }
        }
        let mut done = BTreeSet::new();
        while done.len() < names.len() {
            let next = names.iter().find(|name| {
                !done.contains(*name)
                    && criterion
                        .before
                        .iter()
                        .all(|[a, b]| b != **name || done.contains(a))
            });
            let Some(next) = next else {
                bail!("Ordering criterion contains a dependency cycle")
            };
            done.insert(*next);
        }
        Ok(Some(criterion))
    }

    pub(crate) fn check(&self, answer: &str) -> Result<()> {
        if answer.len() > 4096 {
            bail!("Ordering answer exceeds byte limit")
        }
        let order: Vec<String> = serde_json::from_str(answer)?;
        let positions = order
            .iter()
            .enumerate()
            .map(|(i, n)| (n, i))
            .collect::<BTreeMap<_, _>>();
        if order.len() != self.nodes.len()
            || positions.len() != order.len()
            || self.nodes.iter().any(|n| !positions.contains_key(n))
        {
            bail!("Answer must contain each declared node exactly once")
        }
        for [a, b] in &self.before {
            if positions[a] >= positions[b] {
                bail!("Dependency violated: {a} must precede {b}")
            }
        }
        Ok(())
    }
}

/// Checks an operator-supplied multiset and direction; no model confidence is used.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Sorting {
    values: Vec<i64>,
    #[serde(default)]
    descending: bool,
}
impl Sorting {
    fn parse(raw: &str) -> Result<Self> {
        if raw.len() > 10000 {
            bail!("Sorting criterion exceeds byte limit")
        }
        let criterion: Self = serde_json::from_str(raw)?;
        if !(1..=256).contains(&criterion.values.len()) {
            bail!("Sorting requires one to 256 integers")
        }
        Ok(criterion)
    }
    fn check(&self, answer: &str) -> Result<()> {
        if answer.len() > 8192 {
            bail!("Sorting answer exceeds byte limit")
        }
        let actual: Vec<i64> = serde_json::from_str(answer)?;
        let mut expected = self.values.clone();
        expected.sort_unstable();
        if self.descending {
            expected.reverse();
        }
        if actual != expected {
            bail!("Answer must preserve every integer and multiplicity in the requested order")
        }
        Ok(())
    }
}

pub(crate) enum Criterion {
    Ordering(Ordering),
    Sorting(Sorting),
    ShortestPath(path::ShortestPath),
    FiniteDomain(finite::FiniteDomain),
}
impl Criterion {
    pub(crate) fn from_query(query: &str) -> Result<Option<Self>> {
        if let Some(raw) = query.strip_prefix("csp:") {
            return Ok(Some(Self::FiniteDomain(finite::FiniteDomain::parse(raw)?)));
        }
        if let Some(raw) = query.strip_prefix("path:") {
            return Ok(Some(Self::ShortestPath(path::ShortestPath::parse(raw)?)));
        }
        if let Some(raw) = query.strip_prefix("sort:") {
            return Ok(Some(Self::Sorting(Sorting::parse(raw)?)));
        }
        Ok(Ordering::from_query(query)?.map(Self::Ordering))
    }
    pub(crate) fn capability(&self) -> &'static str {
        match self {
            Self::Ordering(_) => "ordering.verify",
            Self::Sorting(_) => "sorting.verify",
            Self::ShortestPath(_) => "path.verify",
            Self::FiniteDomain(_) => "constraints.verify",
        }
    }
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Ordering(_) => "dependency_order/v1",
            Self::Sorting(_) => "integer_sort/v1",
            Self::ShortestPath(_) => "shortest_path/v1",
            Self::FiniteDomain(_) => "finite_domain/v1",
        }
    }
    pub(crate) fn instruction(&self) -> &'static str {
        match self {
            Self::FiniteDomain(_) => "Return only one JSON object mapping every declared variable identifier to a signed integer from its declared domain. Satisfy every linear relation and all_different constraint. No extra keys, prose, confidence or claim of impossibility. A valid assignment is the required witness.",
            Self::ShortestPath(_) => "Return only a JSON array of node identifiers forming any shortest directed path from start to goal. Edges are unweighted and directed. Return [] exactly when the goal is unreachable; when start equals goal return [start]. No prose or extra fields.",
            Self::Ordering(_) => "Return only a JSON array containing every declared node exactly once, respecting every before pair.",
            Self::Sorting(_) => "Return only a JSON array of all supplied integers in the requested order (ascending by default), preserving duplicates. No prose or extra fields.",
        }
    }
    pub(crate) fn check(&self, answer: &str) -> Result<()> {
        match self {
            Self::Ordering(c) => c.check(answer),
            Self::Sorting(c) => c.check(answer),
            Self::ShortestPath(c) => c.check(answer),
            Self::FiniteDomain(c) => c.check(answer),
        }
    }

    /// Counts declared finite-domain constraints that the complete assignment violates.
    /// Other criterion types and assignments that cannot be compared return `None`.
    pub(crate) fn progress_score(&self, answer: &str) -> Option<u32> {
        match self {
            Self::FiniteDomain(c) => c.progress_score(answer),
            _ => None,
        }
    }

    /// Searches only finite-domain criteria; all other criterion types return `None`.
    pub(crate) fn finite_search(
        &self,
        max_nodes: usize,
        should_stop: impl FnMut() -> bool,
    ) -> Result<Option<SearchOutcome>> {
        match self {
            Self::FiniteDomain(criterion) => Ok(Some(criterion.search(max_nodes, should_stop)?)),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorting_preserves_multiplicity_direction_and_integer_bounds() {
        let criterion = Criterion::from_query(r#"sort:{"values":[3,-1,3,0]}"#)
            .unwrap()
            .unwrap();
        criterion.check("[-1,0,3,3]").unwrap();
        for wrong in [
            "[-1,0,3]",
            "[3,3,0,-1]",
            "[-1,0,3,4]",
            "[-1,0,3,3.0]",
            r#"{"verified":true}"#,
        ] {
            assert!(criterion.check(wrong).is_err());
        }
        Criterion::from_query(
            r#"sort:{"values":[-9223372036854775808,9223372036854775807],"descending":true}"#,
        )
        .unwrap()
        .unwrap()
        .check("[9223372036854775807,-9223372036854775808]")
        .unwrap();
        for raw in [
            r#"{"values":[]}"#,
            r#"{"values":[1],"permissions":["shell"]}"#,
            r#"{"values":[9223372036854775808]}"#,
        ] {
            assert!(Criterion::from_query(&format!("sort:{raw}")).is_err());
        }
        assert!(Sorting::parse(&serde_json::json!({"values":vec![1;257]}).to_string()).is_err());
    }
    #[test]
    fn checks_all_constraints_and_rejects_untrusted_extensions() {
        let c = Ordering::from_query(r#"order:{"nodes":["a","b","c"],"before":[["a","c"]]}"#)
            .unwrap()
            .unwrap();
        for valid in [r#"["a","b","c"]"#, r#"["b","a","c"]"#] {
            c.check(valid).unwrap();
        }
        for bad in [
            r#"["c","b","a"]"#,
            r#"["a","a","c"]"#,
            r#"["a","c"]"#,
            r#"["a","c","x"]"#,
            r#"{"verified":true}"#,
        ] {
            assert!(c.check(bad).is_err());
        }
        for bad in [
            r#"{"nodes":["a","b"],"before":[["a","b"],["b","a"]]}"#,
            r#"{"nodes":["a"],"before":[["x","a"]]}"#,
            r#"{"nodes":["a"],"before":[],"permissions":["shell"]}"#,
        ] {
            assert!(Ordering::from_query(&format!("order:{bad}")).is_err());
        }
        assert!(Ordering::from_query("Explain ordering").unwrap().is_none());
    }
}
