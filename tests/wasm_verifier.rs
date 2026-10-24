#![cfg(feature = "wasm-verifier")]

use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    store::Store,
    verification::wasm::WasmVerifierProvider,
    VerificationRequest, VerifierProvider,
};
use std::{
    collections::VecDeque,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("cogfabric-wasm-{}", uuid::Uuid::new_v4()))
}

fn uleb(mut value: usize) -> Vec<u8> {
    let mut output = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return output;
        }
    }
}

fn section(id: u8, payload: &[u8], module: &mut Vec<u8>) {
    module.push(id);
    module.extend(uleb(payload.len()));
    module.extend(payload);
}

fn wasm_module(return_code: u8, minimum_memory_pages: usize, imported_function: bool) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    // One function type: verify(i32, i32, i32, i32, i32, i32) -> i32.
    section(
        1,
        &[1, 0x60, 6, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 1, 0x7f],
        &mut module,
    );
    if imported_function {
        let imports = [1, 3, b'e', b'n', b'v', 4, b'h', b'o', b'o', b'k', 0, 0];
        section(2, &imports, &mut module);
    }
    section(3, &[1, 0], &mut module);
    let mut memory = vec![1, 0];
    memory.extend(uleb(minimum_memory_pages));
    section(5, &memory, &mut module);

    let mut exports = vec![
        2, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, 6, b'v', b'e', b'r', b'i', b'f', b'y', 0,
    ];
    exports.extend(uleb(usize::from(imported_function)));
    section(7, &exports, &mut module);

    // Locals vector, i32.const return_code, end.
    let body = [0, 0x41, return_code, 0x0b];
    let mut code = vec![1, body.len() as u8];
    code.extend(body);
    section(10, &code, &mut module);
    module
}

fn fuel_exhausting_module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        1,
        &[1, 0x60, 6, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 1, 0x7f],
        &mut module,
    );
    section(3, &[1, 0], &mut module);
    section(5, &[1, 0, 1], &mut module);
    section(
        7,
        &[
            2, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, 6, b'v', b'e', b'r', b'i', b'f', b'y',
            0, 0,
        ],
        &mut module,
    );
    // A valid infinite loop. The i32.const is unreachable unless execution
    // escapes the loop; configured fuel must terminate it first.
    let body = [0, 0x03, 0x40, 0x0c, 0, 0x0b, 0x41, 1, 0x0b];
    let mut code = vec![1, body.len() as u8];
    code.extend(body);
    section(10, &code, &mut module);
    module
}

fn invalid_signature_module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(1, &[1, 0x60, 1, 0x7f, 1, 0x7f], &mut module);
    section(3, &[1, 0], &mut module);
    section(5, &[1, 0, 1], &mut module);
    section(
        7,
        &[
            2, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, 6, b'v', b'e', b'r', b'i', b'f', b'y',
            0, 0,
        ],
        &mut module,
    );
    let body = [0, 0x41, 1, 0x0b];
    let mut code = vec![1, body.len() as u8];
    code.extend(body);
    section(10, &code, &mut module);
    module
}

fn save_module(dir: &Path, name: &str, bytes: &[u8]) -> (PathBuf, String) {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    (path, cogfabric::store::sha256(bytes))
}

fn load(path: &Path, hash: &str) -> anyhow::Result<WasmVerifierProvider> {
    WasmVerifierProvider::load(path, hash, "fixture.wasm_verifier", "1.0.0")
}

fn request(answer: &str) -> VerificationRequest {
    VerificationRequest {
        criterion: "integer_sort/v1".into(),
        objective: r#"sort:{"values":[3,1,3]}"#.into(),
        objective_sha256: cogfabric::store::sha256(r#"sort:{"values":[3,1,3]}"#.as_bytes()),
        answer: answer.into(),
        answer_sha256: cogfabric::store::sha256(answer.as_bytes()),
        deadline: Instant::now() + Duration::from_secs(5),
    }
}

struct Script(Mutex<VecDeque<&'static str>>);

impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        assert_eq!(role, "solver");
        let text = self.0.lock().unwrap().pop_front().unwrap();
        Box::pin(async move {
            Ok(Generation {
                text: text.to_owned(),
                model: "wasm-verifier-fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
                provider_metrics: None,
            })
        })
    }
}

