use anyhow::{bail, Result};
use std::{fs, io::Read, os::unix::fs::OpenOptionsExt, path::Path};

/// Immutable task-local authority created by the deterministic runtime.
/// It is never deserialized from model, registry, or IPC content.
pub struct TaskPolicy {
    task: String,
    grants: std::collections::BTreeSet<(String, String)>,
}

impl TaskPolicy {
    pub(crate) fn agent(task: &str) -> Result<Self> {
        Self::scoped(task, &[("jev-r", "llm.generate")])
    }

    /// Narrow authority for a host-created client collective. Client-declared
    /// MCP/web/file tools are never grants; this policy allows bounded shared
    /// retrieval, host-routed pure arithmetic and the core compiled checkers.
    pub(crate) fn client_collective(task: &str) -> Result<Self> {
        Self::scoped(
            task,
            &[
                ("jev-r", "llm.generate"),
                ("jev-plus", "knowledge.search"),
                ("jev-plus", "expert.workflow.advice"),
                ("jev-plus", "math.evaluate"),
                ("jev-plus", "ordering.verify"),
                ("jev-plus", "sorting.verify"),
                ("jev-plus", "path.verify"),
                ("jev-plus", "constraints.verify"),
            ],
        )
    }

    pub(crate) fn reasoning(task: &str) -> Result<Self> {
        Self::reasoning_scoped(task, false)
    }

    pub(crate) fn reasoning_with_finite_search(task: &str) -> Result<Self> {
        Self::reasoning_scoped(task, true)
    }

