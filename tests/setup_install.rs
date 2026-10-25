//! Exercise installer feature propagation without a network fetch or Rust build.
#[cfg(unix)]
#[test]
fn installer_preserves_nvml_feature_for_both_installed_packages() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("cogfabric-install-{}", uuid::Uuid::new_v4()));
    let bin = root.join("tools");
    std::fs::create_dir_all(&bin).unwrap();
    for (name, body) in [
        ("cargo", "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$INSTALL_LOG\"\n"),
        ("rustc", "#!/bin/sh\nexit 0\n"),
        ("git", "#!/bin/sh\nif [ \"$1\" = '-C' ]; then\n dir=$2\n shift 2\nfi\ncase $1 in\n checkout) mkdir -p \"$dir/scripts\"; printf '#!/bin/sh\\nexit 0\\n' > \"$dir/scripts/opencode-local.sh\" ;;\n rev-parse) echo 0123456789ab ;;\nesac\n"),
    ] {
        let path = bin.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    for gpu in [false, true] {
        for replace in [false, true] {
            let prefix = root.join(format!("install {gpu} {replace}"));
            let log = root.join(format!("calls-{gpu}-{replace}"));
            let mut command = std::process::Command::new("/bin/sh");
            command
                .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/setup.sh"))
                .arg("--prefix")
                .arg(&prefix)
                .env_clear()
                .env("HOME", &root)
                .env("TMPDIR", &root)
                .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                .env("INSTALL_LOG", &log);
            if gpu {
                command.arg("--nvml-gpu");
            }
            if replace {
                command.arg("--replace");
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let calls = std::fs::read_to_string(log).unwrap();
            let calls = calls.lines().collect::<Vec<_>>();
            assert_eq!(calls.len(), 2);
            for call in &calls {
                assert!(call.starts_with("install "));
                assert!(call.contains("--locked"));
                assert_eq!(call.contains("--features nvml-gpu"), gpu);
                assert_eq!(call.contains("--force"), replace);
            }
            assert!(calls[0].contains("--bins"));
            assert!(calls[1].contains("/crates/cogfabric-server"));
            assert!(prefix.join("bin/cogfabric-opencode").is_file());
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
