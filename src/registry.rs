//! Immutable reference-skill manifests and transactional dependency-aware lifecycle.
//! Registration never grants tool permissions or executes imported code.
use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[cfg(test)]
mod bounds_tests;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub id: String,
    pub requirement: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowStep {
    pub id: String,
    pub advice: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checker: Option<String>,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub source: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_steps: Option<Vec<WorkflowStep>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<crate::models::Selection>,
}

pub(crate) fn identifier(id: &str) -> bool {
    (3..=101).contains(&id.len())
        && id.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
}

impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if !identifier(&self.id)
            || !matches!(self.kind.as_str(), "reference" | "workflow" | "checker")
            || self.description.trim().is_empty()
            || self.description.len() > 500
            || self.tags.is_empty()
            || self.tags.len() > 32
            || self.dependencies.len() > 16
        {
            bail!("Invalid expert manifest")
        }
        match (self.kind.as_str(), self.workflow.as_deref(), self.workflow_steps.as_ref()) {
            ("reference" | "checker", None, None) => {},
            ("workflow", Some(text), None) if !text.trim().is_empty() && text.len() <= 2000 => {},
            ("workflow", None, Some(_)) => { self.workflow_order()?; },
            _ => bail!("Workflow experts require bounded advice; reference experts cannot contain workflows"),
        }
        if self.kind == "checker" {
            if !matches!(
                self.checker.as_deref(),
                Some("dependency_order/v1" | "integer_sort/v1")
            ) {
                bail!("Checker experts require a supported compiled checker identity")
            }
        } else if self.checker.is_some() {
            bail!("Only checker experts can bind a compiled checker")
        }
        if let Some(model) = &self.model {
            model.validate()?;
            if self.kind != "workflow" {
                bail!("Only workflow experts can bind an inference model")
            }
        }
        let version = Version::parse(&self.version)?;
        if version.to_string() != self.version || self.version.len() > 64 {
            bail!("Version must be canonical semantic version")
        }
        let mut tags = BTreeSet::new();
        for tag in &self.tags {
            if !(2..=40).contains(&tag.len())
                || !tag
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_+-".contains(&b))
                || !tags.insert(tag)
            {
                bail!("Invalid or duplicate manifest tag")
            }
        }
        let mut dependencies = BTreeSet::new();
        for dep in &self.dependencies {
            if !identifier(&dep.id) || dep.requirement.len() > 64 || !dependencies.insert(&dep.id) {
                bail!("Invalid or duplicate dependency")
            }
            VersionReq::parse(&dep.requirement)?;
        }
        security::validate_provenance(&self.source, &self.license)?;
        if self.source.len() > 1000 || self.license.len() > 128 {
            bail!("Manifest provenance exceeds bounds")
        }
        Ok(())
    }

    pub(crate) fn workflow_order(&self) -> Result<Vec<usize>> {
        let steps = self
            .workflow_steps
            .as_ref()
            .context("Missing structured workflow")?;
        let names = steps.iter().map(|step| &step.id).collect::<BTreeSet<_>>();
        if steps.is_empty()
            || steps.len() > 4
            || names.len() != steps.len()
            || steps.iter().map(|step| step.advice.len()).sum::<usize>() > 2000
        {
            bail!("Invalid workflow step count, identities or total advice size")
        }
        for step in steps {
            let dependencies = step.depends_on.iter().collect::<BTreeSet<_>>();
            if !identifier(&step.id)
                || step.id.len() > 32
                || step.advice.trim().is_empty()
                || step.advice.len() > 1000
                || step.depends_on.len() > 3
                || dependencies.len() != step.depends_on.len()
                || step
                    .depends_on
                    .iter()
                    .any(|id| id == &step.id || !names.contains(id))
            {
                bail!("Invalid workflow advice or dependency")
            }
        }
        let mut order = Vec::new();
        let mut done = BTreeSet::new();
        while order.len() < steps.len() {
            let index = steps
                .iter()
                .enumerate()
                .find(|(_, step)| {
                    !done.contains(&step.id) && step.depends_on.iter().all(|id| done.contains(id))
                })
                .map(|(index, _)| index)
                .context("Workflow dependency cycle")?;
            done.insert(steps[index].id.clone());
            order.push(index);
        }
        Ok(order)
    }
}

