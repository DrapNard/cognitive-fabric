//! Bounded immutable SQLite snapshots for comparable retrieval across evaluation arms.
use crate::store::{sha256, Store};
use anyhow::{bail, Result};
use rusqlite::{
    backup::{Backup, StepResult},
    Connection, DatabaseName,
};
use serde::Serialize;

const MAX_SNAPSHOT_BYTES: u64 = 64_000_000;
#[derive(Clone, Debug, Serialize)]
pub struct Metadata {
    pub database_snapshot_checksum: String,
    pub database_snapshot_bytes: u64,
    pub frozen: bool,
}
pub(crate) struct FrozenCorpus {
    pub(crate) store: Store,
    pub(crate) metadata: Metadata,
}
impl Store {
    pub(crate) fn freeze_corpus(&self) -> Result<FrozenCorpus> {
        let mut source = self.lock()?;
        let transaction = source.transaction()?;
        // Establish one read snapshot before measuring/copying; concurrent WAL writers
        // cannot change its contents or repeatedly restart the backup.
        transaction.query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
            row.get::<_, u64>(0)
        })?;
        let pages: u64 = transaction.pragma_query_value(None, "page_count", |row| row.get(0))?;
        let size: u64 = transaction.pragma_query_value(None, "page_size", |row| row.get(0))?;
        if pages.saturating_mul(size) > MAX_SNAPSHOT_BYTES {
            bail!("Evaluation snapshot exceeds 64 MB; use a smaller evaluation store before comparison")
        }
        let mut destination = Connection::open_in_memory()?;
        {
            let backup = Backup::new(&transaction, &mut destination)?;
            match backup.step(-1)? {
                StepResult::Done => {}
                _ => bail!("Evaluation snapshot could not acquire a consistent database view"),
            }
        }
        transaction.commit()?;
        let metadata = {
            let bytes = destination.serialize(DatabaseName::Main)?;
            if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
                bail!("Evaluation snapshot exceeds byte limit")
            }
            Metadata {
                database_snapshot_checksum: sha256(&bytes),
                database_snapshot_bytes: bytes.len() as u64,
                frozen: true,
            }
        };
        let store = Store::from_frozen_connection(destination)?;
        Ok(FrozenCorpus { store, metadata })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{self, Document};
    #[test]
    fn oversized_snapshots_fail_without_mutating_the_source() {
        let store = Store::in_memory().unwrap();
        store.lock().unwrap().execute_batch("CREATE TABLE oversized_fixture(body BLOB); INSERT INTO oversized_fixture VALUES (zeroblob(64000000));").unwrap();
        assert!(store
            .freeze_corpus()
            .err()
            .unwrap()
            .to_string()
            .contains("64 MB"));
        assert!(store.create_task("source remains writable", "fast").is_ok());
    }
    #[test]
    fn frozen_retrieval_preserves_versions_and_episodes_without_accepting_writes() {
        let store = Store::in_memory().unwrap();
        store::bootstrap(&store).unwrap();
        store
            .add_document(&Document {
                id: "first".into(),
                skill_id: "rust.s1".into(),
                title: "Rust".into(),
                body: "Rust snapshot sentinel".into(),
                source: "test".into(),
                license: "CC0".into(),
            })
            .unwrap();
        let task = store.create_task("Rust episode", "fast").unwrap();
        store
            .finish_task(&task, "unverified", "original episode")
            .unwrap();
        let memory = store.publish_memory(&task, "episode", "reviewed").unwrap();
        let mut manifest = crate::registry::Manifest {
            workflow: None,
            workflow_steps: None,
            id: "rust.s1".into(),
            version: "1.0.0".into(),
            kind: "reference".into(),
            description: "Rust references".into(),
            tags: vec!["rust".into()],
            dependencies: Vec::new(),
            source: "test".into(),
            license: "CC0".into(),
        };
        crate::registry::register(&store, &manifest).unwrap();
        crate::registry::activate(&store, "rust.s1", "1.0.0").unwrap();
        let frozen = store.freeze_corpus().unwrap();
        assert_eq!(frozen.metadata.database_snapshot_checksum.len(), 64);
        assert!(frozen.metadata.database_snapshot_bytes <= MAX_SNAPSHOT_BYTES);
        store.revoke_memory(&memory.id).unwrap();
        manifest.version = "2.0.0".into();
        crate::registry::register(&store, &manifest).unwrap();
        crate::registry::activate(&store, "rust.s1", "2.0.0").unwrap();
        store
            .add_document(&Document {
                id: "second".into(),
                skill_id: "rust.s1".into(),
                title: "Rust".into(),
                body: "Rust later mutation".into(),
                source: "test".into(),
                license: "CC0".into(),
            })
            .unwrap();
        let context = frozen
            .store
            .task_context_scoped("Rust", true, false)
            .unwrap();
        assert_eq!(
            context
                .manifests
                .iter()
                .find(|manifest| manifest.id == "rust.s1")
                .unwrap()
                .version,
            "1.0.0"
        );
        assert_eq!(
            store
                .task_context("Rust")
                .unwrap()
                .manifests
                .iter()
                .find(|manifest| manifest.id == "rust.s1")
                .unwrap()
                .version,
            "2.0.0"
        );
        assert_eq!(context.documents.len(), 1);
        assert_eq!(context.documents[0].id, "first");
        assert_eq!(context.memories.len(), 1);
        assert_eq!(store.search("Rust", None, 3).unwrap().len(), 2);
        assert!(frozen.store.create_task("cannot write", "fast").is_err());
        let next = store.freeze_corpus().unwrap();
        assert_ne!(
            frozen.metadata.database_snapshot_checksum,
            next.metadata.database_snapshot_checksum
        );
    }
}
