//! Bounded whole-goal candidate recurrence, reconstructed from durable role nodes.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(crate) struct History {
    candidates: Vec<(u32, String)>,
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl History {
    pub(crate) fn from_nodes(nodes: &[Node], cycles: u32) -> Result<Self> {
        if nodes.len() > 64 || cycles > 16 {
            bail!("Candidate history exceeds original reasoning bounds")
        }
        let mut first = BTreeMap::new();
        for node in nodes {
            if node.cycle > cycles {
                bail!("Candidate history exceeds reserved cycles")
            }
            if node.role == "solver" && node.cycle > 0 {
                if !valid_hash(&node.content_hash) {
                    bail!("Invalid candidate history hash")
                }
                // Recursive ablations can label critique/reflection as solver too.
                // Only the first solver of a reserved cycle is the whole-goal answer.
                first
                    .entry(node.cycle)
                    .or_insert_with(|| node.content_hash.clone());
            }
        }
        if first.len() != cycles as usize || first.keys().copied().ne(1..=cycles) {
            bail!("Missing whole-goal candidate history")
        }
        Ok(Self {
            candidates: first.into_iter().collect(),
        })
    }
    pub(crate) fn has_recurrence(&self) -> bool {
        self.candidates
            .iter()
            .map(|(_, hash)| hash)
            .collect::<BTreeSet<_>>()
            .len()
            != self.candidates.len()
    }
    fn observe(&mut self, cycle: u32, hash: &str) -> Result<Vec<u32>> {
        if cycle != self.candidates.len() as u32 + 1 || cycle > 16 || !valid_hash(hash) {
            bail!("Invalid whole-goal candidate transition")
        }
        let prior = self
            .candidates
            .iter()
            .filter(|(_, seen)| seen == hash)
            .map(|(cycle, _)| *cycle)
            .collect::<Vec<_>>();
        self.candidates.push((cycle, hash.into()));
        Ok(prior)
    }
}
impl Session {
    pub(super) fn candidate_progress(
        &self,
        history: &mut History,
        query: &str,
        hash: &str,
        independent_rejection: bool,
    ) -> Result<bool> {
        let previous = history.observe(self.resources.cycles, hash)?;
        self.store.event(&self.task,"candidate_progress","jev-r",json!({"format_version":1,"policy":"candidate-recurrence/v1","cycle":self.resources.cycles,"objective_hash":crate::store::sha256(query.as_bytes()),"answer_hash":hash,"previous_cycles":previous,"candidate_count":history.candidates.len(),"repeated":!previous.is_empty(),"verification_state":if independent_rejection {"rejected"} else {"unverified"},"verified":false,"permissions_granted":false}))?;
        Ok(previous.is_empty())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: usize, cycle: u32, role: &str, text: &str) -> Node {
        Node {
            id,
            parent: id.checked_sub(1),
            role: role.into(),
            cycle,
            model: "fixture".into(),
            content_hash: crate::store::sha256(text.as_bytes()),
        }
    }
    #[test]
    fn history_ignores_advice_and_same_cycle_critic_calls_but_detects_nonadjacent_answers() {
        let nodes = vec![
            node(0, 0, "solver", "A"),
            node(1, 1, "solver", "A"),
            node(2, 1, "solver", "B"),
            node(3, 1, "solver", "A"),
            node(4, 2, "solver", "B"),
        ];
        let mut history = History::from_nodes(&nodes, 2).unwrap();
        assert!(!history.has_recurrence());
        assert_eq!(
            history.observe(3, &crate::store::sha256(b"A")).unwrap(),
            vec![1]
        );
        assert!(history.has_recurrence());
        assert!(history.observe(5, &crate::store::sha256(b"new")).is_err());
    }
    #[test]
    fn malformed_missing_excessive_and_future_cycle_histories_are_rejected() {
        assert!(History::from_nodes(&[], 0).is_ok());
        assert!(History::from_nodes(&[], 1).is_err());
        assert!(History::from_nodes(&[node(0, 2, "solver", "A")], 2).is_err());
        assert!(History::from_nodes(&[node(0, 1, "solver", "A")], 0).is_err());
        assert!(History::from_nodes(&[], 17).is_err());
        let mut invalid = node(0, 1, "solver", "A");
        invalid.content_hash = "forged".into();
        assert!(History::from_nodes(&[invalid], 1).is_err());
        assert!(History::from_nodes(
            &(0..65)
                .map(|i| node(i, 0, "planner", "data"))
                .collect::<Vec<_>>(),
            0
        )
        .is_err());
    }
}
