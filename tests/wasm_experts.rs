#[cfg(not(feature = "wasm-experts"))]
#[test]
fn wasm_expert_cli_refuses_missing_feature_before_state_initialization() {
    let home = std::env::temp_dir().join(format!("fabric-wasm-disabled-{}", uuid::Uuid::new_v4()));
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args(["--wasm-expert", "missing.json", "ask", "calc:6*7"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!home.exists());
}

#[test]
fn wasm_restoration_without_approval_refuses_before_state_initialization() {
    let home =
        std::env::temp_dir().join(format!("fabric-wasm-resume-deny-{}", uuid::Uuid::new_v4()));
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&home)
        .args([
            "resume-task",
            "11111111-1111-4111-8111-111111111111",
            "--restore-wasm-expert",
            "missing.json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("explicit operator approval"));
    assert!(!home.exists());
}

#[cfg(feature = "wasm-experts")]
mod enabled {
    use cogfabric::{
        config::Config,
        engine::Engine,
        fabric::modules::{
            wasm::WasmModule, Capability, Descriptor, Limits, Module, Request, Response,
        },
        store::{sha256, Store},
    };
    use serde_json::{json, Value};
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    };
    fn root() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("fabric-wasm-expert-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        root
    }
    fn uleb(mut value: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            let mut b = (value & 127) as u8;
            value >>= 7;
            if value != 0 {
                b |= 128;
            }
            bytes.push(b);
            if value == 0 {
                return bytes;
            }
        }
    }
    fn sleb(mut value: i64) -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            let mut b = (value & 127) as u8;
            value >>= 7;
            let done = (value == 0 && b & 64 == 0) || (value == -1 && b & 64 != 0);
            if !done {
                b |= 128;
            }
            bytes.push(b);
            if done {
                return bytes;
            }
        }
    }
    fn section(id: u8, bytes: &[u8], out: &mut Vec<u8>) {
        out.push(id);
        out.extend(uleb(bytes.len()));
        out.extend(bytes);
    }
    fn program(payload: &[u8], mode: u8) -> Vec<u8> {
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();
        let mut types = vec![if mode == 4 { 2 } else { 1 }, 0x60, 2, 0x7f, 0x7f, 1, 0x7e];
        if mode == 4 {
            types.extend([0x60, 0, 0]);
        }
        section(1, &types, &mut wasm);
        if mode == 1 {
            section(
                2,
                &[1, 3, b'e', b'n', b'v', 4, b'h', b'o', b'o', b'k', 0, 0],
                &mut wasm,
            );
        }
        section(3, if mode == 4 { &[2, 0, 1] } else { &[1, 0] }, &mut wasm);
        let mut memory = vec![1, 0];
        memory.extend(uleb(if mode == 5 { 17 } else { 2 }));
        section(5, &memory, &mut wasm);
        let mut exports = vec![
            2, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, 7, b'e', b'x', b'e', b'c', b'u', b't',
            b'e', 0,
        ];
        exports.extend(uleb(usize::from(mode == 1)));
        section(7, &exports, &mut wasm);
        if mode == 4 {
            section(8, &[1], &mut wasm);
        }
        let mut body = vec![0];
        if mode == 2 {
            body.extend([0x03, 0x40, 0x0c, 0, 0x0b]);
        }
        if mode == 3 {
            body.extend([0x41, 32, 0x40, 0, 0x1a]);
        }
        body.push(0x42);
        body.extend(sleb(if mode == 6 {
            -1
        } else {
            ((65536u64 << 32) | payload.len() as u64) as i64
        }));
        body.push(0x0b);
        let mut code = vec![if mode == 4 { 2 } else { 1 }];
        code.extend(uleb(body.len()));
        code.extend(body);
        if mode == 4 {
            code.extend([2, 0, 0x0b]);
        }
        section(10, &code, &mut wasm);
        let mut data = vec![1, 0, 0x41];
        data.extend(sleb(65536));
        data.push(0x0b);
        data.extend(uleb(payload.len()));
        data.extend(payload);
        section(11, &data, &mut wasm);
        wasm
    }
    fn descriptor() -> Descriptor {
        Descriptor {
            id: "wasm.math".into(),
            version: "1.0.0".into(),
            capabilities: [Capability::MathEvaluate].into(),
        }
    }
    fn load(root: &std::path::Path, bytes: &[u8]) -> anyhow::Result<WasmModule> {
        let path = root.join("expert.wasm");
        std::fs::write(&path, bytes).unwrap();
        WasmModule::load(descriptor(), &path, &sha256(bytes))
    }
    fn config(root: &std::path::Path) -> Config {
        Config {
            home: root.join("state"),
            backend: "demo".into(),
            interpret_goals: false,
            ..Default::default()
        }
    }

    #[test]
    fn reconstruction_preserves_guest_identity_and_limits_and_fails_atomically() {
        use cogfabric::{
            fabric::modules::Broker, recovery::continuation::reconstruct_wasm_registry,
        };
        for scenario in 0..16 {
            let root = root();
            let path = root.join("fabric.sqlite3");
            let store = Arc::new(Store::open(&path).unwrap());
            let task = store
                .create_task("Restore pinned guest", "standard")
                .unwrap();
            let bytes = program(
                &WasmModule::encode_result(&Response::Arithmetic { value: "42".into() }).unwrap(),
                0,
            );
            let module = load(&root, &bytes).unwrap();
            let identity = module.implementation_identity().clone();
            let original = Broker::new(store.clone());
            let broker_id = original.identity_id();
            original
                .register(
                    Arc::new(module),
                    Limits {
                        queue_capacity: 1,
                        concurrency: 1,
                    },
                )
                .unwrap();
            let mut snapshot = json!({"format_version":4,"broker_id":broker_id.to_string(),
                "modules":[{"id":"wasm.math","version":"1.0.0","capabilities":["math.evaluate"],
                "source_sha256":null,"implementation":identity,"limits":{"queue_capacity":1,"concurrency":1}}],
                "permissions_granted":false});
            match scenario {
                2 => {
                    snapshot["modules"][0]["implementation"]["settings_sha256"] =
                        json!("0".repeat(64))
                }
                3 => snapshot["modules"][0]["version"] = json!("2.0.0"),
                4 => snapshot["modules"][0]["capabilities"] = json!(["sql.query"]),
                5 => snapshot["format_version"] = json!(3),
                6 => snapshot["modules"][0]["limits"]["concurrency"] = json!(0),
                7 => {
                    snapshot["modules"][0]["implementation"]["module_sha256"] =
                        json!("0".repeat(64))
                }
                14 => {
                    snapshot["modules"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("limits");
                }
                15 => snapshot["modules"][0]["limits"]["unrestricted"] = json!(true),
                _ => {}
            }
            store
                .event(&task, "typed_module_snapshot", "fabric", snapshot.clone())
                .unwrap();
            if scenario == 8 {
                store
                    .event(&task, "typed_module_snapshot", "fabric", snapshot)
                    .unwrap();
            }
            if scenario == 9 {
                store
                    .event(&task, "native_task_scope", "security-kernel", json!({}))
                    .unwrap();
            }
            if scenario == 10 {
                rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER deny_wasm_reconstruction BEFORE INSERT ON events WHEN NEW.kind='wasm_registry_reconstruction' BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
            }
            if scenario == 11 {
                rusqlite::Connection::open(&path)
                    .unwrap()
                    .execute("UPDATE tasks SET status='completed' WHERE id=?1", [&task])
                    .unwrap();
            }
            if scenario == 12 {
                store
                    .event(&task, "module_dispatch_intent", "jev-plus", json!({}))
                    .unwrap();
            }
            drop(original);
            drop(store);
            let store = Arc::new(Store::open(&path).unwrap());
            let before = store.task(&task).unwrap();
            let mut supplied = vec![load(&root, &bytes).unwrap()];
            if scenario == 13 {
                supplied.push(load(&root, &bytes).unwrap());
            }
            let result = reconstruct_wasm_registry(store.clone(), &task, scenario != 1, supplied);
            if scenario == 0 {
                let broker = result.unwrap();
                assert_eq!(broker.identity_id(), broker_id);
                let pin = broker.pin(&task, "wasm.math", "1.0.0").unwrap();
                assert_eq!(
                    pin.limits(),
                    Limits {
                        queue_capacity: 1,
                        concurrency: 1
                    }
                );
                let trace = store.task(&task).unwrap();
                let events = trace["events"].as_array().unwrap();
                let evidence = &events
                    .iter()
                    .find(|e| e["kind"] == "wasm_registry_reconstruction")
                    .unwrap()["payload"];
                assert_eq!(evidence["permissions_granted"], false);
                assert_eq!(evidence["worker_dispatched"], false);
                assert!(!events
                    .iter()
                    .any(|e| e["kind"] == "module_dispatch_intent"
                        || e["kind"] == "tool_reservation"));
            } else {
                assert!(result.is_err(), "scenario {scenario}");
                assert_eq!(store.task(&task).unwrap(), before, "scenario {scenario}");
            }
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn engine_executes_pinned_wasm_under_task_grants_and_independent_arithmetic_check() {
        let root = root();
        let cfg = config(&root);
        cfg.save().unwrap();
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        let bytes = program(
            &WasmModule::encode_result(&Response::Arithmetic { value: "42".into() }).unwrap(),
            0,
        );
        let module = load(&root, &bytes).unwrap();
        assert!(module
            .execute(
                "00000000-0000-4000-8000-000000000001",
                Instant::now() + Duration::from_secs(1),
                Request::Arithmetic {
                    expression: "6*7".into()
                }
            )
            .await
            .is_err());
        let engine = Engine::new(cfg, store.clone());
        engine
            .modules
            .register(Arc::new(module), Limits::default())
            .unwrap();
        std::fs::write(
            root.join("expert.wasm"),
            "changed pathname cannot replace compiled bytes",
        )
        .unwrap();
        let result = engine.run("calc:6*7", "fast").await.unwrap();
        assert_eq!(result.status, "completed");
        assert_eq!(result.answer, "42");
        assert_eq!(result.resources.calls, 0);
        let trace = store.task(&result.task_id).unwrap();
        let events = trace["events"].as_array().unwrap();
        let snapshot = &events
            .iter()
            .find(|e| e["kind"] == "typed_module_snapshot")
            .unwrap()["payload"];
        assert_eq!(snapshot["format_version"], 4);
        assert_eq!(
            snapshot["modules"][0]["implementation"]["module_sha256"],
            sha256(&bytes)
        );
        assert!(events.iter().any(
            |e| e["kind"] == "module_dispatch_outcome" && e["payload"]["status"] == "returned"
        ));
        let rejected = engine.run("calc:6*8", "fast").await.unwrap();
        assert_eq!(rejected.status, "failed");
        drop(engine);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn cli_manifest_expert_works_without_inference_and_cannot_grant_new_capabilities() {
        let root = root();
        let cfg = config(&root);
        cfg.save().unwrap();
        let bytes = program(
            &WasmModule::encode_result(&Response::Arithmetic { value: "42".into() }).unwrap(),
            0,
        );
        std::fs::write(root.join("expert.wasm"), &bytes).unwrap();
        let manifest = root.join("expert.json");
        let mut value = json!({"format_version":1,"id":"wasm.math","version":"1.0.0","module":"expert.wasm","sha256":sha256(&bytes),"capabilities":["math.evaluate"]});
        std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"))
            .arg("--home")
            .arg(&cfg.home)
            .arg("--json")
            .arg("--wasm-expert")
            .arg(&manifest)
            .args(["ask", "calc:6*7", "--mode", "fast"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["answer"], "42");
        value["capabilities"] = json!(["shell.execute"]);
        std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(WasmModule::load_manifest(&manifest).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn malicious_guests_cannot_import_grow_loop_or_return_untyped_and_out_of_range_results() {
        for mode in 0..8 {
            let root = root();
            let payload = if mode == 7 {
                b"malformed output".to_vec()
            } else {
                WasmModule::encode_result(&Response::Arithmetic { value: "42".into() }).unwrap()
            };
            let bytes = program(&payload, mode);
            let module = load(&root, &bytes);
            if matches!(mode, 1 | 4) {
                assert!(module.is_err(), "mode {mode}");
            } else {
                let cfg = config(&root);
                cfg.save().unwrap();
                let store = Arc::new(Store::open(&cfg.db()).unwrap());
                let engine = Engine::new(cfg, store.clone());
                engine
                    .modules
                    .register(Arc::new(module.unwrap()), Limits::default())
                    .unwrap();
                let result = engine.run("calc:6*7", "fast").await.unwrap();
                assert_eq!(
                    result.status,
                    if mode == 0 { "completed" } else { "failed" },
                    "mode {mode}"
                );
                drop(engine);
                drop(store);
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn artifact_pins_and_typed_output_bounds_are_required() {
        let root = root();
        let bytes = program(
            &WasmModule::encode_result(&Response::Arithmetic { value: "42".into() }).unwrap(),
            0,
        );
        let path = root.join("expert.wasm");
        std::fs::write(&path, bytes).unwrap();
        assert!(WasmModule::load(descriptor(), &path, &"0".repeat(64)).is_err());
        assert!(WasmModule::encode_result(&Response::Arithmetic {
            value: "x".repeat(257)
        })
        .is_err());
        assert!(WasmModule::encode_result(&Response::Sql {
            columns: vec!["column".into(); 17],
            rows: vec![]
        })
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
