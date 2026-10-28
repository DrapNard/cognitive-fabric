use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use cogfabric::{
    config::Config,
    engine::Engine,
    evaluation, security, service,
    store::{self, Store},
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(
    name = "cogfabric",
    version,
    about = "Resource-bounded Rust cognitive coordination runtime"
)]
struct Cli {
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    #[arg(long, global = true)]
    backend: Option<String>,
    #[arg(long, global = true)]
    json: bool,
    /// Execute supported typed tools in the compiled same-image module worker.
    #[arg(long, global = true)]
    typed_module_worker: bool,
    /// Install hash-pinned no-import Wasm experts (requires wasm-experts).
    #[arg(long, global = true, conflicts_with = "typed_module_worker")]
    wasm_expert: Vec<PathBuf>,
    /// Optional isolated secondary verifier; requires a wasm-verifier feature build.
    #[arg(long, global = true, requires = "verifier_wasm_sha256")]
    verifier_wasm: Option<PathBuf>,
    #[arg(long, global = true, requires = "verifier_wasm")]
    verifier_wasm_sha256: Option<String>,
    /// Explicit operator policy for task-driven local model lifecycle.
    #[arg(long, global = true)]
    native_lifecycle_policy: Option<PathBuf>,
    #[arg(long, global = true, requires = "native_lifecycle_policy")]
    approve_native_lifecycle: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    #[command(hide = true)]
    ModuleWorker {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    #[command(hide = true)]
    AgentWorker {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    #[command(hide = true)]
    InferenceWorker {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    #[command(hide = true)]
    ArithmeticWorker {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    #[command(hide = true)]
    SqlWorker {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    Init,
    #[command(hide = true)]
    GpuNvmlWorker,
    /// Publish a new immutable version of an operator-declared project profile.
    ProjectPublish {
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Inspect one explicitly selected id@version profile without modifying state.
    ProjectShow {
        reference: String,
    },
    ProjectList,
    /// Publish a bounded schema-based project-memory revision; never imports a transcript.
    ProjectMemoryPublish {
        manifest: PathBuf,
    },
    /// Read schema-based memory revisions for an exact project profile.
    ProjectMemoryShow {
        reference: String,
    },
    /// Inspect explicit pairwise contradictions in a memory draft without publishing it.
    ProjectMemoryConflicts {
        manifest: PathBuf,
    },
    Status,
    SelfTest,
    SecurityEval,
    SecurityCorpusEvaluate {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        sha256: String,
    },
    /// Inspect reviewed retry evidence without initializing runtime state.
    RetryStudyEvaluate {
        study: PathBuf,
        #[arg(long)]
        thresholds: PathBuf,
    },
    WorkflowStudyEvaluate {
        study: PathBuf,
        #[arg(long)]
        thresholds: PathBuf,
    },
    Diagnostics {
        #[arg(long)]
        probe_inference: bool,
        #[arg(long)]
        probe_gpu: bool,
        /// Select the GPU sensor explicitly; NVML requires the nvml-gpu build feature.
        #[arg(long, requires = "probe_gpu", value_parser = ["nvidia-smi", "nvml"])]
        gpu_sensor: Option<String>,
        #[arg(long)]
        probe_registered_models: bool,
    },
    ResumePreview {
        id: String,
        #[arg(long, requires = "retry_thresholds")]
        retry_study: Option<PathBuf>,
        #[arg(long, requires = "retry_study")]
        retry_thresholds: Option<PathBuf>,
    },
    NativeResumePreview {
        id: String,
        #[arg(long)]
        policy: PathBuf,
    },
    ResumeTask {
        id: String,
        #[arg(long)]
        human_approve: bool,
        /// Reconstruct the fixed builtin worker from matching recorded sources.
        #[arg(long)]
        restore_builtin_worker: bool,
        /// Reconstruct the original pinned Wasm experts from operator manifests.
        #[arg(long, conflicts_with_all = ["restore_builtin_worker", "native_policy"])]
        restore_wasm_expert: Vec<PathBuf>,
        /// Restore the original approved native model/instance policy.
        #[arg(long, conflicts_with = "restore_builtin_worker")]
        native_policy: Option<PathBuf>,
        #[arg(long, requires = "retry_thresholds")]
        retry_study: Option<PathBuf>,
        #[arg(long, requires = "retry_study")]
        retry_thresholds: Option<PathBuf>,
    },
    RecoverRevisionResult {
        id: String,
        #[arg(long)]
        human_approve: bool,
    },
    RecoverAdvisoryResult {
        id: String,
        #[arg(long)]
        human_approve: bool,
    },
    /// Dispatch only a checkpointed terminal language request within its original budget.
    ResumeLanguagePresentation {
        id: String,
        #[arg(long)]
        human_approve: bool,

        /// Original immutable native lifecycle policy; instance bindings must still match.
        #[arg(long, requires = "human_approve")]
        native_policy: Option<PathBuf>,
    },
    /// Restore an already published final language response without model replay.
    RecoverLanguageResult {
        id: String,
    },
    RecoverVerifiedResult {
        id: String,
    },
    RecoverTasks,
    RecoverWorkers,
    WorkerFilesPreview {
        #[arg(long, default_value_t = 0)]
        after: u64,
        #[arg(long, default_value_t = 128)]
        limit: u32,
    },
    WorkerFilesApply {
        plan: PathBuf,
        #[arg(long)]
        human_approve: bool,
    },
    StorageStatus,
    /// Checkpoint and truncate the WAL without evicting active readers.
    StorageCheckpoint,
    WatchTask {
        id: String,
        #[arg(long, default_value = "0")]
        after: u64,
    },
    Backup {
        #[arg(long)]
        include_rag_artifacts: bool,
    },
    RestoreDatabase {
        snapshot: PathBuf,
        #[arg(long)]
        checksum: String,
        #[arg(long)]
        destination: PathBuf,
        #[arg(long)]
        restore_configuration: bool,
        #[arg(long)]
        restore_rag_artifacts: bool,
        #[arg(long)]
        migrate_schema: bool,
    },
    RetentionPreview {
        #[arg(long)]
        before_unix: u64,
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    RetentionApply {
        plan: PathBuf,
        #[arg(long)]
        human_approve: bool,
    },
    TaskProgress {
        id: String,
        #[arg(long, default_value = "0")]
        after: u64,
        #[arg(long)]
        socket: bool,
    },
    CancelTask {
        id: String,
    },
    ResidencyLoad {
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "2048")]
        context_length: u32,
        /// Explicit KV-cache placement request; this does not control model-layer placement.
        #[arg(long)]
        offload_kv_cache_to_gpu: Option<bool>,
        #[arg(long)]
        human_approve: bool,
    },
    ResidencyUnload {
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
        #[arg(long)]
        instance_id: String,
        #[arg(long)]
        human_approve: bool,
    },
    /// Run a task with an explicitly approved native model lifecycle request.
    ResidencyRun {
        request: PathBuf,
        #[arg(long)]
        human_approve: bool,
    },
    /// Acknowledge independently stopped unknown native task inference.
    ResidencyTaskResolve {
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
        #[arg(long)]
        task_id: String,
        #[arg(long)]
        human_approve: bool,
        #[arg(long)]
        provider_stop_acknowledged: bool,
    },
    /// Load a model and retain an auditable managed-instance receipt.
    ResidencyManagedLoad {
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "2048")]
        context_length: u32,
        /// Require provider-echoed and observed KV-cache GPU/CPU placement.
        #[arg(long)]
        offload_kv_cache_to_gpu: Option<bool>,
        #[arg(long)]
        human_approve: bool,
    },
    /// Release only the instance bound to one active managed-load receipt.
    ResidencyManagedRelease {
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
        #[arg(long)]
        receipt_id: String,
        #[arg(long)]
        human_approve: bool,
    },
    /// Resolve an unknown operation without repeating its provider mutation.
    ResidencyManagedResolve {
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
        #[arg(long)]
        task_id: String,
        #[arg(long)]
        human_approve: bool,
        #[arg(long)]
        provider_stop_acknowledged: bool,
        #[arg(long)]
        retire_stale_cohort: bool,
    },
    /// Retire stale release authority after the operator confirms an empty provider.
    ResidencyManagedAbandon {
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
        #[arg(long)]
        receipt_id: String,
        #[arg(long)]
        human_approve: bool,
        #[arg(long)]
        provider_stop_acknowledged: bool,
    },
    /// Archive a terminal receipt while retaining its immutable evidence.
    ResidencyManagedArchive {
        #[arg(long)]
        receipt_id: String,
        #[arg(long)]
        human_approve: bool,
    },
    /// Inspect one current or archived managed receipt without provider access.
    ResidencyManagedInspect {
        #[arg(long)]
        receipt_id: String,
    },
    /// Inspect local managed receipts without contacting the provider.
    ResidencyManagedStatus {
        #[arg(long)]
        api_base: String,
    },
    ResidencyStatus {
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
    },
    /// Propose model reuse/load/release without mutating the provider.
    ResidencyPlan {
        request: PathBuf,
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
    },
    /// Apply one reviewed residency plan under explicit operator approval.
    ResidencyApply {
        plan: PathBuf,
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
        #[arg(long)]
        human_approve: bool,
    },
    ResidencyInspect {
        id: String,
        #[arg(long)]
        api_base: String,
        #[arg(long)]
        authenticate: bool,
    },
    Models,
    ModelRegister {
        manifest: PathBuf,
    },
    ModelUse {
        id: String,
        version: String,
    },
    ModelEnable {
        id: String,
        version: String,
    },
    ModelDisable {
        id: String,
        version: String,
    },
    Skills,
    SkillRegister {
        manifest: PathBuf,
    },
    SkillActivationPlan {
        changes: PathBuf,
    },
    SkillActivationApply {
        changes: PathBuf,
        #[arg(long)]
        registry_sha256: String,
    },
    SkillActivate {
        id: String,
        version: String,
    },
    /// Atomically disable an explicitly selected set without cascading.
    SkillDisableSet {
        #[arg(required = true, num_args = 1..=256)]
        ids: Vec<String>,
    },
    SkillDisable {
        id: String,
    },
    SkillHealth,
    SkillLookup {
        capability: String,
        #[arg(long, default_value = "*")]
        requirement: String,
    },
    ConfigureLocal {
        #[arg(long)]
        model: String,
        #[arg(long)]
        api_base: Option<String>,
        /// Select an inference role, or `language` to reserve this model for interpretation/presentation only.
        #[arg(long)]
        role: Option<String>,
        #[arg(long, default_value = "0.15")]
        temperature: f64,
        #[arg(long)]
        interface_tokens: Option<usize>,
        #[arg(long, value_parser = ["default", "none", "low", "medium", "high"])]
        reasoning_effort: Option<String>,
        /// Per-call provider deadline; the root task wall limit still applies.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=300))]
        timeout_seconds: Option<u64>,
        /// Maximum concurrent dependency-ready reasoning or expert workflow steps.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=4))]
        parallelism: Option<u64>,
        /// Concurrent independent experts; 1 restores sequential selection.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=4))]
        expert_parallelism: Option<u64>,
    },
    /// Replace the role map with a language-only model and reasoning model network.
    ConfigureNetwork {
        #[arg(long)]
        language_model: String,
        #[arg(long)]
        reasoning_model: String,
        /// Optional repeated ROLE=MODEL overrides: planner, solver, explorer, critic, reflector.
        #[arg(long = "role", value_name = "ROLE=MODEL")]
        roles: Vec<String>,
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u64).range(1..=4))]
        parallelism: u64,
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u64).range(1..=4))]
        expert_parallelism: u64,
    },
    /// Print an unapproved on-demand native policy draft for configured network routes.
    NativePolicyDraft {
        /// Native provider API endpoint, for example http://127.0.0.1:1234/api/v1.
        #[arg(long)]
        api_base: String,
        /// Additional exact model keys for independently registered experts.
        #[arg(long = "extra-model")]
        extra_models: Vec<String>,
        /// Explicit context length applied to each configured route.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=32768))]
        context_length: u32,
        /// Optional explicit KV-cache placement for every model.
        #[arg(long, action = clap::ArgAction::Set)]
        offload_kv_cache_to_gpu: Option<bool>,
        /// Use the separately configured native provider credential.
        #[arg(long)]
        authenticate: bool,
        /// Maximum scope lifetime in seconds.
        #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..=1800))]
        max_lifecycle_seconds: u64,
    },
    ConfigureAuth {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    /// Opt the client-facing cogfabric-agent endpoint into bounded Jev+ / Jev-R orchestration.
    ConfigureAgentCollective {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    /// Pin an immutable operator project profile to newly created agent sessions.
    ConfigureAgentProject {
        #[arg(long, required_unless_present = "clear", conflicts_with = "clear")]
        profile: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    ConfigShow,
    ResourceStatus,
    ResourceRelease {
        id: String,
        #[arg(long)]
        human_approve: bool,
    },
    /// Set a reviewed shared inference budget; restart running services to apply it.
    ConfigureResources {
        #[arg(required_unless_present = "disable", conflicts_with = "disable")]
        configuration: Option<PathBuf>,
        #[arg(long)]
        disable: bool,
    },
    ConfigureToolWorker {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    ConfigureSqlWorker {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    /// Set Linux kernel limits for managed worker processes, not external model servers.
    /// Enable exact wire-token admission through the native llama.cpp API.
    ConfigureInputTokens {
        #[arg(long, conflicts_with_all = ["max_input_tokens", "max_context_tokens", "authenticate"])]
        disable: bool,
        #[arg(long, required_unless_present = "disable")]
        max_input_tokens: Option<u64>,
        #[arg(long, required_unless_present = "disable")]
        max_context_tokens: Option<u32>,
        /// Use the separately scoped COGFABRIC_LLAMA_API_KEY for native endpoints.
        #[arg(long)]
        authenticate: bool,
    },
    ConfigureWorkerLimits {
        #[arg(long, conflicts_with_all = ["address_space_bytes", "cpu_seconds"])]
        clear: bool,
        #[arg(long, required_unless_present = "clear")]
        address_space_bytes: Option<u64>,
        #[arg(long, required_unless_present = "clear")]
        cpu_seconds: Option<u64>,
    },
    ConfigureWorker {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    Ask {
        query: String,
        #[arg(long, default_value = "standard")]
        mode: String,
        #[arg(long)]
        socket: bool,
        #[arg(long)]
        conversation: Option<String>,
        /// Select an immutable project profile using id@version.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        explain: bool,
        /// Use reviewed workflow utility evidence for this direct task.
        #[arg(long, requires = "workflow_thresholds", conflicts_with = "socket")]
        workflow_study: Option<PathBuf>,
        #[arg(long, requires = "workflow_study", conflicts_with = "socket")]
        workflow_thresholds: Option<PathBuf>,
    },
    Serve {
        /// Pin reviewed workflow utility evidence for this service lifetime.
        #[arg(long, requires = "workflow_thresholds")]
        workflow_study: Option<PathBuf>,
        #[arg(long, requires = "workflow_study")]
        workflow_thresholds: Option<PathBuf>,
    },
    ServiceStatus,
    Modules {
        #[arg(long)]
        socket: bool,
    },
    InferenceModules {
        #[arg(long)]
        socket: bool,
    },
    ToolModules {
        #[arg(long)]
        socket: bool,
    },
    CancelActive,
    Stop,
    SkillAdd {
        id: String,
        #[arg(long)]
        description: String,
        #[arg(long)]
        tags: String,
    },
    Ingest {
        file: PathBuf,
        #[arg(long)]
        skill: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        license: String,
    },
    Search {
        query: String,
        #[arg(long)]
        skill: Option<String>,
    },
    MemoryPublish {
        task: String,
        #[arg(long, default_value = "episode")]
        kind: String,
        #[arg(long)]
        note: String,
    },
    MemorySearch {
        query: String,
    },
    MemoryRevoke {
        id: String,
    },
    Task {
        id: String,
    },
    /// Retire an interrupted client activation intent after explicit provider stop acknowledgement.
    ResolveClientActivation {
        id: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        human_approve: bool,
        #[arg(long)]
        provider_stop_acknowledged: bool,
    },
    TaskEvents {
        id: String,
        #[arg(long, default_value = "0")]
        after: u64,
    },
    GraphAudit {
        id: String,
    },
    InferenceAudit {
        id: String,
    },
    Bench {
        file: PathBuf,
        #[arg(long, default_value = "fast")]
        mode: String,
        #[arg(long, default_value_t = evaluation::DEFAULT_BENCHMARK_RUN_SECONDS)]
        max_run_seconds: u64,
    },
    Compare {
        file: PathBuf,
        /// Observe shared-device GPU metrics during each execution window.
        #[arg(long)]
        observe_gpu: bool,
        #[arg(long, requires = "observe_gpu", value_parser = ["nvidia-smi", "nvml"])]
        gpu_sensor: Option<String>,
        #[arg(long, default_value = "deep")]
        mode: String,
        #[arg(long)]
        larger_model: Option<String>,
        #[arg(long)]
        source: String,
        #[arg(long)]
        license: String,
        #[arg(long)]
        code_revision: String,
        #[arg(long)]
        dataset_sha256: Option<String>,
        #[arg(long, conflicts_with = "workflow_check")]
        held_out: bool,
        #[arg(long, conflicts_with = "held_out")]
        workflow_check: bool,
        #[arg(long, default_value = "1800")]
        max_run_seconds: u64,
    },
    /// Compare optional structured progress stopping against the same collective without it.
    CompareProgress {
        file: PathBuf,
        #[arg(long, default_value = "research")]
        mode: String,
        #[arg(long, default_value_t = 2)]
        plateau_cycles: u32,
        #[arg(long)]
        source: String,
        #[arg(long)]
        license: String,
        #[arg(long)]
        code_revision: String,
        #[arg(long)]
        dataset_sha256: Option<String>,
        #[arg(long, conflicts_with = "workflow_check")]
        held_out: bool,
        #[arg(long, conflicts_with = "held_out")]
        workflow_check: bool,
        #[arg(long, default_value_t = 1800)]
        max_run_seconds: u64,
    },
    Candidate {
        #[command(subcommand)]
        action: CandidateAction,
    },
}
#[derive(Subcommand)]
enum CandidateAction {
    List,
    CreatePrepared {
        #[arg(long)]
        skill: String,
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        report_sha256: String,
        #[arg(long)]
        license: String,
    },
    Provenance {
        id: String,
    },
    /// Fetch hash-pinned HTTPS text sources into an unreviewed local bundle.
    FetchSources {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        sha256: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Draft remote source choices from a reviewed catalog and recorded gap.
    DiscoverRemoteSources {
        task: String,
        #[arg(long)]
        capability: String,
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        sha256: String,
        #[arg(long, default_value_t = 8)]
        max_sources: usize,
        #[arg(long)]
        output: PathBuf,
    },
    DiscoverSources {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        license: String,
        #[arg(long)]
        output: PathBuf,
    },
    PrepareSources {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        sha256: String,
        #[arg(long)]
        output: PathBuf,
    },
    CreateFromGap {
        task: String,
        #[arg(long)]
        capability: String,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        license: String,
    },
    Gaps {
        task: String,
    },
    PrepareGap {
        task: String,
        #[arg(long)]
        capability: String,
    },
    Create {
        #[arg(long)]
        skill: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        license: String,
    },
    Evaluate {
        id: String,
        #[arg(long)]
        suite: String,
        #[arg(long)]
        dataset: Option<PathBuf>,
        #[arg(long)]
        dataset_source: Option<String>,
        #[arg(long)]
        dataset_license: Option<String>,
        #[arg(long)]
        held_out: bool,
    },
    Promote {
        id: String,
        #[arg(long)]
        human_approve: bool,
    },
    Rollback {
        id: String,
        #[arg(long)]
        human_approve: bool,
    },
}

fn valid_skill_id(id: &str) -> bool {
    id.len() >= 3
        && id.len() <= 101
        && id.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c))
}
fn output<T: Serialize>(value: T, force_json: bool) -> Result<()> {
    if force_json {
        println!("{}", serde_json::to_string_pretty(&value)?)
    } else {
        let value = serde_json::to_value(value)?;
        match value {
            Value::String(s) => println!("{s}"),
            v => println!("{}", serde_json::to_string_pretty(&v)?),
        }
    }
    Ok(())
}

fn gpu_sensor_choice(sensor: Option<&str>) -> cogfabric::gpu::Sensor {
    match sensor {
        Some("nvml") => cogfabric::gpu::Sensor::Nvml,
        _ => cogfabric::gpu::Sensor::NvidiaSmi,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let result = run_cli().await;
    // Cancellation drops request futures, but their workers still own process
    // and lease cleanup. Keep the runtime alive until bounded reapers finish.
    let agent_cleanup = cogfabric::agent::process::drain_workers().await;
    let collective_cleanup = cogfabric::fabric::worker_cleanup::drain_workers().await;
    result?;
    agent_cleanup?;
    collective_cleanup?;
    Ok(())
}

async fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    if matches!(&cli.command, Command::GpuNvmlWorker) {
        return cogfabric::gpu::nvml::run_worker();
    }
    if let Command::InferenceAudit { id } = &cli.command {
        // Observation must not load executable providers or initialize runtime state.
        let cfg = Config::load(cli.home.clone(), cli.backend.clone())?;
        let store = Store::open_existing_read_only(&cfg.db())?;
        return output(store.inference_audit(id)?, cli.json);
    }
    if !cli.wasm_expert.is_empty() {
        if cli.wasm_expert.len() > 8
            || !matches!(&cli.command, Command::Ask { .. } | Command::Serve { .. })
        {
            bail!("At most eight Wasm expert manifests are supported for ask and serve")
        }
        #[cfg(not(feature = "wasm-experts"))]
        bail!("Wasm experts require a build with the wasm-experts feature");
    }
    if cli.native_lifecycle_policy.is_some() {
        if !cli.approve_native_lifecycle {
            bail!("Native lifecycle policy requires explicit operator approval")
        }
        if !matches!(&cli.command, Command::Ask { .. } | Command::Serve { .. }) {
            bail!("Native lifecycle policy is supported only for ask and serve")
        }
        if cli.typed_module_worker {
            bail!("Set typed_module_worker inside the approved native lifecycle policy")
        }
    }
    if let Command::Candidate {
        action:
            CandidateAction::FetchSources {
                manifest,
                sha256,
                output: directory,
            },
    } = &cli.command
    {
        let acquired = cogfabric::evolution::acquisition::acquire(manifest, sha256).await?;
        return output(&acquired.write(directory)?, cli.json);
    }
    if let Command::Candidate {
        action:
            CandidateAction::DiscoverSources {
                root,
                source,
                license,
                output: directory,
            },
    } = &cli.command
    {
        let draft = cogfabric::evolution::discovery::discover(root, source, license)?;
        return output(&draft.write(directory)?, cli.json);
    }
    if let Command::Candidate {
        action:
            CandidateAction::PrepareSources {
                root,
                manifest,
                sha256,
                output: directory,
            },
    } = &cli.command
    {
        let prepared = cogfabric::evolution::preparation::prepare(root, manifest, sha256)?;
        return output(&prepared.write(directory)?, cli.json);
    }
    if let Command::SecurityCorpusEvaluate { manifest, sha256 } = &cli.command {
        return output(
            cogfabric::security_evaluation::corpus::evaluate_file(manifest, sha256)?,
            cli.json,
        );
    }
    if let Command::WorkflowStudyEvaluate { study, thresholds } = &cli.command {
        return output(
            cogfabric::routing::empirical::evaluate_files(study, thresholds)?,
            cli.json,
        );
    }
    if matches!(
        &cli.command,
        Command::ResidencyApply {
            human_approve: false,
            ..
        }
    ) {
        bail!("Residency plan application requires explicit operator approval")
    }
    if matches!(
        &cli.command,
        Command::ResidencyManagedLoad {
            human_approve: false,
            ..
        } | Command::ResidencyManagedRelease {
            human_approve: false,
            ..
        } | Command::ResidencyManagedResolve {
            human_approve: false,
            ..
        } | Command::ResidencyManagedAbandon {
            human_approve: false,
            ..
        } | Command::ResidencyManagedArchive {
            human_approve: false,
            ..
        } | Command::ResidencyRun {
            human_approve: false,
            ..
        } | Command::ResidencyTaskResolve {
            human_approve: false,
            ..
        }
    ) {
        bail!("Managed residency changes require explicit operator approval")
    }
    if matches!(
        &cli.command,
        Command::ResidencyManagedResolve {
            provider_stop_acknowledged: false,
            ..
        } | Command::ResidencyManagedAbandon {
            provider_stop_acknowledged: false,
            ..
        } | Command::ResidencyTaskResolve {
            provider_stop_acknowledged: false,
            ..
        }
    ) {
        bail!("Managed resolution requires explicit provider-stop acknowledgement")
    }
    if let Command::ResidencyPlan {
        request,
        api_base,
        authenticate,
    } = &cli.command
    {
        let bytes = security::read_regular_bytes(request, 16384)?;
        let request = serde_json::from_value(cogfabric::strict_json::value(&bytes, 16384)?)?;
        return output(
            cogfabric::residency::planning::preview(api_base, *authenticate, &request).await?,
            cli.json,
        );
    }
    let residency_plan = if let Command::ResidencyApply { plan, .. } = &cli.command {
        let bytes = security::read_regular_bytes(plan, 32768)?;
        let plan: cogfabric::residency::planning::Plan =
            serde_json::from_value(cogfabric::strict_json::value(&bytes, 32768)?)?;
        plan.validate()?;
        Some(plan)
    } else {
        None
    };
    if matches!(
        &cli.command,
        Command::ResourceRelease {
            human_approve: false,
            ..
        }
    ) {
        bail!("Resource reconciliation requires explicit provider-stop acknowledgement")
    }
    if matches!(
        &cli.command,
        Command::ResumeTask {
            human_approve: false,
            ..
        }
    ) {
        bail!("Task continuation requires explicit operator approval")
    }
    if let Command::ResumeTask {
        restore_wasm_expert,
        ..
    } = &cli.command
    {
        if restore_wasm_expert.len() > 8 {
            bail!("At most eight original Wasm expert manifests may be restored")
        }
        #[cfg(not(feature = "wasm-experts"))]
        if !restore_wasm_expert.is_empty() {
            bail!("Wasm expert restoration requires a build with the wasm-experts feature")
        }
    }
    let restored_policy: Option<Arc<dyn cogfabric::escalation::EscalationPolicy>> =
        match &cli.command {
            Command::ResumePreview {
                retry_study: Some(study),
                retry_thresholds: Some(thresholds),
                ..
            }
            | Command::ResumeTask {
                retry_study: Some(study),
                retry_thresholds: Some(thresholds),
                ..
            } => Some(Arc::new(cogfabric::escalation::empirical::load_files(
                study, thresholds,
            )?)),
            _ => None,
        };
    if let Command::RetryStudyEvaluate { study, thresholds } = &cli.command {
        return output(
            cogfabric::escalation::empirical::evaluate_files(study, thresholds)?,
            cli.json,
        );
    }
    if let Command::ModuleWorker { socket, host_pid } = &cli.command {
        return cogfabric::fabric::modules::worker_entrypoint(socket, *host_pid).await;
    }
    if let Command::AgentWorker { socket, host_pid } = &cli.command {
        let home = cli
            .home
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Worker requires an explicit home"))?;
        return cogfabric::agent::process::run_worker(home, socket, *host_pid).await;
    }
    if let Command::InferenceWorker { socket, host_pid } = &cli.command {
        let home = cli
            .home
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Worker requires an explicit home"))?;
        return cogfabric::fabric::unix::process::run_worker(home, socket, *host_pid).await;
    }
    if let Command::ArithmeticWorker { socket, host_pid }
    | Command::SqlWorker { socket, host_pid } = &cli.command
    {
        let home = cli
            .home
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Tool worker requires an explicit home"))?;
        return cogfabric::fabric::tools::unix::process::run_worker(
            home,
            socket,
            *host_pid,
            matches!(&cli.command, Command::SqlWorker { .. }),
        )
        .await;
    }
    if matches!(
        &cli.command,
        Command::RecoverAdvisoryResult {
            human_approve: false,
            ..
        }
    ) {
        bail!("Advisory recovery requires explicit operator approval")
    }
    if matches!(
        &cli.command,
        Command::RecoverRevisionResult {
            human_approve: false,
            ..
        }
    ) {
        bail!("Revision recovery requires explicit operator approval")
    }
    // Resolve explicit policy data before creating state or constructing any backend.
    // These immutable bytes are never read again while this host accepts tasks.
    let workflow_router: Option<Arc<dyn cogfabric::routing::ExpertRouter>> = match &cli.command {
        Command::Ask {
            workflow_study: Some(study),
            workflow_thresholds: Some(thresholds),
            ..
        }
        | Command::Serve {
            workflow_study: Some(study),
            workflow_thresholds: Some(thresholds),
        } => Some(Arc::new(cogfabric::routing::empirical::load_files(
            study, thresholds,
        )?)),
        _ => None,
    };
    let verifier_provider: Option<Arc<dyn cogfabric::verification::VerifierProvider>> =
        match (&cli.verifier_wasm, &cli.verifier_wasm_sha256) {
            (Some(path), Some(digest)) => {
                security::authorize("human-cli", "expert.module.evaluate")?;
                #[cfg(feature = "wasm-verifier")]
                {
                    Some(Arc::new(
                        cogfabric::verification::wasm::WasmVerifierProvider::load(
                            path,
                            digest,
                            "wasm.verifier",
                            "1.0.0",
                        )?,
                    ))
                }
                #[cfg(not(feature = "wasm-verifier"))]
                {
                    let _ = (path, digest);
                    bail!("Wasm verifier support requires a build with the wasm-verifier feature")
                }
            }
            (None, None) => None,
            _ => bail!("Wasm verifier requires both a module file and its exact SHA-256 pin"),
        };
    #[cfg(feature = "wasm-experts")]
    let wasm_experts = cli
        .wasm_expert
        .iter()
        .map(|path| cogfabric::fabric::modules::wasm::WasmModule::load_manifest(path))
        .collect::<Result<Vec<_>>>()?;
    #[cfg(feature = "wasm-experts")]
    let restored_wasm_experts = match &cli.command {
        Command::ResumeTask {
            restore_wasm_expert,
            ..
        } if !restore_wasm_expert.is_empty() => Some(
            restore_wasm_expert
                .iter()
                .map(|path| cogfabric::fabric::modules::wasm::WasmModule::load_manifest(path))
                .collect::<Result<Vec<_>>>()?,
        ),
        _ => None,
    };
    let native_lifecycle = cli
        .native_lifecycle_policy
        .as_ref()
        .map(cogfabric::residency::lifecycle::NativeLifecyclePolicy::load)
        .transpose()?;
    let project_draft = if let Command::ProjectPublish { manifest } = &cli.command {
        security::authorize("human-cli", "project.publish")?;
        let bytes = security::read_regular_bytes(manifest, 8192)?;
        let draft: cogfabric::projects::ProjectDraft =
            serde_json::from_value(cogfabric::strict_json::value(&bytes, 8192)?)?;
        draft.validate()?;
        Some(draft)
    } else {
        None
    };
    if let Command::Ask {
        project: Some(reference),
        ..
    } = &cli.command
    {
        cogfabric::projects::Selection::parse(reference)?;
    }
    let cfg = Config::load(cli.home, cli.backend)?;
    if let Command::ConfigureAgentCollective { enabled } = &cli.command {
        security::authorize("human-cli", "resources.configure")?;
        let mut changed = cfg.clone();
        changed.agent_collective = *enabled;
        changed.save()?;
        return output(
            json!({
                "agent_collective_enabled": changed.agent_collective,
                "running_services_require_restart": true,
                "model_switch": false
            }),
            cli.json,
        );
    }
    if let Command::ConfigureAgentProject { profile, clear } = &cli.command {
        security::authorize("human-cli", "resources.configure")?;
        let mut changed = cfg.clone();
        if *clear {
            changed.agent_project_profile = None;
        } else {
            let reference = profile
                .as_deref()
                .ok_or_else(|| anyhow!("Specify --profile id@version or --clear"))?;
            cogfabric::projects::Selection::parse(reference)?;
            let store = Store::open_existing_read_only(&changed.db())?;
            store.project_profile(reference)?;
            changed.agent_project_profile = Some(reference.to_owned());
        }
        changed.save()?;
        return output(
            json!({
                "agent_project_profile": changed.agent_project_profile,
                "new_sessions_only": true,
                "running_services_require_restart": true
            }),
            cli.json,
        );
    }
    match &cli.command {
        Command::Candidate {
            action:
                CandidateAction::DiscoverRemoteSources {
                    task,
                    capability,
                    catalog,
                    sha256,
                    max_sources,
                    output: directory,
                },
        } => {
            let store = Store::open_existing_read_only(&cfg.db())?;
            let draft = cogfabric::evolution::catalog::discover(
                &store,
                task,
                capability,
                catalog,
                sha256,
                *max_sources,
            )?;
            return output(draft.write(directory)?, cli.json);
        }
        Command::StorageStatus => {
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(store.storage_stats()?, cli.json);
        }
        Command::StorageCheckpoint => {
            security::authorize("human-cli", "storage.checkpoint")?;
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(store.checkpoint_wal()?, cli.json);
        }
        Command::ProjectMemoryConflicts { manifest } => {
            security::authorize("human-cli", "project.read")?;
            let bytes = security::read_regular_bytes(
                manifest,
                cogfabric::project_memory::MAX_RECORD_BYTES,
            )?;
            let draft: cogfabric::project_memory::ProjectMemoryDraft = serde_json::from_value(
                cogfabric::strict_json::value(&bytes, cogfabric::project_memory::MAX_RECORD_BYTES)?,
            )?;
            let record = draft.into_record()?;
            return output(
                json!({
                    "format_version": 1,
                    "project_ref": record.project_ref,
                    "revision": record.revision,
                    "scope": "explicit_pairwise_scalar_contradictions_only",
                    "semantic_completeness_proven": false,
                    "published": false,
                    "conflicts": record.conflicts()?
                }),
                cli.json,
            );
        }
        Command::ProjectMemoryPublish { manifest } => {
            security::authorize("human-cli", "project.publish")?;
            let bytes = security::read_regular_bytes(
                manifest,
                cogfabric::project_memory::MAX_RECORD_BYTES,
            )?;
            let draft: cogfabric::project_memory::ProjectMemoryDraft = serde_json::from_value(
                cogfabric::strict_json::value(&bytes, cogfabric::project_memory::MAX_RECORD_BYTES)?,
            )?;
            let record = draft.into_record()?;
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(store.publish_project_memory(&record)?, cli.json);
        }
        Command::ProjectMemoryShow { reference } => {
            let store = Store::open_existing_read_only(&cfg.db())?;
            return output(store.project_memories(reference)?, cli.json);
        }
        Command::ProjectShow { reference } => {
            cogfabric::projects::Selection::parse(reference)?;
            let store = Store::open_existing_read_only(&cfg.db())?;
            return output(store.project_profile(reference)?, cli.json);
        }
        Command::ProjectList => {
            let store = Store::open_existing_read_only(&cfg.db())?;
            return output(store.projects()?, cli.json);
        }
        Command::ProjectPublish { .. } => {
            cfg.prepare()?;
            let store = Store::open_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(
                store.publish_project(project_draft.expect("Validated project draft"))?,
                cli.json,
            );
        }
        _ => {}
    }
    match &cli.command {
        Command::ResidencyRun {
            request,
            human_approve,
        } => {
            let bytes = security::read_regular_bytes(request, 16384)?;
            let request = serde_json::from_value(cogfabric::strict_json::value(&bytes, 16384)?)?;
            let store = Arc::new(Store::open_existing_with_limit(
                &cfg.db(),
                cfg.max_database_bytes,
            )?);
            return output(
                cogfabric::residency::lifecycle::run_with_verifier(
                    cfg.clone(),
                    store,
                    request,
                    *human_approve,
                    verifier_provider.clone(),
                )
                .await?,
                cli.json,
            );
        }
        Command::ResidencyTaskResolve {
            api_base,
            authenticate,
            task_id,
            human_approve,
            provider_stop_acknowledged,
        } => {
            let store = Arc::new(Store::open_existing_with_limit(
                &cfg.db(),
                cfg.max_database_bytes,
            )?);
            return output(
                cogfabric::residency::lifecycle::resolve_generation(
                    store,
                    api_base,
                    *authenticate,
                    task_id,
                    *human_approve,
                    *provider_stop_acknowledged,
                )
                .await?,
                cli.json,
            );
        }
        Command::ResidencyManagedLoad {
            api_base,
            authenticate,
            model,
            context_length,
            offload_kv_cache_to_gpu,
            human_approve,
        } => {
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(
                cogfabric::residency::managed::load_with_gpu(
                    &store,
                    api_base,
                    *authenticate,
                    model,
                    *context_length,
                    *offload_kv_cache_to_gpu,
                    *human_approve,
                )
                .await?,
                cli.json,
            );
        }
        Command::ResidencyManagedRelease {
            api_base,
            authenticate,
            receipt_id,
            human_approve,
        } => {
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(
                cogfabric::residency::managed::release(
                    &store,
                    api_base,
                    *authenticate,
                    receipt_id,
                    *human_approve,
                )
                .await?,
                cli.json,
            );
        }
        Command::ResidencyManagedResolve {
            api_base,
            authenticate,
            task_id,
            human_approve,
            provider_stop_acknowledged,
            retire_stale_cohort,
        } => {
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(
                cogfabric::residency::managed::resolution::resolve(
                    &store,
                    api_base,
                    *authenticate,
                    task_id,
                    cogfabric::residency::managed::resolution::Options {
                        human_approve: *human_approve,
                        provider_stop_acknowledged: *provider_stop_acknowledged,
                        retire_stale_cohort: *retire_stale_cohort,
                    },
                )
                .await?,
                cli.json,
            );
        }
        Command::ResidencyManagedAbandon {
            api_base,
            authenticate,
            receipt_id,
            human_approve,
            provider_stop_acknowledged,
        } => {
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(
                cogfabric::residency::managed::abandonment::abandon(
                    &store,
                    api_base,
                    *authenticate,
                    receipt_id,
                    *human_approve,
                    *provider_stop_acknowledged,
                )
                .await?,
                cli.json,
            );
        }
        Command::ResidencyManagedArchive {
            receipt_id,
            human_approve,
        } => {
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            return output(
                cogfabric::residency::managed::retirement::archive(
                    &store,
                    receipt_id,
                    *human_approve,
                )?,
                cli.json,
            );
        }
        Command::ResidencyManagedInspect { receipt_id } => {
            let store = Store::open_existing_read_only(&cfg.db())?;
            return output(
                cogfabric::residency::managed::inspect(&store, receipt_id)?,
                cli.json,
            );
        }
        Command::ResidencyManagedStatus { api_base } => {
            let store = Store::open_existing_read_only(&cfg.db())?;
            return output(
                cogfabric::residency::managed::list(&store, api_base)?,
                cli.json,
            );
        }
        _ => {}
    }
    if let Command::ResidencyApply {
        api_base,
        authenticate,
        human_approve,
        ..
    } = &cli.command
    {
        let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
        let report = cogfabric::residency::planning::apply(
            &store,
            api_base,
            *authenticate,
            residency_plan.as_ref().expect("Validated plan"),
            *human_approve,
        )
        .await?;
        output(&report, cli.json)?;
        if report["state"] != "completed" {
            bail!("Residency plan interrupted; inspect confirmed and uncertain operations")
        }
        return Ok(());
    }
    if matches!(
        &cli.command,
        Command::ResourceStatus | Command::ResourceRelease { .. }
    ) {
        if let Command::ResourceRelease { id, human_approve } = &cli.command {
            let store = Store::open_existing_with_limit(&cfg.db(), cfg.max_database_bytes)?;
            cogfabric::resource_scheduler::ResourcePool::release_abandoned(
                "human-cli",
                &store,
                id,
                *human_approve,
            )?;
            return output(
                json!({"released":id,"operator_acknowledged_stop":true,"provider_stop_proven":false}),
                cli.json,
            );
        }
        let store = Store::open_existing_read_only(&cfg.db())?;
        return output(
            cogfabric::resource_scheduler::ResourcePool::abandoned_status("human-cli", &store)?,
            cli.json,
        );
    }
    if let Command::ConfigureInputTokens {
        disable,
        max_input_tokens,
        max_context_tokens,
        authenticate,
    } = &cli.command
    {
        security::authorize("human-cli", "inference.tokens.configure")?;
        let mut changed = cfg.clone();
        changed.input_token_admission = if *disable {
            None
        } else {
            Some(cogfabric::token_admission::Settings {
                format_version: 1,
                max_input_tokens: max_input_tokens.context("Missing task input-token ceiling")?,
                max_context_tokens: max_context_tokens
                    .context("Missing per-call context ceiling")?,
                authenticate: *authenticate,
            })
        };
        changed.save()?;
        return output(
            json!({"input_token_admission":changed.input_token_admission,"native_llamacpp_endpoints":true,"running_services_require_restart":true,"weights_attested":false}),
            cli.json,
        );
    }
    if let Command::ConfigureResources {
        configuration,
        disable,
    } = &cli.command
    {
        security::authorize("human-cli", "resources.configure")?;
        let mut changed = cfg.clone();
        changed.resource_admission = if *disable {
            None
        } else {
            let path = configuration
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Resource configuration file required"))?;
            let bytes = security::read_regular_bytes(path, 16_384)?;
            let value = cogfabric::strict_json::value(&bytes, 16_384)?;
            let configuration: cogfabric::resource_scheduler::Configuration =
                serde_json::from_value(value)?;
            configuration.validate()?;
            Some(configuration)
        };
        changed.save()?;
        return output(
            json!({"resource_admission":changed.resource_admission,"running_services_require_restart":true,"physical_limits_enforced":false}),
            cli.json,
        );
    }
    if let Command::ResidencyInspect {
        id,
        api_base,
        authenticate,
    } = &cli.command
    {
        let store = Store::open_existing_read_only(&cfg.db())?;
        return output(
            cogfabric::residency::inspect_operation(&store, id, api_base, *authenticate).await?,
            cli.json,
        );
    }
    if let Command::RestoreDatabase {
        snapshot,
        checksum,
        destination,
        restore_configuration,
        restore_rag_artifacts,
        migrate_schema,
    } = &cli.command
    {
        output(
            &cogfabric::restore::database_with_migration(
                &cfg,
                snapshot,
                checksum,
                destination,
                *restore_configuration,
                *restore_rag_artifacts,
                *migrate_schema,
            )?,
            cli.json,
        )?;
        return Ok(());
    }
    if let Command::Candidate {
        action: CandidateAction::Provenance { id },
    } = &cli.command
    {
        let store = Store::open_existing_read_only(&cfg.db())?;
        return output(
            &cogfabric::evolution::prepared::provenance(&store, id)?,
            cli.json,
        );
    }
    if let Command::NativeResumePreview { id, policy } = &cli.command {
        let policy = cogfabric::residency::lifecycle::NativeLifecyclePolicy::load(policy)?;
        let store = Arc::new(Store::open_existing_read_only(&cfg.db())?);
        let report =
            cogfabric::residency::lifecycle::restoration::inspect(store, id, &policy).await?;
        output(&report, cli.json)?;
        return Ok(());
    }
    if let Command::ResumePreview { id, .. } = &cli.command {
        let store = Store::open_existing_read_only(&cfg.db())?;
        let report = match (restored_policy, verifier_provider.clone()) {
            (Some(policy), Some(provider)) => {
                cogfabric::recovery::continuation::preview_with_restored_policies(
                    &store, id, policy, provider,
                )?
            }
            (Some(policy), None) => {
                cogfabric::recovery::continuation::preview_with_escalation_policy(
                    &store, id, policy,
                )?
            }
            (None, Some(provider)) => {
                cogfabric::recovery::continuation::preview_with_verifier_provider(
                    &store, id, provider,
                )?
            }
            (None, None) => cogfabric::recovery::continuation::preview(&store, id)?,
        };
        output(&report, cli.json)?;
        return Ok(());
    }
    if let Command::ResumeTask {
        id,
        human_approve,
        restore_builtin_worker,
        native_policy,
        ..
    } = &cli.command
    {
        let store = Arc::new(Store::open_existing_with_limit(
            &cfg.db(),
            cfg.max_database_bytes,
        )?);
        let cancel = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        if let Some(path) = native_policy {
            let policy = cogfabric::residency::lifecycle::NativeLifecyclePolicy::load(path)?;
            let runtime = cogfabric::recovery::continuation::RuntimeRestoration {
                escalation_policy: restored_policy,
                verifier: verifier_provider.clone(),
                modules: None,
            };
            let report = cogfabric::recovery::continuation::resume_native_with_runtime(
                store,
                id,
                *human_approve,
                &policy,
                runtime,
                cancel,
            )
            .await?;
            output(&report, cli.json)?;
            return Ok(());
        }
        if *restore_builtin_worker {
            let modules = cogfabric::recovery::continuation::reconstruct_builtin_registry(
                store.clone(),
                id,
                *human_approve,
            )?;
            let runtime = cogfabric::recovery::continuation::RuntimeRestoration {
                escalation_policy: restored_policy,
                verifier: verifier_provider.clone(),
                modules: Some(modules),
            };
            let report = cogfabric::recovery::continuation::resume_with_runtime(
                store,
                id,
                *human_approve,
                runtime,
                cancel,
            )
            .await?;
            output(&report, cli.json)?;
            return Ok(());
        }
        #[cfg(feature = "wasm-experts")]
        if let Some(experts) = restored_wasm_experts {
            let modules = cogfabric::recovery::continuation::reconstruct_wasm_registry(
                store.clone(),
                id,
                *human_approve,
                experts,
            )?;
            let runtime = cogfabric::recovery::continuation::RuntimeRestoration {
                escalation_policy: restored_policy,
                verifier: verifier_provider.clone(),
                modules: Some(modules),
            };
            let report = cogfabric::recovery::continuation::resume_with_runtime(
                store,
                id,
                *human_approve,
                runtime,
                cancel,
            )
            .await?;
            output(&report, cli.json)?;
            return Ok(());
        }
        let report = match (restored_policy, verifier_provider.clone()) {
            (Some(policy), Some(provider)) => {
                cogfabric::recovery::continuation::resume_with_restored_policies(
                    store,
                    id,
                    *human_approve,
                    policy,
                    provider,
                    cancel,
                )
                .await?
            }
            (Some(policy), None) => {
                cogfabric::recovery::continuation::resume_with_escalation_policy(
                    store,
                    id,
                    *human_approve,
                    policy,
                    cancel,
                )
                .await?
            }
            (None, Some(provider)) => {
                cogfabric::recovery::continuation::resume_with_verifier_provider(
                    store,
                    id,
                    *human_approve,
                    provider,
                    cancel,
                )
                .await?
            }
            (None, None) => {
                cogfabric::recovery::continuation::resume(store, id, *human_approve, cancel).await?
            }
        };
        output(&report, cli.json)?;
        return Ok(());
    }
    cfg.prepare()?;
    let store = Arc::new(Store::open_with_limit(&cfg.db(), cfg.max_database_bytes)?);
    store::bootstrap(&store)?;
    let engine = Engine::new(cfg.clone(), store.clone());
    let engine = match verifier_provider {
        Some(provider) => engine.with_verifier_provider(provider)?,
        None => engine,
    };
    let engine = if cli.typed_module_worker {
        engine.with_builtin_module_worker()?
    } else {
        engine
    };
    #[cfg(feature = "wasm-experts")]
    for module in wasm_experts {
        engine.modules.register(
            Arc::new(module),
            cogfabric::fabric::modules::Limits::default(),
        )?;
    }
    let engine = match workflow_router {
        Some(router) => engine.with_expert_router(router)?,
        None => engine,
    };
    let engine = match native_lifecycle {
        Some(policy) => {
            engine.with_approved_native_lifecycle(policy, cli.approve_native_lifecycle)?
        }
        None => engine,
    };
    let response: Value = match cli.command {
        Command::RetryStudyEvaluate { .. } => {
            bail!("Retry study evaluation must run before runtime initialization")
        }
        Command::ModuleWorker { .. }
        | Command::AgentWorker { .. }
        | Command::InferenceWorker { .. }
        | Command::ArithmeticWorker { .. }
        | Command::SqlWorker { .. } => {
            bail!("Worker must run before normal initialization")
        }
        Command::RestoreDatabase { .. } => {
            bail!("Restore must run before workspace initialization")
        }
        Command::WatchTask { id, after } => {
            let mut subscription = service::subscribe_progress(&cfg, &id, after).await?;
            loop {
                let update = tokio::select! {
                    _=tokio::signal::ctrl_c()=>break,
                    update=subscription.receive()=>update?,
                };
                let Some(update) = update else { break };
                if cli.json {
                    println!("{}", serde_json::to_string(&update)?);
                } else {
                    output(&update, false)?;
                }
            }
            return Ok(());
        }
        Command::Diagnostics {
            probe_inference,
            probe_gpu,
            gpu_sensor,
            probe_registered_models,
        } => {
            let mut report =
                cogfabric::diagnostics::diagnose(&cfg, &store, probe_inference).await?;
            report["gpu_telemetry"] = if probe_gpu {
                cogfabric::gpu::sample_with(gpu_sensor_choice(gpu_sensor.as_deref())).await?
            } else {
                serde_json::json!({"status":"not_requested"})
            };
            report["registered_model_discovery"] = if probe_registered_models {
                cogfabric::diagnostics::registered_endpoints(&cfg, &store).await?
            } else {
                serde_json::json!({"status":"not_requested"})
            };
            if report["registered_model_discovery"]["status"] == "failed"
                || report["registered_selection_valid"] == false
                || report["inference_discovery"]["status"] == "failed"
                || report["inference_discovery"]["all_configured_models_advertised"] == false
            {
                output(&report, cli.json)?;
                bail!("Diagnostics found an invalid selection or unavailable configured inference model")
            }
            report
        }
        Command::TaskProgress { id, after, socket } => serde_json::to_value(if socket {
            service::task_progress(&cfg, &id, after).await?
        } else {
            store.progress_since(&id, after)?
        })?,
        Command::CancelTask { id } => {
            json!({"task_id":id,"accepted":service::cancel_task(&cfg,&id).await?})
        }
        Command::StorageStatus | Command::StorageCheckpoint => {
            bail!("Storage maintenance must run before runtime initialization")
        }
        Command::Backup {
            include_rag_artifacts,
        } => serde_json::to_value(cogfabric::backups::create_with_rag(
            &cfg,
            &store,
            include_rag_artifacts,
        )?)?,
        Command::RetentionPreview { before_unix, limit } => {
            serde_json::to_value(store.retention_preview(before_unix, limit)?)?
        }
        Command::RetentionApply {
            plan,
            human_approve,
        } => {
            let bytes = security::read_regular_bytes(&plan, 128000)?;
            let plan: cogfabric::storage::RetentionPlan = serde_json::from_slice(&bytes)?;
            json!({"audit_id":store.apply_retention(&plan,human_approve)?,"deleted_tasks":plan.task_ids.len(),"deleted_events":plan.event_count})
        }
        Command::Models => serde_json::to_value(store.models()?)?,
        Command::ModelRegister { manifest } => {
            serde_json::to_value(store.register_model(&manifest)?)?
        }
        Command::ModelUse { id, version } => {
            let settings =
                store.model_configuration(&cfg, &cogfabric::models::Selection { id, version })?;
            settings.save()?;
            json!({"saved":true,"configuration":settings,"server_weights_attested":false})
        }
        Command::ModelEnable { id, version } => {
            store.set_model_enabled(&cogfabric::models::Selection { id, version }, true)?;
            json!({"enabled":true})
        }
        Command::ModelDisable { id, version } => {
            store.set_model_enabled(&cogfabric::models::Selection { id, version }, false)?;
            json!({"enabled":false})
        }
        Command::RecoverAdvisoryResult { id, human_approve } => {
            store.recover_advisory_result(&id, human_approve)?
        }
        Command::RecoverRevisionResult { id, human_approve } => {
            store.recover_revision_result(&id, human_approve)?
        }
        Command::ResumeLanguagePresentation {
            id,
            human_approve,
            native_policy,
        } => {
            let cancel = async {
                let _ = tokio::signal::ctrl_c().await;
            };
            if let Some(path) = native_policy {
                let policy = cogfabric::residency::lifecycle::NativeLifecyclePolicy::load(path)?;
                cogfabric::recovery::language_presentation::resume_native(
                    store.clone(),
                    &id,
                    human_approve,
                    &policy,
                    cancel,
                )
                .await?
            } else {
                cogfabric::recovery::language_presentation::resume(
                    store.clone(),
                    &id,
                    human_approve,
                    cancel,
                )
                .await?
            }
        }
        Command::RecoverLanguageResult { id } => store.recover_language_result(&id)?,
        Command::RecoverVerifiedResult { id } => store.recover_verified_result(&id)?,
        Command::RecoverTasks => {
            json!({"interrupted_tasks":store.recover_tasks()?,"automatic_replay":false})
        }
        Command::RecoverWorkers => {
            json!({"interrupted_worker_sessions":store.recover_workers()?,"automatic_replay":false,"external_inference_stopped_verified":false})
        }
        Command::WorkerFilesPreview { after, limit } => {
            serde_json::to_value(store.preview_worker_files(after, limit)?)?
        }
        Command::WorkerFilesApply {
            plan,
            human_approve,
        } => {
            let bytes = security::read_regular_bytes(&plan, 256_000)?;
            let plan: cogfabric::recovery::worker_files::CleanupPlan =
                serde_json::from_slice(&bytes)?;
            json!({"removed_directories":store.clean_worker_files(&plan, human_approve)?,"processes_killed":false})
        }
        Command::Init => {
            json!({"home":cfg.home,"version":env!("CARGO_PKG_VERSION"),"skills":store.skills()?.len()})
        }
        Command::Status => {
            json!({"home":cfg.home,"backend":cfg.backend,"model":cfg.model,"inference_process":cfg.inference_process,
            "resource_admission":cfg.resource_admission,"agent_collective_enabled":cfg.agent_collective,
            "agent_project_profile":cfg.agent_project_profile,
            "skills":store.skills()?.len(),"candidates":store.candidates()?.len()})
        }
        Command::ConfigureInputTokens { .. } => {
            bail!("Token admission configuration must run before runtime initialization")
        }
        Command::ConfigureResources { .. } => {
            bail!("Resource configuration must run before initialization")
        }
        Command::ConfigureAgentCollective { .. } => {
            bail!("Agent collective configuration must run before runtime initialization")
        }
        Command::ConfigureAgentProject { .. } => {
            bail!("Agent project configuration must run before runtime initialization")
        }
        Command::ConfigureLocal {
            model,
            api_base,
            role,
            temperature,
            interface_tokens,
            reasoning_effort,
            timeout_seconds,
            parallelism,
            expert_parallelism,
        } => {
            let mut settings = cfg.clone();
            settings.backend = "openai-local".into();
            settings.model_registration = None;
            if let Some(tokens) = interface_tokens {
                settings.interface_tokens = tokens;
            }
            if let Some(effort) = reasoning_effort {
                settings.reasoning_effort = (effort != "default").then_some(effort);
            }
            if let Some(seconds) = timeout_seconds {
                settings.timeout_seconds = seconds;
            }
            if let Some(cap) = parallelism {
                settings.subtask_parallelism = Some(cap as usize);
            }
            if let Some(cap) = expert_parallelism {
                settings.expert_parallelism = (cap > 1).then_some(cap as usize);
            }
            if let Some(endpoint) = api_base {
                settings.api_base = endpoint;
            }
            if let Some(role) = role {
                settings
                    .role_profiles
                    .insert(role, cogfabric::config::RoleProfile { model, temperature });
            } else {
                settings.model = model;
                settings.temperature = temperature;
            }
            settings.save()?;
            json!({"saved":true,"configuration":settings})
        }
        Command::ConfigureNetwork {
            language_model,
            reasoning_model,
            roles,
            parallelism,
            expert_parallelism,
        } => {
            let mut bindings = std::collections::BTreeMap::new();
            if roles.len() > 5 {
                bail!("At most five reasoning role overrides are supported");
            }
            for assignment in roles {
                let (role, model) = assignment.split_once('=').ok_or_else(|| {
                    anyhow::anyhow!("Reasoning role binding must have ROLE=MODEL form")
                })?;
                if bindings.insert(role.to_owned(), model.to_owned()).is_some() {
                    bail!("Duplicate reasoning role binding")
                }
            }
            let settings = cfg.language_network(
                language_model,
                reasoning_model,
                bindings,
                parallelism as usize,
                expert_parallelism as usize,
            )?;
            settings.save()?;
            let routes = [
                "interface",
                "planner",
                "solver",
                "explorer",
                "critic",
                "reflector",
                "synthesizer",
            ]
            .into_iter()
            .map(|role| {
                settings
                    .model_settings(role)
                    .map(|(model, _)| json!({"role":role,"model":model}))
            })
            .collect::<Result<Vec<_>>>()?;
            json!({"saved":true,"routes":routes,"parallelism":parallelism,
                "expert_parallelism":expert_parallelism,"client_collective":true,
                "models_loaded":false,"trained_weights_created":false,
                "verification":"Independent compiled kernel; no verifier-model override",
                "active_sessions_reconfigured":false})
        }
        Command::NativePolicyDraft {
            api_base,
            extra_models,
            context_length,
            offload_kv_cache_to_gpu,
            authenticate,
            max_lifecycle_seconds,
        } => serde_json::to_value(
            cogfabric::residency::lifecycle::NativeLifecyclePolicy::draft_for_config(
                &cfg,
                api_base,
                extra_models,
                context_length,
                offload_kv_cache_to_gpu,
                authenticate,
                max_lifecycle_seconds,
            )?,
        )?,
        Command::ConfigureAuth { enabled } => {
            security::authorize("human-cli", "inference.auth.configure")?;
            let mut settings = cfg.clone();
            settings.inference_auth_scope = enabled.then(|| cfg.api_base.clone());
            settings.save()?;
            json!({"saved":true,"inference_auth_scope":settings.inference_auth_scope,"credential_environment_variable":cogfabric::credentials::ENVIRONMENT_VARIABLE,"credential_stored":false})
        }
        Command::ConfigShow => serde_json::to_value(&cfg)?,
        Command::GpuNvmlWorker => bail!("GPU worker must run before normal initialization"),
        Command::ProjectPublish { .. }
        | Command::ProjectShow { .. }
        | Command::ProjectList
        | Command::ProjectMemoryPublish { .. }
        | Command::ProjectMemoryShow { .. }
        | Command::ProjectMemoryConflicts { .. } => {
            bail!("Project commands must run before engine initialization")
        }
        Command::SecurityCorpusEvaluate { .. } => {
            bail!("Security corpus evaluation must precede state initialization")
        }
        Command::WorkflowStudyEvaluate { .. } => {
            bail!("Workflow study inspection must precede state initialization")
        }
        Command::ResidencyPlan { .. }
        | Command::ResidencyApply { .. }
        | Command::ResidencyManagedLoad { .. }
        | Command::ResidencyRun { .. }
        | Command::ResidencyTaskResolve { .. }
        | Command::ResidencyManagedRelease { .. }
        | Command::ResidencyManagedStatus { .. }
        | Command::ResidencyManagedInspect { .. }
        | Command::ResidencyManagedResolve { .. }
        | Command::ResidencyManagedAbandon { .. }
        | Command::ResidencyManagedArchive { .. } => {
            bail!("Residency plans must run before runtime initialization")
        }
        Command::ResourceStatus | Command::ResourceRelease { .. } => {
            bail!("Resource reconciliation must run before initialization")
        }
        Command::ConfigureToolWorker { enabled } => {
            security::authorize("human-cli", "tool.worker.configure")?;
            let mut changed = cfg.clone();
            changed.arithmetic_process = enabled;
            changed.save()?;
            json!({"arithmetic_process":enabled,"sandboxed":false})
        }
        Command::ConfigureSqlWorker { enabled } => {
            security::authorize("human-cli", "tool.worker.configure")?;
            let mut changed = cfg.clone();
            changed.sql_process = enabled;
            changed.save()?;
            json!({"sql_process":enabled,"sandboxed":false})
        }
        Command::ConfigureWorkerLimits {
            clear,
            address_space_bytes,
            cpu_seconds,
        } => {
            security::authorize("human-cli", "workers.limits.configure")?;
            let mut changed = cfg.clone();
            changed.worker_process_limits = if clear {
                None
            } else {
                let limits = cogfabric::process_limits::Limits {
                    address_space_bytes: address_space_bytes
                        .context("Missing address-space bound")?,
                    cpu_seconds: cpu_seconds.context("Missing CPU-seconds bound")?,
                };
                limits.validate()?;
                Some(limits)
            };
            changed.save()?;
            json!({"worker_process_limits":changed.worker_process_limits,"running_services_require_restart":true,"external_provider_limited":false,"sandboxed":false})
        }
        Command::ConfigureWorker { enabled } => {
            security::authorize("human-cli", "inference.worker.configure")?;
            let mut changed = cfg.clone();
            changed.inference_process = enabled;
            changed.save()?;
            json!({"inference_process":enabled,"sandboxed":false})
        }
        Command::ResumePreview { .. } => bail!("Preview must run before runtime initialization"),
        Command::NativeResumePreview { .. } => {
            bail!("Native preview must run before runtime initialization")
        }
        Command::ResumeTask { .. } => bail!("Continuation must run before runtime initialization"),
        Command::ResidencyLoad {
            api_base,
            authenticate,
            model,
            context_length,
            offload_kv_cache_to_gpu,
            human_approve,
        } => {
            cogfabric::residency::mutate(
                &store,
                &api_base,
                authenticate,
                cogfabric::residency::Mutation::Load {
                    model,
                    context_length,
                    offload_kv_cache_to_gpu,
                },
                human_approve,
            )
            .await?
        }
        Command::ResidencyUnload {
            api_base,
            authenticate,
            instance_id,
            human_approve,
        } => {
            cogfabric::residency::mutate(
                &store,
                &api_base,
                authenticate,
                cogfabric::residency::Mutation::Unload { instance_id },
                human_approve,
            )
            .await?
        }
        Command::ResidencyStatus {
            api_base,
            authenticate,
        } => cogfabric::residency::status(&api_base, authenticate).await?,
        Command::ResidencyInspect {
            id,
            api_base,
            authenticate,
        } => cogfabric::residency::inspect_operation(&store, &id, &api_base, authenticate).await?,
        Command::SecurityEval => {
            let report = cogfabric::security_evaluation::run()?;
            let success = report["success"] == true;
            output(report, cli.json)?;
            if !success {
                anyhow::bail!("Security boundary evaluation failed")
            }
            return Ok(());
        }
        Command::SelfTest => {
            let math = engine.run("calc: (7 + 5) * 3", "fast").await?;
            let wire = cogfabric::wire::new_request("calc: 7 * 6".into(), "fast".into());
            let binary = cogfabric::wire::decode(&cogfabric::wire::encode(&wire)?)?;
            json!({"math_ok":math.answer=="36","wire_ok":matches!(binary,cogfabric::wire::Message::Ask{..}),
                "protocol":"flatbuffers/v1","backend":cfg.backend})
        }
        Command::Ask {
            query,
            mode,
            socket,
            conversation,
            project,
            explain,
            ..
        } => {
            let options = cogfabric::language::TaskOptions {
                conversation_ref: conversation,
                project_ref: project,
                explain: explain || cfg.synthesize_results,
            };
            let outcome = if socket {
                tokio::select! {
                    result = service::request_with_options(&cfg, query, mode,options) => result?,
                    _ = tokio::signal::ctrl_c() => bail!("Request cancelled; the service cancels on disconnect"),
                }
            } else {
                engine
                    .run_with_options(&query, &mode, options, async {
                        let _ = tokio::signal::ctrl_c().await;
                    })
                    .await?
            };
            let unsuccessful = matches!(
                outcome.status.as_str(),
                "failed" | "rejected" | "cancelled" | "exhausted"
            );
            let status = outcome.status.clone();
            if unsuccessful {
                if cli.json {
                    output(&outcome, true)?;
                } else {
                    output(&outcome.answer, false)?;
                }
                bail!("Task ended with status: {status}")
            }
            if cli.json {
                serde_json::to_value(outcome)?
            } else {
                json!(outcome.answer)
            }
        }
        Command::Serve { .. } => {
            service::serve(engine).await?;
            return Ok(());
        }
        Command::ServiceStatus => {
            let (active, active_task_id) = service::status(&cfg).await?;
            json!({"active":active,"accepted":true,"active_task_id":active_task_id})
        }
        Command::CancelActive | Command::Stop => {
            let action = match cli.command {
                Command::CancelActive => cogfabric::wire::ControlAction::CancelActive,
                _ => cogfabric::wire::ControlAction::Stop,
            };
            let (active, accepted) = service::control(&cfg, action).await?;
            json!({"active":active,"accepted":accepted})
        }
        Command::Skills => serde_json::to_value(store.skills()?)?,
        Command::Modules { socket } => serde_json::to_value(if socket {
            service::modules(&cfg).await?
        } else {
            engine.broker.discover()?
        })?,
        Command::InferenceModules { socket } => serde_json::to_value(if socket {
            service::inference_modules(&cfg).await?
        } else {
            engine.broker.statuses()?
        })?,
        Command::ToolModules { socket } => serde_json::to_value(if socket {
            service::tool_modules(&cfg).await?
        } else {
            engine.tools.statuses()?
        })?,
        Command::SkillRegister { manifest } => {
            cogfabric::registry::register(&store, &cogfabric::registry::read_manifest(&manifest)?)?
        }
        Command::SkillActivationPlan { changes } => {
            let bytes = security::read_regular_bytes(&changes, 32768)?;
            let changes: Vec<cogfabric::registry::ActivationChange> =
                serde_json::from_value(cogfabric::strict_json::value(&bytes, 32768)?)?;
            cogfabric::registry::activation_plan(&store, &changes)?
        }
        Command::SkillActivationApply {
            changes,
            registry_sha256,
        } => {
            let bytes = security::read_regular_bytes(&changes, 32768)?;
            let changes: Vec<cogfabric::registry::ActivationChange> =
                serde_json::from_value(cogfabric::strict_json::value(&bytes, 32768)?)?;
            cogfabric::registry::apply_activation_plan(&store, &changes, &registry_sha256)?
        }
        Command::SkillActivate { id, version } => {
            cogfabric::registry::activate(&store, &id, &version)?
        }
        Command::SkillDisableSet { ids } => cogfabric::registry::disable_set(&store, &ids)?,
        Command::SkillDisable { id } => cogfabric::registry::disable(&store, &id)?,
        Command::SkillHealth => cogfabric::registry::health(&store)?,
        Command::SkillLookup {
            capability,
            requirement,
        } => cogfabric::registry::lookup(&store, &capability, &requirement)?,
        Command::SkillAdd {
            id,
            description,
            tags,
        } => {
            if !valid_skill_id(&id) || description.trim().is_empty() || description.len() > 500 {
                bail!("Invalid skill ID or description")
            }
            let tags = tags
                .split(',')
                .map(|t| t.trim().to_ascii_lowercase())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>();
            if tags.is_empty()
                || tags.len() > 32
                || tags.iter().any(|s| {
                    s.len() < 2
                        || s.len() > 40
                        || !s.bytes().all(|c| {
                            c.is_ascii_lowercase() || c.is_ascii_digit() || b"_+-".contains(&c)
                        })
                })
            {
                bail!("Invalid skill tags")
            }
            store.ensure_skill(&id, &description, &tags)?;
            json!({"id":id,"registered":true,"tags":tags})
        }
        Command::Ingest {
            file,
            skill,
            source,
            license,
        } => {
            security::validate_provenance(&source, &license)?;
            let body = security::read_reviewed_document(&file)?;
            if !store.skills()?.iter().any(|s| s.id == skill) {
                bail!("Unknown skill")
            }
            let id = uuid::Uuid::new_v4().to_string();
            let hash = store::sha256(body.as_bytes());
            let document = store::Document {
                id: id.clone(),
                skill_id: skill,
                title: file
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("document")
                    .into(),
                body,
                source,
                license,
            };
            let added = store.add_document(&document)?;
            json!({"added":added,"id":if added{Some(id)}else{None},"sha256":hash})
        }
        Command::Search { query, skill } => {
            serde_json::to_value(store.search(&query, skill.as_deref(), 10)?)?
        }
        Command::MemoryPublish { task, kind, note } => {
            serde_json::to_value(store.publish_memory(&task, &kind, &note)?)?
        }
        Command::MemorySearch { query } => serde_json::to_value(store.search_memories(&query)?)?,
        Command::MemoryRevoke { id } => {
            store.revoke_memory(&id)?;
            json!({"revoked":id})
        }
        Command::ResolveClientActivation {
            id,
            model,
            human_approve,
            provider_stop_acknowledged,
        } => cogfabric::agent::native_scope::NativeClientScope::acknowledge_stopped_activation(
            store.clone(),
            &cfg,
            &id,
            &model,
            human_approve,
            provider_stop_acknowledged,
        )?,
        Command::Task { id } => store.task(&id)?,
        Command::TaskEvents { id, after } => store.task_events_since(&id, after)?,
        Command::GraphAudit { id } => store.graph_audit(&id)?,
        Command::InferenceAudit { .. } => {
            bail!("Inference audit must run before runtime initialization")
        }
        Command::Bench {
            file,
            mode,
            max_run_seconds,
        } => {
            let report =
                evaluation::benchmark_until(&engine, &file, &mode, max_run_seconds, async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
            if report["complete"] != true {
                output(&report, cli.json)?;
                bail!("Benchmark interrupted or exceeded its run duration")
            }
            report
        }
        Command::Compare {
            file,
            observe_gpu,
            gpu_sensor,
            mode,
            larger_model,
            source,
            license,
            code_revision,
            dataset_sha256,
            held_out,
            workflow_check,
            max_run_seconds,
        } => {
            let report = cogfabric::benchmarking::compare_observed_with_sensor_until(
                &cfg,
                store.clone(),
                cogfabric::benchmarking::Comparison {
                    dataset: &file,
                    mode: &mode,
                    larger: larger_model.as_deref(),
                    review: &cogfabric::benchmarking::Review {
                        source,
                        license,
                        code_revision,
                        expected_dataset_sha256: dataset_sha256,
                        held_out,
                        workflow_check,
                        max_run_seconds,
                    },
                    gpu_observation: observe_gpu.then_some(Default::default()),
                },
                gpu_sensor_choice(gpu_sensor.as_deref()),
                async {
                    let _ = tokio::signal::ctrl_c().await;
                },
            )
            .await?;
            if report["complete"] != true {
                output(&report, cli.json)?;
                bail!("Comparison interrupted before all arms completed")
            }
            report
        }
        Command::CompareProgress {
            file,
            mode,
            plateau_cycles,
            source,
            license,
            code_revision,
            dataset_sha256,
            held_out,
            workflow_check,
            max_run_seconds,
        } => {
            let report = cogfabric::benchmarking::compare_progress_until(
                &cfg,
                store.clone(),
                &file,
                &mode,
                cogfabric::reasoning::progress::Settings { plateau_cycles },
                &cogfabric::benchmarking::Review {
                    source,
                    license,
                    code_revision,
                    expected_dataset_sha256: dataset_sha256,
                    held_out,
                    workflow_check,
                    max_run_seconds,
                },
                async {
                    let _ = tokio::signal::ctrl_c().await;
                },
            )
            .await?;
            if report["complete"] != true {
                output(&report, cli.json)?;
                bail!("Progress comparison interrupted before all arms completed")
            }
            report
        }
        Command::Candidate { action } => match action {
            CandidateAction::CreatePrepared {
                skill,
                bundle,
                report_sha256,
                license,
            } => serde_json::to_value(cogfabric::evolution::prepared::create(
                &cfg,
                &store,
                &skill,
                &bundle,
                &report_sha256,
                &license,
            )?)?,
            CandidateAction::Provenance { .. } => {
                bail!("Provenance inspection must precede runtime initialization")
            }
            CandidateAction::FetchSources { .. } => {
                bail!("Source acquisition must run before runtime initialization")
            }
            CandidateAction::DiscoverRemoteSources { .. } => {
                bail!("Catalog source discovery must precede runtime initialization")
            }
            CandidateAction::DiscoverSources { .. } => {
                bail!("Source discovery must precede runtime initialization")
            }
            CandidateAction::PrepareSources { .. } => {
                bail!("Source preparation must precede runtime initialization")
            }
            CandidateAction::List => serde_json::to_value(store.candidates()?)?,
            CandidateAction::CreateFromGap {
                task,
                capability,
                artifact,
                source,
                license,
            } => serde_json::to_value(cogfabric::evolution::create_from_gap(
                &cfg,
                &store,
                &task,
                &capability,
                &artifact,
                &source,
                &license,
            )?)?,
            CandidateAction::Gaps { task } => cogfabric::gaps::recorded(&store, &task)?,
            CandidateAction::PrepareGap { task, capability } => {
                serde_json::to_value(cogfabric::gaps::prepare(&store, &task, &capability)?)?
            }
            CandidateAction::Create {
                skill,
                kind,
                artifact,
                source,
                license,
            } => serde_json::to_value(cogfabric::evolution::create(
                &cfg, &store, &skill, &kind, &artifact, &source, &license,
            )?)?,
            CandidateAction::Evaluate {
                id,
                suite,
                dataset,
                dataset_source,
                dataset_license,
                held_out,
            } => {
                let review = if suite == "quality" {
                    Some(evaluation::DatasetReview {
                        source: dataset_source
                            .ok_or_else(|| anyhow::anyhow!("Quality requires --dataset-source"))?,
                        license: dataset_license
                            .ok_or_else(|| anyhow::anyhow!("Quality requires --dataset-license"))?,
                        held_out,
                    })
                } else {
                    None
                };
                evaluation::evaluate_reviewed(
                    &store,
                    &id,
                    &suite,
                    dataset.as_deref(),
                    review.as_ref(),
                )?
            }
            CandidateAction::Promote { id, human_approve } => {
                evaluation::promote(&store, &id, human_approve)?
            }
            CandidateAction::Rollback { id, human_approve } => {
                evaluation::rollback(&store, &id, human_approve)?
            }
        },
    };
    output(response, cli.json)
}