pub fn read_manifest(path: &Path) -> Result<Manifest> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 64_000 {
        bail!("Manifest must be a bounded regular file")
    }
    let manifest: Manifest = serde_json::from_slice(&security::read_regular_bytes(path, 64_000)?)?;
    manifest.validate()?;
    Ok(manifest)
}

fn verified_manifest(raw: String, checksum: String) -> Result<Manifest> {
    if raw.len() > 64_000 || sha256(raw.as_bytes()) != checksum {
        bail!("Manifest integrity check failed")
    }
    let manifest: Manifest = serde_json::from_str(&raw)?;
    manifest.validate()?;
    Ok(manifest)
}

fn active_graph(db: &Connection) -> Result<BTreeMap<String, Manifest>> {
    active_graph_except(db, &[])
}

fn active_graph_except(db: &Connection, excluded: &[String]) -> Result<BTreeMap<String, Manifest>> {
    let mut statement=db.prepare("SELECT CASE WHEN length(CAST(m.manifest AS BLOB))<=64000 THEN m.manifest END, CASE WHEN length(CAST(m.checksum AS BLOB))=64 THEN m.checksum END, CASE WHEN length(CAST(m.skill_id AS BLOB))<=101 THEN m.skill_id END, CASE WHEN length(CAST(m.version AS BLOB))<=64 THEN m.version END, CASE WHEN length(CAST(s.description AS BLOB))<=500 THEN s.description END, CASE WHEN length(CAST(s.tags_json AS BLOB))<=2048 THEN s.tags_json END FROM expert_manifests m JOIN skills s ON s.id=m.skill_id WHERE m.active=1 AND s.enabled=1 AND m.skill_id NOT IN (SELECT value FROM json_each(?1)) ORDER BY m.skill_id LIMIT 257")?;
    let mut rows = statement.query([serde_json::to_string(excluded)?])?;
    let mut graph = BTreeMap::new();
    while let Some(row) = rows.next()? {
        if graph.len() == 256 {
            bail!("Active expert registry exceeds capacity")
        }
        let raw: String = row.get(0)?;
        let checksum: String = row.get(1)?;
        let id: String = row.get(2)?;
        let version: String = row.get(3)?;
        let description: String = row.get(4)?;
        let tags_raw: String = row.get(5)?;
        let tags: Vec<String> = serde_json::from_str(&tags_raw)
            .map_err(|_| anyhow::anyhow!("Invalid active expert routing tags"))?;
        let manifest = verified_manifest(raw, checksum)?;
        if manifest.description != description || manifest.tags != tags {
            bail!("Active expert routing metadata differs from its immutable manifest")
        }
        if manifest.id != id || manifest.version != version {
            bail!("Manifest identity mismatch")
        }
        if graph.insert(id, manifest).is_some() {
            bail!("Duplicate active expert identity")
        }
    }
    Ok(graph)
}

fn validate_graph(graph: &BTreeMap<String, Manifest>) -> Result<()> {
    if graph.len() > 256 {
        bail!("Active expert registry exceeds capacity")
    }
    for manifest in graph.values() {
        for dependency in &manifest.dependencies {
            let target = graph
                .get(&dependency.id)
                .with_context(|| format!("Missing active dependency: {}", dependency.id))?;
            if !VersionReq::parse(&dependency.requirement)?
                .matches(&Version::parse(&target.version)?)
            {
                bail!(
                    "Dependency version conflict: {} requires {} {}",
                    manifest.id,
                    dependency.id,
                    dependency.requirement
                )
            }
        }
    }
    let mut resolved = BTreeSet::new();
    while resolved.len() < graph.len() {
        let before = resolved.len();
        for (id, manifest) in graph {
            if manifest
                .dependencies
                .iter()
                .all(|dep| resolved.contains(&dep.id))
            {
                resolved.insert(id.clone());
            }
        }
        if resolved.len() == before {
            bail!("Expert dependency cycle")
        }
    }
    Ok(())
}