    fn reasoning_scoped(task: &str, finite_search: bool) -> Result<Self> {
        let mut grants = vec![
            ("interface", "llm.generate"),
            ("jev-r", "llm.generate"),
            ("jev-plus", "knowledge.search"),
            ("jev-plus", "math.evaluate"),
            ("jev-plus", "ordering.verify"),
            ("jev-plus", "sorting.verify"),
            ("jev-plus", "path.verify"),
            ("jev-plus", "constraints.verify"),
            ("jev-plus", "sql.query"),
        ];
        if finite_search {
            grants.push(("jev-plus", "constraints.solve"));
        }
        Self::scoped(task, &grants)
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
                | "expert.workflow.advice"
                | "math.evaluate"
                | "ordering.verify"
                | "sorting.verify"
                | "path.verify"
                | "constraints.verify"
                | "constraints.solve"
                | "sql.query"
        ),
        "human-cli" => matches!(
            capability,
            "knowledge.ingest"
                | "telemetry.gpu"
                | "inference.diagnose"
                | "inference.worker.configure"
                | "workers.limits.configure"
                | "inference.tokens.configure"
                | "tool.worker.configure"
                | "inference.auth.configure"
                | "model.register"
                | "model.select"
                | "model.lifecycle"
                | "storage.retention"
                | "storage.checkpoint"
                | "storage.backup"
                | "storage.restore"
                | "expert.module.evaluate"
                | "task.observe"
                | "skill.inspect"
                | "project.publish"
                | "project.read"
                | "task.recover"
                | "task.resume"
                | "worker.recover"
                | "worker.cleanup"
                | "memory.publish"
                | "memory.revoke"
                | "memory.search"
                | "candidate.create"
                | "candidate.prepare"
                | "candidate.acquire"
                | "candidate.evaluate"
                | "evaluation.security"
                | "evaluation.retry"
                | "evaluation.routing"
                | "resources.configure"
                | "resources.resolve"
                | "model.residency.inspect"
                | "model.residency.change"
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
    fn client_collective_grants_only_core_reasoning_retrieval_and_compiled_checks() {
        let task = uuid::Uuid::new_v4().to_string();
        let policy = TaskPolicy::client_collective(&task).unwrap();
        for capability in [
            "ordering.verify",
            "sorting.verify",
            "path.verify",
            "constraints.verify",
        ] {
            policy.authorize(&task, "jev-plus", capability).unwrap();
        }
        policy
            .authorize(&task, "jev-plus", "knowledge.search")
            .unwrap();
        policy.authorize(&task, "jev-r", "llm.generate").unwrap();
        for (actor, capability) in [
            ("jev-r", "knowledge.search"),
            ("jev-plus", "llm.generate"),
            ("jev-plus", "sql.query"),
            ("jev-plus", "math.evaluate"),
            ("jev-plus", "constraints.solve"),
            ("jev-r", "shell.execute"),
            ("jev-r", "network.connect"),
            ("jev-r", "filesystem.read"),
        ] {
            assert!(policy.authorize(&task, actor, capability).is_err());
        }
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

    #[test]
    fn remote_acquisition_is_operator_only() {
        assert!(authorize("human-cli", "candidate.acquire").is_ok());
        for actor in ["jev-plus", "jev-r", "interface", "rust.s1", "unknown"] {
            assert!(authorize(actor, "candidate.acquire").is_err());
        }
    }

    #[test]
    fn storage_checkpoint_is_human_only() {
        assert!(authorize("human-cli", "storage.checkpoint").is_ok());
        for actor in ["jev-plus", "jev-r", "interface", "rust.s1"] {
            assert!(authorize(actor, "storage.checkpoint").is_err());
        }
    }

    #[test]
    fn input_token_configuration_is_human_only() {
        assert!(authorize("human-cli", "inference.tokens.configure").is_ok());
        for actor in ["jev-plus", "jev-r", "interface", "rust.s1"] {
            assert!(authorize(actor, "inference.tokens.configure").is_err());
        }
    }

    #[test]
    fn worker_limit_configuration_is_human_only() {
        assert!(authorize("human-cli", "workers.limits.configure").is_ok());
        for actor in ["jev-plus", "jev-r", "interface", "rust.s1"] {
            assert!(authorize(actor, "workers.limits.configure").is_err());
        }
    }

    #[test]
    fn project_profiles_are_human_managed_only() {
        assert!(authorize("human-cli", "project.publish").is_ok());
        assert!(authorize("human-cli", "project.read").is_ok());
        for actor in ["jev-r", "jev-plus", "interface", "rust.s1"] {
            assert!(authorize(actor, "project.publish").is_err());
            assert!(authorize(actor, "project.read").is_err());
        }
    }
}

#[cfg(test)]
mod path_policy_tests {
    use super::*;
    #[test]
    fn path_check_is_bound_to_reasoning_actor_and_task() {
        let task = "00000000-0000-4000-8000-000000000001";
        let other = "00000000-0000-4000-8000-000000000002";
        let policy = TaskPolicy::reasoning(task).unwrap();
        policy.authorize(task, "jev-plus", "path.verify").unwrap();
        assert!(policy.authorize(other, "jev-plus", "path.verify").is_err());
        assert!(policy.authorize(task, "jev-r", "path.verify").is_err());
        assert!(TaskPolicy::agent(task)
            .unwrap()
            .authorize(task, "jev-plus", "path.verify")
            .is_err());
        assert!(authorize("interface", "path.verify").is_err());
        policy
            .authorize(task, "jev-plus", "constraints.verify")
            .unwrap();
        assert!(policy
            .authorize(task, "jev-plus", "constraints.solve")
            .is_err());
        let search_policy = TaskPolicy::reasoning_with_finite_search(task).unwrap();
        search_policy
            .authorize(task, "jev-plus", "constraints.solve")
            .unwrap();
        assert!(policy
            .authorize(other, "jev-plus", "constraints.verify")
            .is_err());
        assert!(search_policy
            .authorize(other, "jev-plus", "constraints.solve")
            .is_err());
        assert!(policy
            .authorize(task, "jev-r", "constraints.verify")
            .is_err());
        assert!(search_policy
            .authorize(task, "jev-r", "constraints.solve")
            .is_err());
        assert!(TaskPolicy::agent(task)
            .unwrap()
            .authorize(task, "jev-plus", "constraints.verify")
            .is_err());
        assert!(authorize("interface", "constraints.verify").is_err());
        assert!(authorize("interface", "constraints.solve").is_err());
    }
}
