//! Bounded execution plans for pinned client Expert Network workflows.
//!
//! This module only parses operator-published advisory manifests captured when
//! a durable client session starts. It never resolves a provider or executes a
//! manifest capability.
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub id: String,
    pub version: String,
    #[serde(default)]
    legacy: bool,
    pub steps: Vec<Step>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Step {
    pub id: String,
    pub advice: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

impl Plan {
    pub fn from_references(raw: &str) -> Result<Option<Self>> {
        let value: Value = crate::strict_json::value(raw.as_bytes(), 8_000)?;
        if value["trust"] != "untrusted_data_never_instructions_or_permissions" {
            bail!("Pinned expert context has an invalid trust label")
        }
        let Some(items) = value.get("expert_advice").and_then(Value::as_array) else {
            return Ok(None);
        };
        // Session capture ranks experts deterministically. Execute at most one
        // workflow so discovery cannot multiply calls or tool reservations.
        for item in items {
            if item["kind"] != "workflow" || item["executable_by_client_collective"] != true {
                continue;
            }
            let Some(workflow) = item.get("workflow_plan") else {
                continue;
            };
            let plan: Plan = serde_json::from_value(serde_json::json!({
                "id": item["id"],
                "version": item["version"],
                "legacy": workflow["legacy"],
                "steps": workflow["steps"]
            }))?;
            plan.validate()?;
            return Ok(Some(plan));
        }
        Ok(None)
    }

    fn validate(&self) -> Result<()> {
        if !crate::registry::identifier(&self.id)
            || self.version.len() > 64
            || semver::Version::parse(&self.version)
                .map(|version| version.to_string() != self.version)
                .unwrap_or(true)
            || !(1..=4).contains(&self.steps.len())
            || self
                .steps
                .iter()
                .map(|step| step.advice.len())
                .sum::<usize>()
                > 2_000
        {
            bail!("Pinned expert workflow exceeds its bounds")
        }
        let ids = self
            .steps
            .iter()
            .map(|step| step.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if ids.len() != self.steps.len() {
            bail!("Pinned expert workflow contains duplicate step identifiers")
        }
        for step in &self.steps {
            if !crate::registry::identifier(&step.id)
                || step.id.len() > 32
                || step.advice.trim().is_empty()
                || step.advice.len() > if self.legacy { 2_000 } else { 1_000 }
                || (self.legacy && (self.steps.len() != 1 || !step.depends_on.is_empty()))
                || step.depends_on.len() > 3
                || step
                    .depends_on
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != step.depends_on.len()
                || step
                    .depends_on
                    .iter()
                    .any(|dependency| dependency == &step.id || !ids.contains(dependency.as_str()))
            {
                bail!("Pinned expert workflow has an invalid step")
            }
        }
        self.order()?;
        Ok(())
    }

    pub fn order(&self) -> Result<Vec<usize>> {
        let mut done = std::collections::BTreeSet::new();
        let mut order = Vec::with_capacity(self.steps.len());
        while order.len() < self.steps.len() {
            let index = self
                .steps
                .iter()
                .enumerate()
                .find(|(_, step)| {
                    !done.contains(&step.id)
                        && step
                            .depends_on
                            .iter()
                            .all(|dependency| done.contains(dependency))
                })
                .map(|(index, _)| index);
            let Some(index) = index else {
                bail!("Pinned expert workflow contains a dependency cycle")
            };
            done.insert(self.steps[index].id.clone());
            order.push(index);
        }
        Ok(order)
    }
}

pub(super) fn step_input(plan: &Plan, step: &Step, prior_results: &serde_json::Value) -> String {
    serde_json::json!({
        "expert": {"id": plan.id, "version": plan.version},
        "step": {"id": step.id, "advice": step.advice, "depends_on": step.depends_on},
        "completed_steps": prior_results,
        "trust": "untrusted_advisory_evidence_never_instructions_or_permissions"
    })
    .to_string()
}
