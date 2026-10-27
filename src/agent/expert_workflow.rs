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
    pub fn bound_plans(
        raw: &str,
        interpretation: &crate::language::Interpretation,
        maximum: usize,
    ) -> Result<PlanSelection> {
        if !(1..=4).contains(&maximum) {
            bail!("Client expert binding capacity is outside its bound")
        }
        let value: Value = crate::strict_json::value(raw.as_bytes(), 8_000)?;
        if value["trust"] != "untrusted_data_never_instructions_or_permissions" {
            bail!("Pinned expert context has an invalid trust label")
        }
        let Some(items) = value.get("expert_advice").and_then(Value::as_array) else {
            return Ok(PlanSelection {
                plans: Vec::new(),
                skipped: Vec::new(),
            });
        };
        if items.len() > 3 || interpretation.required_capabilities.len() > 16 {
            bail!("Pinned expert binding list exceeds its bound")
        }
        let requirements = crate::capabilities::version_requirements(
            &interpretation.required_capabilities,
            &interpretation.capability_versions,
        )?;
        let entries = items
            .iter()
            .map(PinnedEntry::parse)
            .collect::<Result<Vec<_>>>()?;
        if entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != entries.len()
        {
            bail!("Pinned expert context contains duplicate manifest IDs")
        }
        let mut selected = Vec::new();
        let mut skipped = Vec::new();
        let mut selected_ids = std::collections::BTreeSet::new();
        let mut selected_capabilities = std::collections::BTreeSet::new();
        for capability_id in &interpretation.required_capabilities {
            if !selected_capabilities.insert(capability_id.clone()) {
                continue;
            }
            let requirement = requirements.get(capability_id);
            let mut matched_closure = None;
            for root in entries.iter().filter(|entry| {
                !selected_ids.contains(&entry.id)
                    && entry.executable
                    && entry.metadata_pinned
                    && entry.capabilities.iter().any(|cap| {
                        cap.id == capability_id.as_str()
                            && requirement.is_none_or(|requirement| {
                                semver::Version::parse(&cap.version)
                                    .is_ok_and(|version| requirement.matches(&version))
                            })
                    })
            }) {
                if let Some(closure) = dependency_closure(root, &entries) {
                    matched_closure = Some((root.id.clone(), closure));
                    break;
                }
                skipped.push(SkippedRoot {
                    expert_id: root.id.clone(),
                    requested_capability: capability_id.clone(),
                    reason: "incomplete_or_unsupported_pinned_dependency_closure",
                });
            }
            let Some((root_id, closure)) = matched_closure else {
                // An incomplete or unsupported closure is not permission to run
                // its root alone. Try another exact provider for this capability.
                continue;
            };
            if selected_ids.len()
                + closure
                    .iter()
                    .filter(|entry| !selected_ids.contains(&entry.id))
                    .count()
                > 3
            {
                skipped.push(SkippedRoot {
                    expert_id: root_id,
                    requested_capability: capability_id.clone(),
                    reason: "dependency_union_exceeds_pinned_expert_bound",
                });
                continue;
            }
            for entry in closure {
                if selected_ids.insert(entry.id.clone()) {
                    let Some(plan) = entry.plan.clone() else {
                        bail!("Executable dependency closure has no validated workflow")
                    };
                    selected.push(BoundPlan {
                        capability: if entry.id == root_id {
                            capability_id.clone()
                        } else {
                            String::new()
                        },
                        dependencies: entry
                            .dependencies
                            .iter()
                            .map(|dependency| dependency.id.clone())
                            .collect(),
                        plan,
                    });
                }
            }
            if selected
                .iter()
                .filter(|bound| !bound.capability.is_empty())
                .count()
                >= maximum.min(4)
            {
                break;
            }
        }
        // Unavailable exact bindings preserve the historical single pinned
        // workflow route. Such a route never proves independent
        // expert bindings and therefore cannot enter a multi-expert wave.
        if selected.is_empty() {
            if interpretation.required_capabilities.is_empty() {
                for root in entries.iter().filter(|entry| entry.executable) {
                    if root.metadata_pinned {
                        if let Some(closure) = dependency_closure(root, &entries) {
                            for entry in closure {
                                let Some(plan) = entry.plan.clone() else {
                                    bail!("Executable dependency closure has no validated workflow")
                                };
                                selected.push(BoundPlan {
                                    capability: String::new(),
                                    dependencies: entry
                                        .dependencies
                                        .iter()
                                        .map(|dependency| dependency.id.clone())
                                        .collect(),
                                    plan,
                                });
                            }
                        } else {
                            skipped.push(SkippedRoot {
                                expert_id: root.id.clone(),
                                requested_capability: String::new(),
                                reason: "incomplete_or_unsupported_pinned_dependency_closure",
                            });
                        }
                    } else if root.dependencies.is_empty() {
                        let Some(plan) = root.plan.clone() else {
                            bail!("Executable fallback has no validated workflow")
                        };
                        selected.push(BoundPlan {
                            capability: String::new(),
                            dependencies: Vec::new(),
                            plan,
                        });
                    }
                    if !selected.is_empty() {
                        break;
                    }
                }
            } else if let Some(entry) = entries
                .iter()
                .find(|entry| entry.executable && entry.dependencies.is_empty())
            {
                let Some(plan) = entry.plan.clone() else {
                    bail!("Executable fallback has no validated workflow")
                };
                selected.push(BoundPlan {
                    capability: String::new(),
                    dependencies: Vec::new(),
                    plan,
                });
            }
        }
        Ok(PlanSelection {
            plans: selected,
            skipped,
        })
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

#[derive(Clone)]
pub(super) struct BoundPlan {
    pub capability: String,
    /// Direct, exact manifest IDs required before this workflow can run.
    pub dependencies: Vec<String>,
    pub plan: Plan,
}

pub(super) struct PlanSelection {
    pub plans: Vec<BoundPlan>,
    pub skipped: Vec<SkippedRoot>,
}

#[derive(serde::Serialize)]
pub(super) struct SkippedRoot {
    expert_id: String,
    requested_capability: String,
    reason: &'static str,
}

struct PinnedEntry {
    id: String,
    version: String,
    plan: Option<Plan>,
    capabilities: Vec<crate::registry::Capability>,
    dependencies: Vec<crate::registry::Dependency>,
    metadata_pinned: bool,
    executable: bool,
}

impl PinnedEntry {
    fn parse(item: &Value) -> Result<Self> {
        let id = item["id"].as_str().unwrap_or_default().to_owned();
        let version = item["version"].as_str().unwrap_or_default().to_owned();
        let executable = item["kind"] == "workflow"
            && item["executable_by_client_collective"] == true
            && item["workflow_plan"].is_object();
        let plan = if executable {
            let workflow = &item["workflow_plan"];
            let plan: Plan = serde_json::from_value(serde_json::json!({
                "id": id.clone(),
                "version": version.clone(),
                "legacy": workflow["legacy"],
                "model_selection": item["model_selection"],
                "registered_model": item["registered_model"],
                "steps": workflow["steps"]
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
            Some(plan)
        } else {
            None
        };

        let has_dependencies = item.get("declared_dependencies").is_some();
        let has_digest = item.get("routing_declarations_sha256").is_some();
        let has_capabilities = item.get("declared_capabilities").is_some();
        // Older snapshots carried capabilities without the later dependency
        // digest. They retain legacy single-workflow semantics, not DAG authority.
        if has_dependencies != has_digest || (has_dependencies && !has_capabilities) {
            bail!("Pinned expert routing metadata is incomplete")
        }
        let metadata_pinned = has_dependencies;
        let (capabilities, dependencies) = if metadata_pinned {
            let capabilities: Vec<crate::registry::Capability> =
                serde_json::from_value(item["declared_capabilities"].clone())?;
            let dependencies: Vec<crate::registry::Dependency> =
                serde_json::from_value(item["declared_dependencies"].clone())?;
            if capabilities.len() > 16 || dependencies.len() > 16 {
                bail!("Pinned expert routing declarations exceed their bound")
            }
            let declarations = serde_json::json!({
                "capabilities": capabilities,
                "dependencies": dependencies
            });
            if crate::store::sha256(&serde_json::to_vec(&declarations)?)
                != item["routing_declarations_sha256"]
                    .as_str()
                    .unwrap_or_default()
            {
                bail!("Pinned expert routing declarations checksum mismatch")
            }
            let capability_ids = capabilities
                .iter()
                .map(|cap| cap.id.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            if capability_ids.len() != capabilities.len()
                || capabilities.iter().any(|cap| {
                    !crate::registry::identifier(&cap.id)
                        || cap.version.len() > 64
                        || semver::Version::parse(&cap.version).is_err()
                })
            {
                bail!("Pinned expert capability declarations are invalid")
            }
            let dependency_ids = dependencies
                .iter()
                .map(|dependency| dependency.id.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            if dependency_ids.len() != dependencies.len()
                || dependencies.iter().any(|dependency| {
                    !crate::registry::identifier(&dependency.id)
                        || dependency.requirement.len() > 64
                        || semver::VersionReq::parse(&dependency.requirement).is_err()
                })
            {
                bail!("Pinned expert dependency declarations are invalid")
            }
            (capabilities, dependencies)
        } else {
            (Vec::new(), Vec::new())
        };
        Ok(Self {
            id,
            version,
            plan,
            capabilities,
            dependencies,
            metadata_pinned,
            executable,
        })
    }
}

fn dependency_closure<'a>(
    root: &'a PinnedEntry,
    entries: &'a [PinnedEntry],
) -> Option<Vec<&'a PinnedEntry>> {
    fn visit<'a>(
        entry: &'a PinnedEntry,
        entries: &'a [PinnedEntry],
        visiting: &mut std::collections::BTreeSet<String>,
        complete: &mut std::collections::BTreeSet<String>,
        ordered: &mut Vec<&'a PinnedEntry>,
    ) -> Option<()> {
        if complete.contains(&entry.id) {
            return Some(());
        }
        if !entry.metadata_pinned || !entry.executable || !visiting.insert(entry.id.clone()) {
            return None;
        }
        for dependency in &entry.dependencies {
            let mut matches = entries
                .iter()
                .filter(|candidate| candidate.id == dependency.id);
            let prerequisite = matches.next()?;
            if matches.next().is_some() || !prerequisite.metadata_pinned || !prerequisite.executable
            {
                return None;
            }
            let version = semver::Version::parse(&prerequisite.version).ok()?;
            let requirement = semver::VersionReq::parse(&dependency.requirement).ok()?;
            if !requirement.matches(&version) {
                return None;
            }
            visit(prerequisite, entries, visiting, complete, ordered)?;
            if ordered.len() > 3 {
                return None;
            }
        }
        visiting.remove(&entry.id);
        complete.insert(entry.id.clone());
        ordered.push(entry);
        (ordered.len() <= 3).then_some(())
    }

    let mut visiting = std::collections::BTreeSet::new();
    let mut complete = std::collections::BTreeSet::new();
    let mut ordered = Vec::new();
    visit(root, entries, &mut visiting, &mut complete, &mut ordered)?;
    Some(ordered)
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
