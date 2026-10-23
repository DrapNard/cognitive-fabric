//! Optional Wasmi verifier for untrusted operator-installed WebAssembly.
//!
//! ABI v1 exports exactly `memory` and `verify(i32,i32,i32,i32,i32,i32)->i32`.
//! The arguments are `(criterion_ptr, criterion_len, objective_ptr,
//! objective_len, answer_ptr, answer_len)`. The host writes UTF-8 bytes to fixed
//! bounded regions before the call. Return `1` to accept the candidate for this
//! secondary check or `0` to veto it; every other result is invalid. The
//! compiled Rust criterion remains the only authority that can accept an answer.
//!
//! There are no WASI imports, host functions, filesystem, network, or tool APIs.
//! Fuel bounds execution including a module start function. A deadline can stop
//! the caller from waiting, but cannot preempt a blocking Wasmi call instantly;
//! the execution permit remains held until that call actually returns.

use super::provider::{
    VerificationClaim, VerificationFuture, VerificationRequest, VerifierDescriptor,
    VerifierProvider,
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    path::Path,
    sync::{Arc, OnceLock},
    time::Instant,
};
use tokio::sync::Semaphore;
use wasmi::{
    Config, EnforcedLimits, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
};

const MAX_MODULE_BYTES: usize = 1_000_000;
const MAX_CRITERION_BYTES: usize = 64;
const MAX_OBJECTIVE_BYTES: usize = 12_000;
const MAX_ANSWER_BYTES: usize = 8_192;
const MAX_MEMORY_BYTES: usize = 1_048_576;
const MAX_TABLE_ELEMENTS: usize = 256;
const MAX_FUEL: u64 = 2_000_000;
const MAX_STACK_HEIGHT: usize = 64 * 1024;
const MAX_RECURSION_DEPTH: usize = 64;
const MAX_CONCURRENT_JOBS: usize = 2;
const ABI_VERSION: u32 = 1;
const CRITERION_OFFSET: usize = 0;
const OBJECTIVE_OFFSET: usize = 64;
const ANSWER_OFFSET: usize = OBJECTIVE_OFFSET + MAX_OBJECTIVE_BYTES;

static EXECUTION_PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn permits() -> Arc<Semaphore> {
    EXECUTION_PERMITS
        .get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_JOBS)))
        .clone()
}

#[derive(Clone, Copy, Debug, Serialize)]
struct SettingsIdentity {
    abi_version: u32,
    max_module_bytes: usize,
    max_criterion_bytes: usize,
    max_objective_bytes: usize,
    max_answer_bytes: usize,
    max_memory_bytes: usize,
    max_table_elements: usize,
    max_fuel: u64,
    max_stack_height: usize,
    max_recursion_depth: usize,
    max_concurrent_jobs: usize,
    imports: &'static str,
    return_codes: &'static str,
}

const SETTINGS: SettingsIdentity = SettingsIdentity {
    abi_version: ABI_VERSION,
    max_module_bytes: MAX_MODULE_BYTES,
    max_criterion_bytes: MAX_CRITERION_BYTES,
    max_objective_bytes: MAX_OBJECTIVE_BYTES,
    max_answer_bytes: MAX_ANSWER_BYTES,
    max_memory_bytes: MAX_MEMORY_BYTES,
    max_table_elements: MAX_TABLE_ELEMENTS,
    max_fuel: MAX_FUEL,
    max_stack_height: MAX_STACK_HEIGHT,
    max_recursion_depth: MAX_RECURSION_DEPTH,
    max_concurrent_jobs: MAX_CONCURRENT_JOBS,
    imports: "none",
    return_codes: "0=veto,1=secondary_acceptance",
};

struct Program {
    engine: Engine,
    module: Module,
}

/// A bytecode-hash-pinned Wasmi verifier. `implementation_sha256` in the
/// captured descriptor binds both the exact module hash and the full runtime
/// settings identity. `module_sha256()` exposes the artifact hash separately.
pub struct WasmVerifierProvider {
    descriptor: VerifierDescriptor,
    module_sha256: String,
    settings_sha256: String,
    program: Arc<Program>,
}