#[tokio::test]
async fn pinned_module_accepts_or_vetoes_with_only_strict_claim_codes() {
    let dir = temp_dir();
    let (accept_path, accept_hash) = save_module(&dir, "accept.wasm", &wasm_module(1, 1, false));
    let accepting = load(&accept_path, &accept_hash).unwrap();
    assert_eq!(accepting.module_sha256(), accept_hash);
    assert!(accepting.verify(request("[1,3,3]")).await.unwrap().accepted);

    let (veto_path, veto_hash) = save_module(&dir, "veto.wasm", &wasm_module(0, 1, false));
    let vetoing = load(&veto_path, &veto_hash).unwrap();
    let claim = vetoing.verify(request("[1,3,3]")).await.unwrap();
    assert!(!claim.accepted);
    assert_eq!(claim.reason_code.as_deref(), Some("wasm_veto"));

    let (invalid_path, invalid_hash) =
        save_module(&dir, "invalid-claim.wasm", &wasm_module(2, 1, false));
    let invalid = load(&invalid_path, &invalid_hash).unwrap();
    assert!(invalid.verify(request("[1,3,3]")).await.is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn malicious_imports_memory_growth_and_excessive_fuel_fail_closed() {
    let dir = temp_dir();
    let (import_path, import_hash) = save_module(&dir, "import.wasm", &wasm_module(1, 1, true));
    assert!(
        load(&import_path, &import_hash).is_err(),
        "imports must be rejected at load"
    );

    let (memory_path, memory_hash) = save_module(&dir, "memory.wasm", &wasm_module(1, 17, false));
    if let Ok(provider) = load(&memory_path, &memory_hash) {
        assert!(provider.verify(request("[1,3,3]")).await.is_err());
    }

    let (fuel_path, fuel_hash) = save_module(&dir, "fuel.wasm", &fuel_exhausting_module());
    let fuel = load(&fuel_path, &fuel_hash).unwrap();
    assert!(fuel.verify(request("[1,3,3]")).await.is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn invalid_abi_hash_and_symlinked_artifacts_are_rejected() {
    let dir = temp_dir();
    let (abi_path, abi_hash) = save_module(&dir, "abi.wasm", &invalid_signature_module());
    let abi = load(&abi_path, &abi_hash).unwrap();
    assert!(abi.verify(request("[1,3,3]")).await.is_err());

    assert!(
        load(&abi_path, &"0".repeat(64)).is_err(),
        "hash drift must fail closed"
    );
    let (module_path, module_hash) = save_module(&dir, "target.wasm", &wasm_module(1, 1, false));
    #[cfg(unix)]
    {
        let link = dir.join("linked.wasm");
        std::os::unix::fs::symlink(&module_path, &link).unwrap();
        assert!(
            load(&link, &module_hash).is_err(),
            "final symlinks must not be followed"
        );
    }
    let pinned = load(&module_path, &module_hash).unwrap();
    assert_eq!(pinned.descriptor().implementation_sha256.len(), 64);
    assert_eq!(pinned.settings_sha256().len(), 64);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn wasm_claims_remain_secondary_to_the_compiled_criterion() {
    let dir = temp_dir();
    let query = r#"sort:{"values":[3,1,3]}"#;
    for (name, claim, answer, expected_completed, expected_compiled, expected_provider) in [
        ("accept-valid", 1, "[1,3,3]", true, true, true),
        ("veto-valid", 0, "[1,3,3]", false, true, false),
        ("accept-invalid", 1, "[1,3]", false, false, true),
    ] {
        let (path, hash) =
            save_module(&dir, &format!("{name}.wasm"), &wasm_module(claim, 1, false));
        let provider = Arc::new(load(&path, &hash).unwrap());
        let cfg = Config {
            home: dir.join(name),
            backend: "openai-local".into(),
            interpret_goals: false,
            max_calls: 4,
            max_cycles: 1,
            max_tokens: 1024,
            max_tool_calls: 4,
            ..Config::default()
        };
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        let engine = Engine::with_backend(
            cfg.clone(),
            store.clone(),
            Arc::new(Script(Mutex::new(VecDeque::from([answer])))),
        )
        .with_verifier_provider(provider)
        .unwrap();
        let outcome = engine.run(query, "fast").await.unwrap();
        assert_eq!(outcome.status == "completed", expected_completed);
        let trace = store.task(&outcome.task_id).unwrap();
        let results = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "verification_provider_result")
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0]["payload"]["compiled_accepted"],
            expected_compiled
        );
        assert_eq!(
            results[0]["payload"]["provider_accepted"],
            expected_provider
        );
        let _ = std::fs::remove_dir_all(cfg.home);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