pub fn register(store: &Store, manifest: &Manifest) -> Result<Value> {
    security::authorize("human-cli", "skill.register")?;
    manifest.validate()?;
    let raw = serde_json::to_string(manifest)?;
    let hash = sha256(raw.as_bytes());
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let existing: Option<(String,String)> = tx
        .query_row(
            "SELECT CASE WHEN length(CAST(manifest AS BLOB))<=64000 THEN manifest END, CASE WHEN length(CAST(checksum AS BLOB))=64 THEN checksum END FROM expert_manifests WHERE skill_id=?1 AND version=?2",
            params![manifest.id, manifest.version],
            |row| Ok((row.get(0)?,row.get(1)?)),
        )
        .optional()?;
    if let Some((raw, existing)) = existing {
        let stored = verified_manifest(raw, existing.clone())?;
        if stored.id != manifest.id || stored.version != manifest.version {
            bail!("Manifest identity mismatch")
        }
        if existing != hash {
            bail!("An immutable manifest version already exists with different content")
        }
        return Ok(
            json!({"id":manifest.id,"version":manifest.version,"registered":false,"checksum":hash}),
        );
    }
    let count: i64 = tx.query_row("SELECT COUNT(*) FROM expert_manifests", [], |row| {
        row.get(0)
    })?;
    if count >= 4096 {
        bail!("Manifest storage capacity exhausted")
    }
    crate::store::admit_skill_id(&tx, &manifest.id)?;
    tx.execute(
        "INSERT OR IGNORE INTO skills(id,description,tags_json,enabled) VALUES (?1,?2,?3,0)",
        params![
            manifest.id,
            manifest.description,
            serde_json::to_string(&manifest.tags)?
        ],
    )?;
    tx.execute(
        "INSERT INTO expert_manifests(skill_id,version,manifest,checksum) VALUES (?1,?2,?3,?4)",
        params![manifest.id, manifest.version, raw, hash],
    )?;
    tx.execute("INSERT INTO expert_audit(skill_id,version,action,created) VALUES (?1,?2,'register',unixepoch())",params![manifest.id,manifest.version])?;
    tx.commit()?;
    Ok(json!({"id":manifest.id,"version":manifest.version,"registered":true,"checksum":hash}))
}

