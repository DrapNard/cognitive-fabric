use anyhow::{bail, Result};
use std::{fs, path::Path};

pub fn authorize(principal: &str, capability: &str) -> Result<()> {
    let allowed = match principal {
        "interface" => matches!(capability, "llm.generate"),
        "jev-r" => matches!(
            capability,
            "llm.generate" | "knowledge.search" | "math.evaluate"
        ),
        "jev-plus" => matches!(capability, "knowledge.search" | "math.evaluate"),
        "human-cli" => matches!(
            capability,
            "knowledge.ingest"
                | "candidate.create"
                | "candidate.evaluate"
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
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("Document must be a regular file, not a symlink")
    }
    if !matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md" | "txt")
    ) {
        bail!("Only .md and .txt documents can be ingested")
    }
    if metadata.len() > 256_000 {
        bail!("Document exceeds 256 KB")
    }
    Ok(fs::read_to_string(path)?)
}

pub fn validate_provenance(source: &str, license: &str) -> Result<()> {
    if source.trim().is_empty()
        || license.trim().is_empty()
        || source.eq_ignore_ascii_case("unknown")
        || license.eq_ignore_ascii_case("unknown")
    {
        bail!("Explicit source and license are required");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deny_unknown_actor() {
        assert!(authorize("rust.s1", "candidate.promote").is_err());
    }
    #[test]
    fn deny_self_promotion() {
        assert!(authorize("jev-r", "candidate.promote").is_err());
    }
    #[test]
    fn allow_knowledge_search() {
        assert!(authorize("jev-r", "knowledge.search").is_ok());
    }
}
