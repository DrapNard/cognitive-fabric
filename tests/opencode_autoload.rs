//! Launcher policy tests; native HTTP mutation/receipt validation is covered separately.
#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn launcher_loads_only_the_installed_configured_model_and_preserves_other_instances() {
    for scenario in [
        "unloaded", "loaded", "small", "missing", "disabled", "unknown", "blocked", "offline",
        "gpu", "bad-gpu",
    ] {
        let root = std::env::temp_dir().join(format!("cgf-autoload-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join("config.json"), "{}").unwrap();
        fs::set_permissions(root.join("config.json"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/opencode.example.json"),
            root.join("opencode.json"),
        )
        .unwrap();
        let fixture = r##"#!/bin/bash
set -eu
if [[ "$1" == --version ]]; then echo 'opencode v2.0.32'; exit 0; fi
for arg in "$@"; do
case "$arg" in
config-show) echo '{"backend":"openai-local","api_base":"http://127.0.0.1:1234/v1","model":"selected","max_tokens":1600,"max_tool_calls":4}'; exit 0;;
residency-managed-status)
 if [[ "$SCENARIO" == blocked ]]; then echo '{"managed_lifecycle_blocked":true,"unresolved_provider_operation":true}'; else echo '{"managed_lifecycle_blocked":false,"unresolved_provider_operation":false}'; fi; exit 0;;
residency-status)
 [[ "$SCENARIO" != offline ]] || exit 1
 context=32768; [[ "$SCENARIO" != small ]] || context=4096
 instances='[]'; if [[ "$SCENARIO" == loaded || "$SCENARIO" == small || -f "$FIXTURE_ROOT/did-load" ]]; then instances="[{\"id\":\"selected-instance\",\"config\":{\"context_length\":$context}}]"; fi
 key=selected; [[ "$SCENARIO" != missing ]] || key=another
 printf '{"adapter":"lmstudio-native-v1","models":[{"key":"%s","type":"llm","loaded_instances":%s},{"key":"unrelated","type":"llm","loaded_instances":[{"id":"keep","config":{"context_length":4096}}]}]}\n' "$key" "$instances"; exit 0;;
residency-managed-load)
 printf '%s\n' "$*" >> "$FIXTURE_ROOT/load-calls"
 [[ "$SCENARIO" != unknown ]] || exit 1
 touch "$FIXTURE_ROOT/did-load"
 gpu=false; [[ "$SCENARIO" != gpu ]] || gpu=true
 printf '{"server_confirmed":true,"receipt":{"model_key":"selected","context_length":32768,"offload_kv_cache_to_gpu":%s}}\n' "$gpu"; exit 0;;