pub fn activate(store: &Store, id: &str, version: &str) -> Result<Value> {
    security::authorize("human-cli", "skill.activate")?;
    if !identifier(id) || version.len() > 64 || Version::parse(version)?.to_string() != version {
        bail!("Invalid expert activation identity/version")
    }
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (raw, checksum): (String, String) = tx.query_row(
        "SELECT CASE WHEN length(CAST(manifest AS BLOB))<=64000 THEN manifest END, CASE WHEN length(CAST(checksum AS BLOB))=64 THEN checksum END FROM expert_manifests WHERE skill_id=?1 AND version=?2",
        params![id, version],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let candidate = verified_manifest(raw, checksum)?;
    if candidate.id != id || candidate.version != version {
        bail!("Manifest identity mismatch")
    }
    if let Some(selection) = &candidate.model {
        let model = crate::models::registered_from(&tx, selection)?
            .context("Unknown expert model registration")?;
        if !model.enabled || model.manifest.max_output_tokens < 192 {
            bail!("Expert model is disabled or cannot supply the reserved advice allocation")
        }
    }
    let mut graph = active_graph(&tx)?;
    graph.insert(id.into(), candidate.clone());
    validate_graph(&graph)?;
    tx.execute(
        "UPDATE expert_manifests SET active=0 WHERE skill_id=?1",
        [id],
    )?;
    tx.execute(
        "UPDATE expert_manifests SET active=1 WHERE skill_id=?1 AND version=?2",
        params![id, version],
    )?;
    tx.execute(
        "UPDATE skills SET enabled=1,description=?2,tags_json=?3 WHERE id=?1",
        params![
            id,
            candidate.description,
            serde_json::to_string(&candidate.tags)?
        ],
    )?;
    tx.execute("INSERT INTO expert_audit(skill_id,version,action,created) VALUES (?1,?2,'activate',unixepoch())",params![id,version])?;
    tx.commit()?;
    Ok(json!({"id":id,"version":version,"active":true}))
}

pub fn disable(store: &Store, id: &str) -> Result<Value> {
    disable_set(store, &[id.to_owned()])?;
    Ok(json!({"id":id,"enabled":false}))
}

/// Disable only the explicitly selected IDs, with one atomic audit transaction.
pub fn disable_set(store: &Store, ids: &[String]) -> Result<Value> {
    security::authorize("human-cli", "skill.disable")?;
    if ids.is_empty() || ids.len() > 256 {
        bail!("Select between 1 and 256 skills to disable")
    }
    let mut selected = BTreeSet::new();
    for id in ids {
        if !identifier(id) || !selected.insert(id) {
            bail!("Invalid or duplicate skill disable identity")
        }
    }
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let graph = active_graph_except(&tx, ids)?;
    validate_graph(&graph)?;
    for id in ids {
        let changed = tx.execute(
            "UPDATE skills SET enabled=0 WHERE id=?1 AND enabled=1",
            [id],
        )?;
        if changed != 1 {
            bail!("Skill is unknown or already disabled")
        }
        tx.execute(
            "INSERT INTO expert_audit(skill_id,action,created) VALUES (?1,'disable',unixepoch())",
            [id],
        )?;
    }
    tx.commit()?;
    Ok(json!({"ids":ids,"enabled":false}))
}

pub fn health(store: &Store) -> Result<Value> {
    let mut db = store.lock()?;
    let tx = db.transaction()?;
    let retained_skill_ids = Store::skills_from(&tx)?.len();
    let graph = active_graph(&tx)?;
    validate_graph(&graph)?;
    let mut entries = Vec::new();
    for manifest in graph.values() {
        let model_health = if let Some(selection) = &manifest.model {
            match crate::models::registered_from(&tx, selection)? {
                Some(model) if model.enabled && model.manifest.max_output_tokens >= 192 => {
                    "registered_enabled"
                }
                _ => "unavailable",
            }
        } else if manifest.kind == "checker" {
            "not_required"
        } else {
            "shared_backend"
        };
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM documents WHERE skill_id=?1",
            [&manifest.id],
            |row| row.get(0),
        )?;
        entries.push(json!({"id":manifest.id,"version":manifest.version,"kind":manifest.kind,"checker":manifest.checker,"stored_documents":count,
            "dependency_health":"valid","model":manifest.model,"model_health":model_health,"note":"Stored document count and registration do not prove retrieval quality, provider availability or loaded weights"}));
    }
    tx.commit()?;
    Ok(
        json!({"active":entries,"retained_skill_ids":retained_skill_ids,"neural_loading":"unsupported"}),
    )
}

