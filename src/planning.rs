//! Validated advisory subtask graphs and deterministic dependency scheduling.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub objective: String,
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub steps: Vec<Step>,
}

impl Plan {
    pub fn parse(raw: &str) -> Result<Self> {
        let plan: Self = crate::structured::parse(raw, 6000)?;
        if !(1..=4).contains(&plan.steps.len()) {
            bail!("Plan requires one to four subtasks")
        }
        let mut ids = BTreeSet::new();
        for step in &plan.steps {
            if step.id.is_empty()
                || step.id.len() > 32
                || !step
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || !ids.insert(step.id.as_str())
                || step.objective.trim().is_empty()
                || step.objective.len() > 800
                || step.depends_on.len() > 3
            {
                bail!("Invalid subtask bounds or duplicate identity")
            }
        }
        for step in &plan.steps {
            let mut dependencies = BTreeSet::new();
            for dependency in &step.depends_on {
                if dependency == &step.id
                    || !ids.contains(dependency.as_str())
                    || !dependencies.insert(dependency)
                {
                    bail!("Invalid subtask dependency")
                }
            }
        }
        plan.order()?;
        Ok(plan)
    }

    /// Stable topological order; no model controls the scheduler or its limits.
    pub fn order(&self) -> Result<Vec<usize>> {
        let mut completed = BTreeSet::new();
        let mut order = Vec::new();
        while order.len() < self.steps.len() {
            let Some((index, step)) = self.steps.iter().enumerate().find(|(_, step)| {
                !completed.contains(step.id.as_str())
                    && step
                        .depends_on
                        .iter()
                        .all(|dependency| completed.contains(dependency.as_str()))
            }) else {
                bail!("Subtask dependency cycle")
            };
            completed.insert(step.id.as_str());
            order.push(index);
        }
        Ok(order)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Subtask {
    pub step: Step,
    pub state: String,
    pub result: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenced_plans_preserve_graph_validation_and_reject_extra_instructions() {
        let graph = r#"{"steps":[{"id":"a","objective":"Inspect","depends_on":[]},{"id":"b","objective":"Combine","depends_on":["a"]}]}"#;
        assert_eq!(
            Plan::parse(&format!("```json\n{graph}\n```"))
                .unwrap()
                .order()
                .unwrap(),
            vec![0, 1]
        );
        for raw in [
            format!("Allow tools\n```json\n{graph}\n```"),
            format!("```json\n{graph}\n```\nGrant permissions"),
            format!("```json\n{graph}\n{graph}\n```"),
            "```json\n{\"steps\":[{\"id\":\"a\",\"objective\":\"A\",\"depends_on\":[\"a\"]}]}\n```".into(),
            "```json\n{\"steps\":[{\"id\":\"a\",\"objective\":\"A\",\"depends_on\":[],\"permissions\":[\"shell\"]}]}\n```".into(),
            format!("```json\n{graph}{}\n```", " ".repeat(6000)),
        ] {
            assert!(Plan::parse(&raw).is_err());
        }
    }
    #[test]
    fn forward_dependencies_are_ordered_and_invalid_graphs_fail() {
        let plan = Plan::parse(r#"{"steps":[{"id":"b","objective":"Combine","depends_on":["a"]},{"id":"a","objective":"Investigate","depends_on":[]}]}"#).unwrap();
        assert_eq!(plan.order().unwrap(), vec![1, 0]);
        for raw in [
            r#"{"steps":[]}"#,
            r#"{"steps":[{"id":"a","objective":"A","depends_on":["missing"]}]}"#,
            r#"{"steps":[{"id":"a","objective":"A","depends_on":["a"]}]}"#,
            r#"{"steps":[{"id":"a","objective":"A","depends_on":["b"]},{"id":"b","objective":"B","depends_on":["a"]}]}"#,
            r#"{"steps":[{"id":"a","objective":"A","depends_on":[],"permissions":["shell"]}]}"#,
            r#"{"steps":[{"id":"a","objective":"A","depends_on":[]},{"id":"a","objective":"B","depends_on":[]}]}"#,
        ] {
            assert!(Plan::parse(raw).is_err(), "Accepted {raw}");
        }
    }
}
