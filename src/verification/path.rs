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

    pub(super) fn deterministic_answer(&self) -> Result<String> {
        if self.start == self.goal {
            let answer = serde_json::to_string(&[self.start.as_str()])?;
            if answer.len() > 4096 {
                bail!("Path answer exceeds byte limit")
            }
            return Ok(answer);
        }

        let mut outgoing = self
            .nodes
            .iter()
            .map(|node| (node.as_str(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        for [from, to] in &self.edges {
            let Some(neighbors) = outgoing.get_mut(from.as_str()) else {
                bail!("Path criterion references an unknown node")
            };
            neighbors.push(to.as_str());
        }
        for neighbors in outgoing.values_mut() {
            neighbors.sort_unstable();
        }

        let start = self.start.as_str();
        let goal = self.goal.as_str();
        let mut visited = BTreeSet::from([start]);
        let mut predecessor: BTreeMap<&str, &str> = BTreeMap::new();
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            let Some(neighbors) = outgoing.get(node) else {
                bail!("Path criterion references an unknown node")
            };
            for neighbor in neighbors {
                if visited.insert(*neighbor) {
                    predecessor.insert(*neighbor, node);
                    if *neighbor == goal {
                        let mut path = vec![goal];
                        let mut current = goal;
                        while current != start {
                            let Some(previous) = predecessor.get(current) else {
                                bail!("Path predecessor chain is incomplete")
                            };
                            current = *previous;
                            path.push(current);
                        }
                        path.reverse();
                        let answer = serde_json::to_string(&path)?;
                        if answer.len() > 4096 {
                            bail!("Path answer exceeds byte limit")
                        }
                        return Ok(answer);
                    }
                    queue.push_back(*neighbor);
                }
            }
        }

        Ok("[]".to_owned())
    }

    /// Return a bounded, criterion-derived structural/length deficit. This is
    /// a stopping hint only; `check` remains the sole acceptance test.
    pub(super) fn progress_score(&self, answer: &str) -> Option<u32> {
        if answer.len() > 4096 {
            return None;
        }
        let path: Vec<String> = serde_json::from_str(answer).ok()?;
        if path.len() > 64
            || path.iter().any(|node| {
                node.is_empty()
                    || node.len() > 32
                    || !node
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    || !self.nodes.iter().any(|known| known == node)
            })
        {
            return None;
        }

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
            // An unreachable goal has one valid answer: the empty path. A
            // nonempty sequence receives a fixed required-empty violation; it
            // does not claim a path-distance measurement.
            return Some(if path.is_empty() { 0 } else { 1 });
        };
        let expected_length = distance.saturating_add(1);
        let endpoint_mismatches = (if path.first() == Some(&self.start) {
            0
        } else {
            1
        }) + (if path.last() == Some(&self.goal) {
            0
        } else {
            1
        });
        let invalid_edges = path
            .windows(2)
            .filter(|pair| {
                !self
                    .edges
                    .iter()
                    .any(|[from, to]| from == &pair[0] && to == &pair[1])
            })
            .count();
        let length_delta = path.len().abs_diff(expected_length);

        // With at most 64 node IDs, the length term is at most 64. A 65-point
        // structural penalty therefore always outweighs any length difference.
        Some(((endpoint_mismatches + invalid_edges) * 65 + length_delta) as u32)
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

    #[test]
    fn progress_score_zero_is_exactly_compiled_acceptance_for_bounded_candidates() {
        let nodes = ["a", "b", "c"];
        let directed_edges: Vec<_> = (0..3)
            .flat_map(|from| {
                (0..3)
                    .filter(move |to| *to != from)
                    .map(move |to| [from, to])
            })
            .collect();
        for edge_mask in 0..(1 << directed_edges.len()) {
            let edges: Vec<[String; 2]> = directed_edges
                .iter()
                .enumerate()
                .filter(|(index, _)| edge_mask & (1 << index) != 0)
                .map(|(_, [from, to])| [nodes[*from].to_owned(), nodes[*to].to_owned()])
                .collect();
            for start in nodes {
                for goal in nodes {
                    let criterion = ShortestPath {
                        nodes: nodes.iter().map(|node| (*node).to_owned()).collect(),
                        edges: edges.clone(),
                        start: start.to_owned(),
                        goal: goal.to_owned(),
                    };
                    for length in 0..=4 {
                        let candidate_count = 3usize.pow(length as u32);
                        for mut index in 0..candidate_count {
                            let mut path = Vec::with_capacity(length);
                            for _ in 0..length {
                                path.push(nodes[index % nodes.len()].to_owned());
                                index /= nodes.len();
                            }
                            let answer = serde_json::to_string(&path).unwrap();
                            assert_eq!(
                                criterion.progress_score(&answer) == Some(0),
                                criterion.check(&answer).is_ok(),
                                "graph mask={edge_mask}, start={start}, goal={goal}, answer={answer}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn structural_penalties_dominate_all_bounded_length_differences() {
        let criterion = ShortestPath::parse(
            r#"{"nodes":["a","b","c"],"edges":[["a","b"],["b","a"]],"start":"a","goal":"b"}"#,
        )
        .unwrap();
        let longest = serde_json::to_string(
            &(0..64)
                .map(|index| if index % 2 == 0 { "a" } else { "b" })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let longest_score = criterion.progress_score(&longest).unwrap();
        assert!(longest_score <= 64);

        let structural_score = criterion.progress_score(r#"["b","a"]"#).unwrap();
        assert!(structural_score >= 65);
        assert!(criterion.progress_score(r#"["unknown"]"#).is_none());
        assert!(criterion
            .progress_score(&serde_json::to_string(&vec!["a"; 65]).unwrap())
            .is_none());
        assert!(criterion.progress_score("not json").is_none());
    }
}
