use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub const VERSION: &str = "25.12.19";
pub const UPSTREAM: &str = "7e163021e59cca4f8e1e35a7c828b5c6b7915953";
pub const SCHEMAS: [&str; 10] = [
    "cognitive",
    "module",
    "events",
    "bootstrap",
    "event_stream",
    "tool",
    "sql_tool",
    "tool_bootstrap",
    "agent",
    "general_module",
];
fn read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata =
        fs::metadata(path).map_err(|e| format!("Cannot inspect {}: {e}", path.display()))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!("Invalid bundled file bounds: {}", path.display()));
    }
    fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))
}
pub fn generated_name(schema: &str) -> String {
    format!("{schema}_generated.rs")
}
/// Detect stale/tampered reviewed source; this is consistency, not code attestation.
pub fn validate(root: &Path) -> Result<Vec<PathBuf>, String> {
    let directory = root.join("protocol/generated");
    let bytes = read(&directory.join("manifest.sha256"), 8192)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid binding manifest encoding")?;
    let mut lines = text.lines();
    if lines.next() != Some(format!("flatc={VERSION}").as_str())
        || lines.next() != Some(format!("upstream={UPSTREAM}").as_str())
    {
        return Err("Binding generator pin mismatch".into());
    }
    let mut entries = BTreeMap::new();
    for line in lines {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 3
            || fields[2].len() != 64
            || !fields[2]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || entries.insert((fields[0], fields[1]), fields[2]).is_some()
        {
            return Err("Invalid or duplicate binding manifest entry".into());
        }
    }
    if entries.len() != SCHEMAS.len() * 2 {
        return Err("Binding manifest inventory mismatch".into());
    }
    let mut generated = Vec::new();
    for schema in SCHEMAS {
        let input = format!("{schema}.fbs");
        let output = generated_name(schema);
        for (kind, name, path, limit) in [
            (
                "schema",
                input.as_str(),
                root.join("protocol").join(&input),
                64000,
            ),
            ("rust", output.as_str(), directory.join(&output), 2000000),
        ] {
            let expected = entries
                .get(&(kind, name))
                .ok_or("Missing binding manifest entry")?;
            let actual = format!("{:x}", Sha256::digest(read(&path, limit)?));
            if actual != *expected {
                return Err(format!("Bundled {kind} checksum mismatch: {name}"));
            }
        }
        generated.push(directory.join(output));
    }
    Ok(generated)
}

/// Accept the project's source dependency form and Cargo's normalized package form.
pub fn runtime_pinned(manifest: &str) -> bool {
    let expected = format!("\"={VERSION}\"");
    let mut section = "";
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = line;
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if ((section == "[dependencies]" && key.trim() == "flatbuffers")
                || (section == "[dependencies.flatbuffers]" && key.trim() == "version"))
                && value.trim() == expected
            {
                return true;
            }
        }
    }
    false
}