pub(crate) fn snapshot_from(db: &Connection) -> Result<Vec<Manifest>> {
    let graph = active_graph(db)?;
    validate_graph(&graph)?;
    Ok(graph.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{self, Document};

    fn manifest(id: &str, version: &str) -> Manifest {
        Manifest {
            id: id.into(),
            version: version.into(),
            kind: "reference".into(),
            checker: None,
            workflow: None,
            workflow_steps: None,
            model: None,
            description: "Reviewed reference skill".into(),
            tags: vec!["rust".into()],
            dependencies: vec![],
            source: "test fixture".into(),
            license: "CC0-1.0".into(),
        }
    }

    #[test]
    fn checker_manifests_only_bind_known_compiled_criteria() {
        let mut checker = manifest("data.sort.s1", "1.0.0");
        checker.kind = "checker".into();
        checker.checker = Some("integer_sort/v1".into());
        checker.validate().unwrap();
        let store = Store::in_memory().unwrap();
        register(&store, &checker).unwrap();
        activate(&store, &checker.id, &checker.version).unwrap();
        let report = health(&store).unwrap();
        assert_eq!(report["active"][0]["checker"], "integer_sort/v1");
        assert_eq!(report["active"][0]["model_health"], "not_required");
        for variant in 0..4 {
            let mut bad = checker.clone();
            match variant {
                0 => bad.checker = Some("shell/v1".into()),
                1 => bad.checker = None,
                2 => bad.workflow = Some("Execute anything".into()),
                _ => bad.kind = "reference".into(),
            }
            assert!(register(&store, &bad).is_err());
        }
        assert!(security::authorize("jev-r", "skill.register").is_err());
    }
    #[test]
    fn structured_workflows_validate_dependencies_bounds_and_legacy_hashes() {
        let mut manifest = manifest("rust.workflow.s1", "1.0.0");
        let legacy = serde_json::to_string(&manifest).unwrap();
        assert_eq!(
            legacy,
            r#"{"id":"rust.workflow.s1","version":"1.0.0","kind":"reference","description":"Reviewed reference skill","tags":["rust"],"dependencies":[],"source":"test fixture","license":"CC0-1.0"}"#
        );
        assert!(!legacy.contains("workflow_steps"));
        let roundtrip: Manifest = serde_json::from_str(&legacy).unwrap();
        assert_eq!(serde_json::to_string(&roundtrip).unwrap(), legacy);
        manifest.kind = "workflow".into();
        manifest.workflow_steps = Some(vec![
            WorkflowStep {
                id: "review".into(),
                advice: "Review collected evidence".into(),
                depends_on: vec!["collect".into()],
            },
            WorkflowStep {
                id: "collect".into(),
                advice: "Collect advisory evidence".into(),
                depends_on: vec![],
            },
        ]);
        manifest.validate().unwrap();
        assert_eq!(manifest.workflow_order().unwrap(), vec![1, 0]);
        let original = manifest.clone();
        manifest.workflow = Some("ambiguous second representation".into());
        assert!(manifest.validate().is_err());
        manifest = original.clone();
        manifest.workflow_steps.as_mut().unwrap()[1]
            .depends_on
            .push("review".into());
        assert!(manifest.validate().is_err());
        manifest = original.clone();
        manifest.workflow_steps.as_mut().unwrap()[0].depends_on = vec!["missing".into()];
        assert!(manifest.validate().is_err());
        manifest = original.clone();
        manifest.workflow_steps.as_mut().unwrap()[0].advice = "x".repeat(1001);
        assert!(manifest.validate().is_err());
        manifest = original;
        manifest.workflow_steps.as_mut().unwrap()[0].id = "collect".into();
        assert!(manifest.validate().is_err());
        manifest.workflow_steps = Some(vec![]);
        assert!(manifest.validate().is_err());
        manifest.workflow_steps = Some(
            (0..5)
                .map(|index| WorkflowStep {
                    id: format!("step_{index}"),
                    advice: "Advice".into(),
                    depends_on: vec![],
                })
                .collect(),
        );
        assert!(manifest.validate().is_err());
        manifest.workflow_steps = Some(
            (0..3)
                .map(|index| WorkflowStep {
                    id: format!("step_{index}"),
                    advice: "x".repeat(800),
                    depends_on: vec![],
                })
                .collect(),
        );
        assert!(manifest.validate().is_err());
    }
    fn setup() -> (std::path::PathBuf, Store) {
        let path =
            std::env::temp_dir().join(format!("fabric-registry-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        (path, store)
    }

    #[test]
    fn staged_versions_are_immutable_and_activation_checks_reverse_dependencies() {
        let (path, store) = setup();
        let a = manifest("rust.base.s1", "1.0.0");
        register(&store, &a).unwrap();
        assert!(store::route(&store.skills().unwrap(), "rust").is_empty());
        assert_eq!(register(&store, &a).unwrap()["registered"], false);
        let mut changed = a.clone();
        changed.description = "different".into();
        assert!(register(&store, &changed).is_err());
        activate(&store, &a.id, &a.version).unwrap();
        let mut b = manifest("rust.async.s1", "1.0.0");
        b.dependencies.push(Dependency {
            id: a.id.clone(),
            requirement: "^1.0".into(),
        });
        register(&store, &b).unwrap();
        activate(&store, &b.id, &b.version).unwrap();
        let next = manifest(&a.id, "2.0.0");
        register(&store, &next).unwrap();
        assert!(activate(&store, &next.id, &next.version).is_err());
        assert_eq!(
            store
                .skills()
                .unwrap()
                .iter()
                .find(|s| s.id == a.id)
                .unwrap()
                .manifest_version
                .as_deref(),
            Some("1.0.0")
        );
        assert!(disable(&store, &a.id).is_err());
        disable(&store, &b.id).unwrap();
        activate(&store, &next.id, &next.version).unwrap();
        activate(&store, &a.id, &a.version).unwrap();
        assert!(health(&store).is_ok());
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_dependencies_cycles_and_tampering_fail_closed() {
        let (path, store) = setup();
        let a = manifest("rust.base.s1", "1.0.0");
        let mut b = manifest("rust.async.s1", "1.0.0");
        b.dependencies.push(Dependency {
            id: a.id.clone(),
            requirement: "*".into(),
        });
        register(&store, &b).unwrap();
        assert!(activate(&store, &b.id, &b.version).is_err());
        register(&store, &a).unwrap();
        activate(&store, &a.id, &a.version).unwrap();
        activate(&store, &b.id, &b.version).unwrap();
        let mut cyclic = manifest(&a.id, "1.1.0");
        cyclic.dependencies.push(Dependency {
            id: b.id.clone(),
            requirement: "*".into(),
        });
        register(&store, &cyclic).unwrap();
        assert!(activate(&store, &cyclic.id, &cyclic.version).is_err());
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE expert_manifests SET manifest='tampered' WHERE skill_id=?1 AND active=1",
                [&a.id],
            )
            .unwrap();
        assert!(health(&store).is_err());
        assert!(store.task_context("rust").is_err());
        assert!(activate(&store, &a.id, &a.version).is_err());
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn task_snapshots_survive_replacement_and_disable_removes_new_retrieval() {
        let (path, store) = setup();
        let old = manifest("rust.base.s1", "1.0.0");
        register(&store, &old).unwrap();
        activate(&store, &old.id, &old.version).unwrap();
        store
            .add_document(&Document {
                id: "reference".into(),
                skill_id: old.id.clone(),
                title: "Rust reference".into(),
                body: "Rust lifetime evidence".into(),
                source: "test".into(),
                license: "CC0".into(),
            })
            .unwrap();
        let pinned = store.task_context("rust lifetime").unwrap();
        let mut new = manifest(&old.id, "1.1.0");
        new.tags = vec!["linux".into()];
        register(&store, &new).unwrap();
        activate(&store, &new.id, &new.version).unwrap();
        assert_eq!(pinned.manifests[0].version, "1.0.0");
        assert_eq!(pinned.documents[0].body, "Rust lifetime evidence");
        assert_eq!(
            store.task_context("rust").unwrap().manifests[0].version,
            "1.1.0"
        );
        disable(&store, &old.id).unwrap();
        assert!(store.search("rust", None, 3).unwrap().is_empty());
        assert!(store::route(&store.skills().unwrap(), "linux").is_empty());
        assert_eq!(pinned.documents.len(), 1);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn metadata_cannot_introduce_execution_or_permissions() {
        let mut value = manifest("rust.s1", "1.0.0");
        value.kind = "lora".into();
        assert!(value.validate().is_err());
        value.kind = "reference".into();
        value.id = "../../secret".into();
        assert!(value.validate().is_err());
        let mut raw = serde_json::to_value(manifest("rust.s1", "1.0.0")).unwrap();
        raw["permissions"] = json!(["shell.execute"]);
        assert!(serde_json::from_value::<Manifest>(raw).is_err());
        assert!(security::authorize("jev-r", "skill.activate").is_err());
        assert!(security::authorize("rust.s1", "skill.register").is_err());
    }
}
