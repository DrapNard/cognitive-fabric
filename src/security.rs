use anyhow::{bail, Result};
use std::{fs, io::Read, os::unix::fs::OpenOptionsExt, path::Path};

/// Immutable task-local authority created by the deterministic runtime.
/// It is never deserialized from model, registry, or IPC content.
pub struct TaskPolicy {
    task: String,
    grants: std::collections::BTreeSet<(String, String)>,
}

impl TaskPolicy {
    pub(crate) fn reasoning(task: &str) -> Result<Self> {
        Self::scoped(
            task,
            &[
                ("interface", "llm.generate"),
                ("jev-r", "llm.generate"),
                ("jev-plus", "knowledge.search"),
                ("jev-plus", "math.evaluate"),
                ("jev-plus", "ordering.verify"),
                ("jev-plus", "sorting.verify"),
                ("jev-plus", "sql.query"),
            ],
        )
    }

    fn scoped(task: &str, grants: &[(&str, &str)]) -> Result<Self> {
        uuid::Uuid::parse_str(task)?;
        let mut approved = std::collections::BTreeSet::new();
        for (actor, capability) in grants {
            // A task grant can narrow the global policy, never expand it.
            authorize(actor, capability)?;
            approved.insert((actor.to_string(), capability.to_string()));
        }
        Ok(Self {
            task: task.into(),
            grants: approved,
        })
    }

    pub fn authorize(&self, task: &str, actor: &str, capability: &str) -> Result<()> {
        if task != self.task || !self.grants.contains(&(actor.into(), capability.into())) {
            bail!("Denied: capability is outside the task/actor scope")
        }
        authorize(actor, capability)
    }

    pub(crate) fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"task_id":self.task,"grants":self.grants.iter().map(|(actor,capability)|serde_json::json!({"actor":actor,"capability":capability})).collect::<Vec<_>>()})
    }
}

/// Open the reviewed file itself without following a substituted final symlink.
pub fn read_regular_bytes(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        bail!("Input must be a bounded regular file")
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        bail!("Input grew beyond its byte limit")
    }
    Ok(bytes)
}

pub fn authorize(principal: &str, capability: &str) -> Result<()> {
    let allowed = match principal {
        "interface" => matches!(capability, "llm.generate"),
        "jev-r" => matches!(
            capability,
            "llm.generate" | "knowledge.search" | "math.evaluate"
        ),
        "jev-plus" => matches!(
            capability,
            "knowledge.search"
                | "math.evaluate"
                | "ordering.verify"
                | "sorting.verify"
                | "sql.query"
        ),
        "human-cli" => matches!(
            capability,
            "knowledge.ingest"
                | "telemetry.gpu"
                | "inference.diagnose"
                | "inference.worker.configure"
                | "inference.auth.configure"
                | "model.register"
                | "model.select"
                | "model.lifecycle"
                | "storage.retention"
                | "storage.backup"
                | "storage.restore"
                | "task.observe"
                | "task.recover"
                | "worker.recover"
                | "memory.publish"
                | "memory.revoke"
                | "memory.search"
                | "candidate.create"
                | "candidate.prepare"
                | "candidate.evaluate"
                | "evaluation.security"
                | "candidate.promote"
                | "candidate.rollback"
                | "skill.register"
                | "skill.activate"
                | "skill.disable"
        ),
        _ => false,
    };
    if !allowed {
        bail!("Denied: {principal} cannot use {capability}")
    }
    Ok(())
}

pub fn read_reviewed_document(path: &Path) -> Result<String> {
    authorize("human-cli", "knowledge.ingest")?;
    if !matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md" | "txt")
    ) {
        bail!("Only .md and .txt documents can be ingested")
    }
    Ok(String::from_utf8(read_regular_bytes(path, 256_000)?)?)
}

pub fn validate_provenance(source: &str, license: &str) -> Result<()> {
    if source.trim().is_empty()
        || license.trim().is_empty()
        || source.trim().eq_ignore_ascii_case("unknown")
        || license.trim().eq_ignore_ascii_case("unknown")
        || source.len() > 1000
        || license.len() > 128
    {
        bail!("Explicit source and license are required");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_scope_cannot_be_replayed_or_expand_actor_permissions() {
        let task = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        let policy = TaskPolicy::reasoning(&task).unwrap();
        assert!(policy.authorize(&task, "jev-plus", "math.evaluate").is_ok());
        assert!(policy
            .authorize(&other, "jev-plus", "math.evaluate")
            .is_err());
        assert!(policy
            .authorize(&task, "interface", "math.evaluate")
            .is_err());
        assert!(policy
            .authorize(&task, "jev-r", "candidate.promote")
            .is_err());
        assert!(policy.authorize(&task, "rust.s1", "llm.generate").is_err());
        assert!(TaskPolicy::scoped(&task, &[("jev-r", "candidate.promote")]).is_err());
        let restricted = TaskPolicy::scoped(&task, &[("jev-plus", "knowledge.search")]).unwrap();
        assert!(restricted
            .authorize(&task, "jev-plus", "math.evaluate")
            .is_err());
    }
    #[test]
    fn reviewed_reads_reject_symlinks_directories_and_oversized_inputs() {
        let root = std::env::temp_dir().join(format!("fabric-read-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let file = root.join("reference.md");
        fs::write(&file, b"reviewed").unwrap();
        assert_eq!(read_regular_bytes(&file, 8).unwrap(), b"reviewed");
        assert!(read_regular_bytes(&file, 7).is_err());
        assert!(read_regular_bytes(&root, 100).is_err());
        let link = root.join("substitute.md");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(read_reviewed_document(&link).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn deny_unknown_actor() {
        assert!(authorize("rust.s1", "candidate.promote").is_err());
    }
    #[test]
    fn deny_self_promotion() {
        assert!(authorize("jev-r", "candidate.promote").is_err());
        assert!(authorize("jev-r", "sql.query").is_err());
        assert!(authorize("interface", "sql.query").is_err());
    }
    #[test]
    fn allow_knowledge_search() {
        assert!(authorize("jev-r", "knowledge.search").is_ok());
    }
}