diagnostics) touch "$FIXTURE_ROOT/reached-diagnostics"; exit 1;;
esac
done
exit 99
"##;
        let binary = root.join("fixture");
        fs::write(&binary, fixture).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let output = Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/opencode-local.sh"
            ))
            .current_dir(&root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &root)
            .env("TMPDIR", &root)
            .env("COGFABRIC_HOME", &root)
            .env("COGFABRIC_BIN", &binary)
            .env("COGFABRIC_SERVER_BIN", &binary)
            .env("OPENCODE_BIN", &binary)
            .env("FIXTURE_ROOT", &root)
            .env("SCENARIO", scenario)
            .env(
                "COGFABRIC_OPENCODE_KV_GPU",
                match scenario {
                    "gpu" => "true",
                    "bad-gpu" => "invalid",
                    _ => "false",
                },
            )
            .env(
                "COGFABRIC_OPENCODE_AUTO_LOAD",
                if scenario == "disabled" { "0" } else { "1" },
            )
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "fixture intentionally stops before inference"
        );
        let loaded = root.join("load-calls").exists();
        assert_eq!(
            loaded,
            matches!(scenario, "unloaded" | "unknown" | "gpu"),
            "{scenario}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            root.join("reached-diagnostics").exists(),
            matches!(scenario, "unloaded" | "loaded" | "offline" | "gpu"),
            "{scenario}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if loaded {
            let calls = fs::read_to_string(root.join("load-calls")).unwrap();
            assert_eq!(calls.lines().count(), 1);
            assert!(calls.contains("--model selected --context-length 32768"));
            assert!(calls.contains("--human-approve"));
            assert!(calls.contains(if scenario == "gpu" {
                "--offload-kv-cache-to-gpu true"
            } else {
                "--offload-kv-cache-to-gpu false"
            }));
            assert!(!calls.contains("--authenticate"));
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn launcher_uses_a_private_client_with_the_current_token_and_refuses_remote_sessions() {
    let root = std::env::temp_dir().join(format!("cgf-private-client-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(root.join("config.json"), "{}").unwrap();
    fs::set_permissions(root.join("config.json"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/opencode.example.json"),
        root.join("opencode.json"),
    )
    .unwrap();
    let tools = root.join("tools");
    fs::create_dir(&tools).unwrap();
    for (name, script) in [
        (
            "core",
            r#"#!/bin/bash
set -eu
for arg in "$@"; do
case "$arg" in
 config-show) echo '{"backend":"openai-local","api_base":"http://127.0.0.1:1234/v1","model":"fixture","max_tokens":1600,"max_tool_calls":4}'; exit 0;;
 residency-status) echo '{"adapter":"lmstudio-native-v1","models":[{"key":"fixture","type":"llm","loaded_instances":[{"id":"fixture","config":{"context_length":32768}}]}]}'; exit 0;;
 residency-managed-status) echo '{"managed_lifecycle_blocked":false,"unresolved_provider_operation":false}'; exit 0;;
 diagnostics) echo '{"inference_discovery":{"status":"responded","all_configured_models_advertised":true}}'; exit 0;;
esac
done
exit 99
"#,
        ),
        (
            "curl",
            r#"#!/bin/bash
set -eu
output=''
while (( $# )); do
 if [[ "$1" == --output ]]; then output=$2; shift; fi
 shift
done
[[ -n "$output" ]]
echo '{"data":[{"id":"cogfabric-agent"}]}' > "$output"
printf 200
"#,
        ),
        (
            "client",
            r#"#!/bin/bash
set -eu
if [[ "$1" == --version ]]; then echo 'opencode v2.0.32'; exit 0; fi
[[ "$1" == --standalone ]]
[[ "${COGFABRIC_SERVER_API_KEY:-}" == fixture-current-server-key-0123456789 ]]
printf '%s\n' "$@" > "$FIXTURE_ROOT/client-args"
"#,
        ),
    ] {
        let path = tools.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for arguments in [
        vec![],
        vec!["run", "--format", "json", "Reply READY"],
        vec!["--server", "http://127.0.0.1:9999"],
    ] {
        let result = Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/opencode-local.sh"
            ))
            .args(&arguments)
            .current_dir(&root)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
            .env("HOME", &root)
            .env("TMPDIR", &root)
            .env("COGFABRIC_HOME", &root)
            .env("COGFABRIC_BIN", tools.join("core"))
            .env("COGFABRIC_SERVER_BIN", tools.join("core"))
            .env("OPENCODE_BIN", tools.join("client"))
            .env(
                "COGFABRIC_SERVER_API_KEY",
                "fixture-current-server-key-0123456789",
            )
            .env("FIXTURE_ROOT", &root)
            .output()
            .unwrap();
        if arguments.first() == Some(&"--server") {
            assert!(!result.status.success());
            assert!(!root.join("client-args").exists());
        } else {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let received = fs::read_to_string(root.join("client-args")).unwrap();
            let expected = std::iter::once("--standalone")
                .chain(arguments)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            assert_eq!(received, expected);
            fs::remove_file(root.join("client-args")).unwrap();
        }
        assert!(!String::from_utf8_lossy(&result.stdout).contains("fixture-current-server-key"));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("fixture-current-server-key"));
    }
    fs::remove_dir_all(root).unwrap();
}