impl WasmVerifierProvider {
    /// Read a bounded regular file without following a final symlink, require
    /// its SHA-256 to match the operator-supplied pin, and compile it once.
    pub fn load(
        path: &Path,
        expected_module_sha256: &str,
        id: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self> {
        validate_sha256(expected_module_sha256)?;
        let bytes = crate::security::read_regular_bytes(path, MAX_MODULE_BYTES)?;
        if bytes.is_empty() {
            bail!("Wasm verifier module is empty")
        }
        let module_sha256 = crate::store::sha256(&bytes);
        if module_sha256 != expected_module_sha256 {
            bail!("Wasm verifier module hash does not match the operator pin")
        }
        let settings_sha256 = crate::store::sha256(&serde_json::to_vec(&SETTINGS)?);
        let implementation_sha256 = crate::store::sha256(&serde_json::to_vec(&(
            "cogfabric-wasm-verifier/v1",
            &module_sha256,
            &settings_sha256,
        ))?);
        let descriptor = VerifierDescriptor {
            id: id.into(),
            version: version.into(),
            implementation_sha256,
        };
        descriptor.validate()?;

        let mut config = Config::default();
        config
            .consume_fuel(true)
            .allow_start_fn(true)
            .ignore_custom_sections(true)
            .set_max_stack_height(MAX_STACK_HEIGHT)
            .set_max_recursion_depth(MAX_RECURSION_DEPTH)
            .wasm_multi_memory(false);
        config.enforced_limits(EnforcedLimits::strict());
        let engine = Engine::new(&config);
        let module = Module::new(&engine, &bytes)
            .map_err(|_| anyhow::anyhow!("Wasm verifier module failed bounded compilation"))?;
        if module.imports().next().is_some() {
            bail!("Wasm verifier modules cannot import host functions or resources")
        }
        let mut exports = module
            .exports()
            .map(|export| export.name())
            .collect::<Vec<_>>();
        exports.sort_unstable();
        if exports.len() != 2 || exports[0] != "memory" || exports[1] != "verify" {
            bail!("Wasm verifier must export only its ABI memory and verify function")
        }

        Ok(Self {
            descriptor,
            module_sha256,
            settings_sha256,
            program: Arc::new(Program { engine, module }),
        })
    }

    pub fn module_sha256(&self) -> &str {
        &self.module_sha256
    }

    pub fn settings_sha256(&self) -> &str {
        &self.settings_sha256
    }

    fn validate_request(request: &VerificationRequest) -> Result<()> {
        if request.criterion.is_empty()
            || request.criterion.len() > MAX_CRITERION_BYTES
            || !request
                .criterion
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_./-".contains(&byte))
            || request.objective.len() > MAX_OBJECTIVE_BYTES
            || request.answer.len() > MAX_ANSWER_BYTES
            || crate::store::sha256(request.objective.as_bytes()) != request.objective_sha256
            || crate::store::sha256(request.answer.as_bytes()) != request.answer_sha256
            || Instant::now() >= request.deadline
        {
            bail!("Wasm verifier request is expired, inconsistent, or exceeds bounds")
        }
        Ok(())
    }

    fn run(program: Arc<Program>, request: VerificationRequest) -> Result<VerificationClaim> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(MAX_MEMORY_BYTES)
            .table_elements(MAX_TABLE_ELEMENTS)
            .instances(1)
            .memories(1)
            .tables(1)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(&program.engine, HostState { limits });
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(MAX_FUEL)
            .context("Failed to configure Wasm verifier fuel")?;

        // With imports rejected, an empty linker can only instantiate modules
        // that depend on their own bounded memory and functions.
        let linker = Linker::<HostState>::new(&program.engine);
        let instance = linker
            .instantiate_and_start(&mut store, &program.module)
            .map_err(|_| anyhow::anyhow!("Wasm verifier instantiation or start failed"))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| anyhow::anyhow!("Wasm verifier ABI memory is missing"))?;
        let required_size = ANSWER_OFFSET + request.answer.len();
        if memory.data_size(&store) < required_size || memory.data_size(&store) > MAX_MEMORY_BYTES {
            bail!("Wasm verifier ABI memory is outside configured bounds")
        }
        memory.write(&mut store, CRITERION_OFFSET, request.criterion.as_bytes())?;
        memory.write(&mut store, OBJECTIVE_OFFSET, request.objective.as_bytes())?;
        memory.write(&mut store, ANSWER_OFFSET, request.answer.as_bytes())?;

        let verify = instance
            .get_typed_func::<(i32, i32, i32, i32, i32, i32), i32>(&store, "verify")
            .map_err(|_| anyhow::anyhow!("Wasm verifier has an invalid ABI signature"))?;
        let result = verify
            .call(
                &mut store,
                (
                    CRITERION_OFFSET as i32,
                    request.criterion.len() as i32,
                    OBJECTIVE_OFFSET as i32,
                    request.objective.len() as i32,
                    ANSWER_OFFSET as i32,
                    request.answer.len() as i32,
                ),
            )
            .map_err(|_| anyhow::anyhow!("Wasm verifier trapped or exhausted its fuel"))?;
        match result {
            1 => Ok(VerificationClaim::accepted()),
            0 => Ok(VerificationClaim::rejected("wasm_veto")),
            _ => bail!("Wasm verifier returned an invalid claim code"),
        }
    }
}

struct HostState {
    limits: StoreLimits,
}

impl VerifierProvider for WasmVerifierProvider {
    fn descriptor(&self) -> VerifierDescriptor {
        self.descriptor.clone()
    }

    fn verify(&self, request: VerificationRequest) -> VerificationFuture<'_> {
        Box::pin(async move {
            Self::validate_request(&request)?;
            let deadline = tokio::time::Instant::from_std(request.deadline);
            let permit = tokio::time::timeout_at(deadline, permits().acquire_owned())
                .await
                .map_err(|_| anyhow::anyhow!("Wasm verifier deadline exceeded while queued"))??;
            let program = self.program.clone();
            let job = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                Self::run(program, request)
            });
            tokio::time::timeout_at(deadline, job)
                .await
                .map_err(|_| anyhow::anyhow!("Wasm verifier deadline exceeded during execution"))?
                .map_err(|_| anyhow::anyhow!("Wasm verifier worker failed"))?
        })
    }
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("Invalid Wasm verifier module hash")
    }
    Ok(())
}
