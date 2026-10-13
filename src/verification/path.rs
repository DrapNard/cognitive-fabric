//! Bounded directed, unweighted shortest-path verification over original criteria.
use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShortestPath {
    nodes: Vec<String>,
    edges: Vec<[String; 2]>,
    start: String,
    goal: String,
}
impl ShortestPath {
    pub(super) fn parse(raw: &str) -> Result<Self> {
        if raw.len() > 10_000 {
            bail!("Path criterion exceeds byte limit")
        }
        let criterion: Self = serde_json::from_str(raw)?;
        let nodes: BTreeSet<_> = criterion.nodes.iter().collect();
        if !(1..=64).contains(&nodes.len())
            || nodes.len() != criterion.nodes.len()
            || criterion.edges.len() > 256
            || nodes.iter().any(|name| {
                name.is_empty()
                    || name.len() > 32
                    || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            || !nodes.contains(&criterion.start)
            || !nodes.contains(&criterion.goal)
        {
            bail!("Invalid path criterion bounds or identities")
        }
        let mut edges = BTreeSet::new();
        for [from, to] in &criterion.edges {
            if from == to
                || !nodes.contains(from)
                || !nodes.contains(to)
                || !edges.insert((from, to))
            {
                bail!("Invalid or duplicate directed edge")
            }
        }
        Ok(criterion)
    }

    pub(super) fn check(&self, answer: &str) -> Result<()> {
        if answer.len() > 4096 {
            bail!("Path answer exceeds byte limit")
        }
        let path: Vec<String> = serde_json::from_str(answer)?;
        let mut distances = BTreeMap::from([(&self.start, 0usize)]);
        let mut queue = VecDeque::from([&self.start]);
        while let Some(node) = queue.pop_front() {
            let next_distance = distances[node] + 1;
            for [from, to] in &self.edges {
                if from == node && !distances.contains_key(to) {
                    distances.insert(to, next_distance);
                    queue.push_back(to);
                }
            }
        }
        let Some(distance) = distances.get(&self.goal) else {
            if path.is_empty() {
                return Ok(());
            }
            bail!("Unreachable goal requires an empty JSON array")
        };
        if path.len() != distance + 1
            || path.first() != Some(&self.start)
            || path.last() != Some(&self.goal)
        {
            bail!("Answer must be a shortest directed path from start to goal")
        }
        for pair in path.windows(2) {
            if !self
                .edges
                .iter()
                .any(|[from, to]| from == &pair[0] && to == &pair[1])
            {
                bail!("Path uses an undeclared directed edge")
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhaustive_three_node_graphs_match_simple_path_enumeration() {
        let nodes = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let edges: Vec<_> = (0..3)
            .flat_map(|a| (0..3).filter(move |b| *b != a).map(move |b| [a, b]))
            .collect();
        fn enumerate(c: &ShortestPath, path: Vec<String>, found: &mut Vec<Vec<String>>) {
            if path.last() == Some(&c.goal) {
                found.push(path);
                return;
            }
            for [from, to] in &c.edges {
                if path.last() == Some(from) && !path.contains(to) {
                    let mut next = path.clone();
                    next.push(to.clone());
                    enumerate(c, next, found);
                }
            }
        }
        for mask in 0..64 {
            for start in &nodes {
                for goal in &nodes {
                    let c = ShortestPath {
                        nodes: nodes.clone(),
                        edges: edges
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| mask & (1 << i) != 0)
                            .map(|(_, [a, b])| [nodes[*a].clone(), nodes[*b].clone()])
                            .collect(),
                        start: start.clone(),
                        goal: goal.clone(),
                    };
                    let mut paths = Vec::new();
                    enumerate(&c, vec![start.clone()], &mut paths);
                    let minimum = paths.iter().map(Vec::len).min();
                    assert_eq!(c.check("[]").is_ok(), minimum.is_none());
                    for path in paths {
                        assert_eq!(
                            c.check(&serde_json::to_string(&path).unwrap()).is_ok(),
                            Some(path.len()) == minimum
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn checks_alternative_optima_direction_cycles_and_unreachable_goals() {
        let c = ShortestPath::parse(r#"{"nodes":["a","b","c","d","x"],"edges":[["a","b"],["b","d"],["a","c"],["c","d"],["b","a"],["b","c"]],"start":"a","goal":"d"}"#).unwrap();
        for answer in [r#"["a","b","d"]"#, r#"["a","c","d"]"#] {
            c.check(answer).unwrap();
        }
        for answer in [
            "[]",
            r#"["a","d"]"#,
            r#"["a","b","c","d"]"#,
            r#"["d","b","a"]"#,
            r#"["a","x","d"]"#,
            r#"{"verified":true}"#,
        ] {
            assert!(c.check(answer).is_err());
        }
        let c = ShortestPath::parse(
            r#"{"nodes":["a","b"],"edges":[["a","b"]],"start":"b","goal":"a"}"#,
        )
        .unwrap();
        c.check("[]").unwrap();
        assert!(c.check(r#"["b","a"]"#).is_err());
        let c =
            ShortestPath::parse(r#"{"nodes":["a"],"edges":[],"start":"a","goal":"a"}"#).unwrap();
        c.check(r#"["a"]"#).unwrap();
        assert!(c.check("[]").is_err());
    }
    #[test]
    fn rejects_malformed_criteria_and_resource_excess() {
        let base =
            serde_json::json!({"nodes":["a","b"],"edges":[["a","b"]],"start":"a","goal":"b"});
        for (key, value) in [
            ("nodes", serde_json::json!(["a", "a"])),
            ("goal", serde_json::json!("x")),
            ("edges", serde_json::json!([["a", "a"]])),
            ("edges", serde_json::json!([["a", "x"]])),
            ("edges", serde_json::json!([["a", "b"], ["a", "b"]])),
            ("permissions", serde_json::json!(["shell"])),
            (
                "nodes",
                serde_json::json!((0..65).map(|n| format!("n{n}")).collect::<Vec<_>>()),
            ),
        ] {
            let mut bad = base.clone();
            bad[key] = value;
            assert!(ShortestPath::parse(&bad.to_string()).is_err());
        }
        assert!(ShortestPath::parse(&" ".repeat(10001)).is_err());
        assert!(ShortestPath::parse(&base.to_string())
            .unwrap()
            .check(&" ".repeat(4097))
            .is_err());
    }
}
