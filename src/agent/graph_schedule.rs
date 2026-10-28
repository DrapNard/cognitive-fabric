//! Host-owned dependency readiness shared by typed and advisory execution.
//! This module selects work; it never grants permissions or reserves resources.
use anyhow::{bail, Result};
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Evidence {
    Advisory,
    Checked,
}

pub(super) struct Dependency {
    pub node: usize,
    pub evidence: Evidence,
}

pub(super) struct Node {
    pub group: usize,
    pub dependencies: Vec<Dependency>,
    /// Already-started work occupies capacity and cannot be selected again.
    pub in_flight: bool,
    /// Waiting on host-owned external prerequisites without occupying capacity.
    pub blocked: bool,
    /// Host assertion after successful result admission, never model-provided.
    /// Checked means the operation contract passed, not natural-goal verification.
    pub completed: Option<Evidence>,
}

/// Select a stable round-robin ready wave. Pending nodes never satisfy edges in
/// the same wave. Checked evidence can feed advisory work; advice cannot satisfy
/// a checked edge. In-flight work consumes the supplied total/group limits.
/// Callers retain route, budget, deadline and output validation.
pub(super) fn ready_wave(
    nodes: &[Node],
    group_limit: usize,
    per_group_limit: usize,
    global_limit: usize,
) -> Result<Vec<usize>> {
    if nodes.is_empty()
        || nodes.len() > 16
        || !(1..=4).contains(&group_limit)
        || !(1..=4).contains(&per_group_limit)
        || !(1..=4).contains(&global_limit)
    {
        bail!("Shared graph scheduling exceeds its node or concurrency limits")
    }
    for (index, node) in nodes.iter().enumerate() {
        if node.group >= 16
            || node.dependencies.len() > 15
            || (node.in_flight && node.completed.is_some())
        {
            bail!("Shared graph group or dependency list exceeds its bound")
        }
        let mut seen = BTreeSet::new();
        for dependency in &node.dependencies {
            if dependency.node >= nodes.len()
                || dependency.node == index
                || !seen.insert(dependency.node)
            {
                bail!("Shared graph contains an invalid or duplicate dependency")
            }
        }
    }
    // Validate the entire graph, including completed nodes, before scheduling.
    let mut visited = BTreeSet::new();
    while visited.len() < nodes.len() {
        let previous = visited.len();
        for (index, node) in nodes.iter().enumerate() {
            if node
                .dependencies
                .iter()
                .all(|edge| visited.contains(&edge.node))
            {
                visited.insert(index);
            }
        }
        if visited.len() == previous {
            bail!("Shared graph contains a dependency cycle")
        }
    }
    let groups = nodes.iter().map(|node| node.group).collect::<BTreeSet<_>>();
    let mut selected = Vec::new();
    let mut counts = [0usize; 16];
    let mut running = 0;
    for node in nodes.iter().filter(|node| node.in_flight) {
        running += 1;
        counts[node.group] += 1;
    }
    let mut active_groups = counts.iter().filter(|count| **count != 0).count();
    if running > global_limit
        || active_groups > group_limit
        || counts.iter().any(|count| *count > per_group_limit)
    {
        bail!("In-flight graph work exceeds its admitted concurrency limits")
    }
    while selected.len() + running < global_limit {
        let previous = selected.len();
        for &group in &groups {
            if selected.len() + running == global_limit
                || counts[group] >= per_group_limit
                || (counts[group] == 0 && active_groups >= group_limit)
            {
                continue;
            }
            let candidate = nodes.iter().enumerate().find(|(index, node)| {
                node.group == group
                    && node.completed.is_none()
                    && !node.in_flight
                    && !node.blocked
                    && !selected.contains(index)
                    && node
                        .dependencies
                        .iter()
                        .all(|edge| match nodes[edge.node].completed {
                            Some(Evidence::Checked) => true,
                            Some(Evidence::Advisory) => edge.evidence == Evidence::Advisory,
                            None => false,
                        })
            });
            if let Some((index, _)) = candidate {
                if counts[group] == 0 {
                    active_groups += 1;
                }
                counts[group] += 1;
                selected.push(index);
            }
        }
        if selected.len() == previous {
            break;
        }
    }
    Ok(selected)
}
