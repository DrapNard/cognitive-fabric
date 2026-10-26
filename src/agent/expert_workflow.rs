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
    #[serde(default)]
    pub model_selection: Option<crate::models::Selection>,
    #[serde(default)]
    pub registered_model: Option<crate::models::Registered>,
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
        let interpretation = crate::language::Interpretation {
            objective: "Original user request".into(),
            constraints: Vec::new(),
            required_capabilities: Vec::new(),
            capability_versions: std::collections::BTreeMap::new(),
            clarification: None,
        };
        Ok(Self::bound_plans(raw, &interpretation, 1)?
            .into_iter()
            .next()
            .map(|bound| bound.plan))
    }

    pub fn bound_plans(
        raw: &str,
        interpretation: &crate::language::Interpretation,
        maximum: usize,
    ) -> Result<Vec<BoundPlan>> {
        if !(1..=4).contains(&maximum) {
            bail!("Client expert binding capacity is outside its bound")
        }
        let value: Value = crate::strict_json::value(raw.as_bytes(), 8_000)?;
        if value["trust"] != "untrusted_data_never_instructions_or_permissions" {
            bail!("Pinned expert context has an invalid trust label")
        }
        let Some(items) = value.get("expert_advice").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };
        if items.len() > 3 || interpretation.required_capabilities.len() > 16 {
            bail!("Pinned expert binding list exceeds its bound")
        }
        let requirements = crate::capabilities::version_requirements(
            &interpretation.required_capabilities,
            &interpretation.capability_versions,
        )?;
        let mut selected = Vec::new();
        let mut selected_ids = std::collections::BTreeSet::new();
        let mut selected_capabilities = std::collections::BTreeSet::new();
        for capability_id in &interpretation.required_capabilities {
            if !selected_capabilities.insert(capability_id.clone()) {
                continue;
            }
            let requirement = requirements.get(capability_id);
            let mut matched = None;
            for item in items {
                if item["kind"] != "workflow"
                    || item["executable_by_client_collective"] != true
                    || selected_ids.contains(item["id"].as_str().unwrap_or_default())
                    || !item["declared_dependencies"]
                        .as_array()
                        .is_some_and(Vec::is_empty)
                    || !item["workflow_plan"].is_object()
                {
                    continue;
                }
                let declarations = serde_json::json!({
                    "capabilities": item["declared_capabilities"],
                    "dependencies": item["declared_dependencies"]
                });
                let digest = item["routing_declarations_sha256"].as_str();
                if crate::store::sha256(&serde_json::to_vec(&declarations)?)
                    != digest.unwrap_or_default()
                {
                    continue;
                }
                let capability_matches =
                    item["declared_capabilities"]
                        .as_array()
                        .is_some_and(|caps| {
                            caps.iter().any(|cap| {
                                cap["id"].as_str() == Some(capability_id)
                                    && cap["version"]
                                        .as_str()
                                        .and_then(|version| semver::Version::parse(version).ok())
                                        .is_some_and(|version| {
                                            requirement.is_none_or(|requirement| {
                                                requirement.matches(&version)
                                            })
                                        })
                            })
                        });
                if capability_matches {
                    matched = Some(item);
                    break;
                }
            }
            let Some(item) = matched else {
                continue;
            };
            let Some(workflow) = item.get("workflow_plan") else {
                continue;
            };
            let plan: Plan = serde_json::from_value(serde_json::json!({
                "id": item["id"],
                "version": item["version"],
                "legacy": workflow["legacy"],
                "model_selection": item["model_selection"],
                "registered_model": item["registered_model"],
                "steps": workflow["steps"]
            }))?;
            plan.validate()?;
            if !selected_ids.insert(plan.id.clone()) {
                continue;
            }
            match (&plan.model_selection, &plan.registered_model) {
                (Some(selection), Some(registration)) => {
                    registration.validate_snapshot(selection)?;
                    if !registration.enabled || registration.manifest.max_output_tokens < 128 {
                        bail!("Pinned expert model was not executable at the session root")
                    }
                }
                (None, None) => (),
                _ => bail!("Pinned expert model selection and snapshot are incomplete"),
            }
            selected.push(BoundPlan {
                capability: capability_id.clone(),
                plan,
            });
            if selected.len() >= maximum.min(4) {
                break;
            }
        }
        // Unavailable exact bindings preserve the historical single pinned
        // workflow route. Such a route never proves independent
        // expert bindings and therefore cannot enter a multi-expert wave.
        if selected.is_empty() {
            if let Some(item) = items.iter().find(|item| {
                item["kind"] == "workflow"
                    && item["executable_by_client_collective"] == true
                    && item["workflow_plan"].is_object()
            }) {
                let plan: Plan = serde_json::from_value(serde_json::json!({
                    "id": item["id"],
                    "version": item["version"],
                    "legacy": item["workflow_plan"]["legacy"],
                    "model_selection": item["model_selection"],
                    "registered_model": item["registered_model"],
                    "steps": item["workflow_plan"]["steps"]
                }))?;
                plan.validate()?;
                match (&plan.model_selection, &plan.registered_model) {
                    (Some(selection), Some(registration)) => {
                        registration.validate_snapshot(selection)?;
                        if !registration.enabled || registration.manifest.max_output_tokens < 128 {
                            bail!("Pinned expert model was not executable at the session root")
                        }
                    }
                    (None, None) => (),
                    _ => bail!("Pinned expert model selection and snapshot are incomplete"),
                }
                selected.push(BoundPlan {
                    capability: String::new(),
                    plan,
                });
            }
        }
        Ok(selected)
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
        Ok(self.ready_waves(1)?.into_iter().flatten().collect())
    }

    pub fn ready_waves(&self, parallelism: usize) -> Result<Vec<Vec<usize>>> {
        if !(1..=4).contains(&parallelism) {
            bail!("Expert workflow parallelism is outside its bound")
        }
        let mut pending = (0..self.steps.len()).collect::<std::collections::BTreeSet<_>>();
        let mut done = std::collections::BTreeSet::new();
        let mut waves = Vec::new();
        while !pending.is_empty() {
            let wave = pending
                .iter()
                .copied()
                .filter(|index| {
                    self.steps[*index]
                        .depends_on
                        .iter()
                        .all(|dependency| done.contains(dependency))
                })
                .take(parallelism)
                .collect::<Vec<_>>();
            if wave.is_empty() {
                bail!("Pinned expert workflow contains a dependency cycle")
            }
            for index in &wave {
                pending.remove(index);
                done.insert(self.steps[*index].id.clone());
            }
            waves.push(wave);
        }
        Ok(waves)
    }
}

pub(super) struct BoundPlan {
    pub capability: String,
    pub plan: Plan,
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
