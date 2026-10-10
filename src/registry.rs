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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub id: String,
    pub requirement: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub source: String,
    pub license: String,
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
            || self.kind != "reference"
            || self.description.trim().is_empty()
            || self.description.len() > 500
            || self.tags.is_empty()
            || self.tags.len() > 32
            || self.dependencies.len() > 16
        {
            bail!("Invalid reference-skill manifest")
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
    let mut statement=db.prepare("SELECT m.manifest,m.checksum,m.skill_id,m.version FROM expert_manifests m JOIN skills s ON s.id=m.skill_id WHERE m.active=1 AND s.enabled=1 ORDER BY m.skill_id LIMIT 257")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 256 {
        bail!("Active expert registry exceeds capacity")
    }
    rows.into_iter()
        .map(|(raw, hash, id, version)| {
            let manifest = verified_manifest(raw, hash)?;
            if manifest.id != id || manifest.version != version {
                bail!("Manifest identity mismatch")
            }
            Ok((manifest.id.clone(), manifest))
        })
        .collect()
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
    let existing: Option<String> = tx
        .query_row(
            "SELECT checksum FROM expert_manifests WHERE skill_id=?1 AND version=?2",
            params![manifest.id, manifest.version],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
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
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (raw, checksum): (String, String) = tx.query_row(
        "SELECT manifest,checksum FROM expert_manifests WHERE skill_id=?1 AND version=?2",
        params![id, version],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let candidate = verified_manifest(raw, checksum)?;
    if candidate.id != id || candidate.version != version {
        bail!("Manifest identity mismatch")
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
    security::authorize("human-cli", "skill.disable")?;
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut graph = active_graph(&tx)?;
    graph.remove(id);
    validate_graph(&graph)?;
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
    tx.commit()?;
    Ok(json!({"id":id,"enabled":false}))
}

pub fn health(store: &Store) -> Result<Value> {
    let db = store.lock()?;
    let graph = active_graph(&db)?;
    validate_graph(&graph)?;
    let mut entries = Vec::new();
    for manifest in graph.values() {
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM documents WHERE skill_id=?1",
            [&manifest.id],
            |row| row.get(0),
        )?;
        entries.push(json!({"id":manifest.id,"version":manifest.version,"kind":manifest.kind,"stored_documents":count,
            "dependency_health":"valid","note":"Stored document count is not retrieval quality or model health"}));
    }
    Ok(json!({"active":entries,"neural_loading":"unsupported"}))
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
            description: "Reviewed reference skill".into(),
            tags: vec!["rust".into()],
            dependencies: vec![],
            source: "test fixture".into(),
            license: "CC0-1.0".into(),
        }
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
