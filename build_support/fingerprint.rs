//! Reproducible source identity for applying reviewed runtime measurements.
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn collect(root: &Path, relative: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(io::Error::other(
                "Source fingerprint cannot follow symlinks",
            ));
        }
        if kind.is_dir() {
            collect(root, &path, paths)?;
        } else if kind.is_file()
            && matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("rs" | "fbs" | "sha256")
            )
        {
            paths.push(path);
        }
    }
    Ok(())
}
/// Includes source, bindings, schemas, build support and locked dependency inputs.
/// It identifies bytes, not a compiler, executable or provider's loaded weights.
pub fn calculate(root: &Path) -> io::Result<String> {
    let mut paths = vec![
        PathBuf::from("Cargo.toml"),
        PathBuf::from("Cargo.lock"),
        PathBuf::from("build.rs"),
    ];
    for directory in ["src", "build_support", "protocol"] {
        collect(root, Path::new(directory), &mut paths)?;
    }
    paths.sort();
    let mut digest = Sha256::new();
    digest.update(b"cogfabric-runtime-sources-v1\0");
    for path in paths {
        let name = path
            .to_str()
            .ok_or_else(|| io::Error::other("Non-UTF8 source path"))?;
        let bytes = fs::read(root.join(&path))?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(format!("{:x}", digest.finalize()))
}
