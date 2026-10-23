//! No-import, fuel-bounded Wasm experts behind ordinary typed task grants.
//! ABI v1: memory + execute(request_ptr:i32,request_len:i32)->i64. The return
//! packs output offset in the high 32 bits and output length in the low 32 bits.
//! Payloads are versioned FlatBuffers CGWI/CGWO, never instructions or authority.
use super::{
    protocol, AuthorizationContext, Descriptor, Module as HostModule, ModuleFuture, Request,
    Response,
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{Arc, OnceLock},
    time::Instant,
};
use tokio::sync::Semaphore;
use wasmi::{
    Config, EnforcedLimits, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
};
const MAX_MODULE: usize = 1_000_000;
const MAX_MEMORY: usize = 1_048_576;
const FUEL: u64 = 2_000_000;
static PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();
fn permits() -> Arc<Semaphore> {
    PERMITS.get_or_init(|| Arc::new(Semaphore::new(2))).clone()
}
struct Program {
    engine: Engine,
    module: Module,
}
struct State {
    limits: StoreLimits,
}
/// Immutable compiled guest; it receives no Store, task grants, host functions,
/// WASI, credentials, filesystem, network or inference backend.
pub struct WasmModule {
    descriptor: Descriptor,
    identity: Value,
    program: Arc<Program>,
}
impl WasmModule {
    pub fn load(descriptor: Descriptor, path: &Path, expected_sha256: &str) -> Result<Self> {
        crate::security::authorize("human-cli", "skill.register")?;
        descriptor.validate()?;
        if expected_sha256.len() != 64
            || !expected_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            bail!("Invalid Wasm expert artifact hash")
        }
        let bytes = crate::security::read_regular_bytes(path, MAX_MODULE)?;
        Self::compile(descriptor, &bytes, expected_sha256)
    }

    /// A strict local manifest may name one sibling .wasm file. It declares
    /// capabilities only; the host still grants each individual invocation.
    pub fn load_manifest(path: &Path) -> Result<Self> {
        use std::{
            ffi::CString,
            fs::{self, File},
            io::Read,
            os::{
                fd::{AsRawFd, FromRawFd},
                unix::fs::OpenOptionsExt,
            },
        };
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Manifest {
            format_version: u32,
            id: String,
            version: String,
            module: String,
            sha256: String,
            capabilities: Vec<String>,
        }
        crate::security::authorize("human-cli", "skill.register")?;
        let raw = crate::security::read_regular_bytes(path, 16_000)?;
        let manifest: Manifest = serde_json::from_value(crate::strict_json::value(&raw, 16_000)?)?;
        if manifest.format_version != 1
            || manifest.module.len() > 240
            || std::path::Path::new(&manifest.module).components().count() != 1
            || !matches!(
                std::path::Path::new(&manifest.module).components().next(),
                Some(std::path::Component::Normal(_))
            )
            || std::path::Path::new(&manifest.module)
                .extension()
                .and_then(|s| s.to_str())
                != Some("wasm")
            || manifest.capabilities.is_empty()
            || manifest.capabilities.len() > 6
        {
            bail!("Invalid Wasm expert manifest shape")
        }
        let mut capabilities = std::collections::BTreeSet::new();
        for name in manifest.capabilities {
            let capability = match name.as_str() {
                "math.evaluate" => super::Capability::MathEvaluate,
                "sql.query" => super::Capability::SqlQuery,
                "sorting.verify" => super::Capability::SortingVerify,
                "ordering.verify" => super::Capability::OrderingVerify,
                "path.verify" => super::Capability::PathVerify,
                "constraints.solve" => super::Capability::ConstraintsSolve,
                _ => bail!("Unknown Wasm expert capability"),
            };
            if !capabilities.insert(capability) {
                bail!("Duplicate Wasm expert capability")
            }
        }
        let descriptor = Descriptor {
            id: manifest.id,
            version: manifest.version,
            capabilities,
        };
        descriptor.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let directory = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(parent)?;
        let name = CString::new(manifest.module)?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_MODULE as u64 {
            bail!("Wasm expert artifact is not a bounded regular file")
        }
        let mut bytes = Vec::new();
        file.take(MAX_MODULE as u64 + 1).read_to_end(&mut bytes)?;
        Self::compile(descriptor, &bytes, &manifest.sha256)
    }

    fn compile(descriptor: Descriptor, bytes: &[u8], expected_sha256: &str) -> Result<Self> {
        descriptor.validate()?;
        if bytes.len() > MAX_MODULE
            || expected_sha256.len() != 64
            || !expected_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            bail!("Invalid Wasm expert artifact pin or size")
        }
        if bytes.is_empty() || crate::store::sha256(bytes) != expected_sha256 {
            bail!("Wasm expert artifact hash differs from operator pin")
        }
        let settings = json!({"abi_version":1,"module_bytes":MAX_MODULE,"memory_bytes":MAX_MEMORY,"fuel":FUEL,
            "table_elements":256,"instances":1,"memories":1,"tables":1,"stack_height":65536,"recursion_depth":64,
            "concurrent_jobs":2,"imports":"none","start_function":false,"request_identifier":"CGWI","result_identifier":"CGWO",
            "payload_schema_sha256":crate::store::sha256(include_bytes!("../../../protocol/general_module.fbs"))});
        let identity = json!({"format_version":1,"runtime":"wasmi-typed-expert/v1","module_sha256":expected_sha256,
            "settings_sha256":crate::store::sha256(&serde_json::to_vec(&settings)?),"permissions_granted":false});
        let mut config = Config::default();
        config
            .consume_fuel(true)
            .allow_start_fn(false)
            .ignore_custom_sections(true)
            .set_max_stack_height(65536)
            .set_max_recursion_depth(64)
            .wasm_multi_memory(false);
        config.enforced_limits(EnforcedLimits::strict());
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes)
            .map_err(|_| anyhow::anyhow!("Wasm expert failed bounded compilation"))?;
        if module.imports().next().is_some() {
            bail!("Wasm experts cannot import functions or resources")
        }
        let mut exports = module
            .exports()
            .map(|export| export.name())
            .collect::<Vec<_>>();
        exports.sort_unstable();
        if exports != ["execute", "memory"] {
            bail!("Wasm expert must export only memory and execute")
        }
        Ok(Self {
            descriptor,
            identity,
            program: Arc::new(Program { engine, module }),
        })
    }
    /// Encode typed guest output for plugin tooling/fixtures. This creates no
    /// authorization context and cannot dispatch a module or accept an answer.
    pub fn encode_result(response: &Response) -> Result<Vec<u8>> {
        protocol::encode_expert_result(response)
    }
    pub fn implementation_identity(&self) -> &Value {
        &self.identity
    }
    fn run(program: Arc<Program>, request: Request, deadline: Instant) -> Result<Response> {
        if Instant::now() >= deadline {
            bail!("Wasm expert deadline expired before execution")
        }
        let input = protocol::encode_expert_request(&request)?;
        let limits = StoreLimitsBuilder::new()
            .memory_size(MAX_MEMORY)
            .table_elements(256)
            .instances(1)
            .memories(1)
            .tables(1)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(&program.engine, State { limits });
        store.limiter(|state| &mut state.limits);
        store.set_fuel(FUEL)?;
        let instance = Linker::<State>::new(&program.engine)
            .instantiate_and_start(&mut store, &program.module)
            .map_err(|_| anyhow::anyhow!("Wasm expert instantiation failed"))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| anyhow::anyhow!("Wasm expert memory is missing"))?;
        if memory.data_size(&store) < input.len() || memory.data_size(&store) > MAX_MEMORY {
            bail!("Wasm expert memory exceeds ABI bounds")
        }
        memory.write(&mut store, 0, &input)?;
        let execute = instance
            .get_typed_func::<(i32, i32), i64>(&store, "execute")
            .map_err(|_| anyhow::anyhow!("Wasm expert ABI signature differs"))?;
        let output = execute
            .call(&mut store, (0, input.len() as i32))
            .map_err(|_| anyhow::anyhow!("Wasm expert trapped or exhausted fuel"))?
            as u64;
        let offset = (output >> 32) as usize;
        let length = (output & 0xffff_ffff) as usize;
        if !(8..=protocol::MAX_FRAME).contains(&length)
            || offset
                .checked_add(length)
                .is_none_or(|end| end > memory.data_size(&store))
        {
            bail!("Wasm expert output range exceeds bounds")
        }
        let mut bytes = vec![0; length];
        memory.read(&store, offset, &mut bytes)?;
        let response = protocol::decode_expert_result(&bytes, request.capability())?;
        response.validate_and_size(&request)?;
        if Instant::now() >= deadline {
            bail!("Wasm expert completed after its original deadline")
        }
        Ok(response)
    }
}
impl HostModule for WasmModule {
    fn descriptor(&self) -> Descriptor {
        self.descriptor.clone()
    }
    fn implementation_identity(&self) -> Option<Value> {
        Some(self.identity.clone())
    }
    fn execute<'a>(
        &'a self,
        _task: &'a str,
        _deadline: Instant,
        _request: Request,
    ) -> ModuleFuture<'a> {
        Box::pin(async { bail!("Wasm expert requires host-minted task authorization") })
    }
    fn execute_authorized<'a>(
        &'a self,
        authorization: AuthorizationContext,
        task: &'a str,
        deadline: Instant,
        request: Request,
    ) -> ModuleFuture<'a> {
        Box::pin(async move {
            authorization.validate(
                task,
                request.capability().actor(),
                request.capability(),
                &self.descriptor,
                deadline,
                &request,
            )?;
            let expires = tokio::time::Instant::from_std(deadline);
            let permit = tokio::time::timeout_at(expires, permits().acquire_owned())
                .await
                .map_err(|_| anyhow::anyhow!("Wasm expert deadline exceeded while queued"))??;
            let program = self.program.clone();
            let job = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                Self::run(program, request, deadline)
            });
            tokio::time::timeout_at(expires, job)
                .await
                .map_err(|_| anyhow::anyhow!("Wasm expert execution deadline exceeded"))?
                .map_err(|_| anyhow::anyhow!("Wasm expert worker failed"))?
        })
    }
}
