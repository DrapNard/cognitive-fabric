use anyhow::{bail, Result};
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
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
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
    Status,
    SelfTest,
    SecurityEval,
    Diagnostics {
        #[arg(long)]
        probe_inference: bool,
        #[arg(long)]
        probe_gpu: bool,
        #[arg(long)]
        probe_registered_models: bool,
    },
    ResumeTask {
        id: String,
        #[arg(long)]
        human_approve: bool,
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
    ResidencyStatus {
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
    SkillActivate {
        id: String,
        version: String,
    },
    SkillDisable {
        id: String,
    },
    SkillHealth,
    ConfigureLocal {
        #[arg(long)]
        model: String,
        #[arg(long)]
        api_base: Option<String>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long, default_value = "0.15")]
        temperature: f64,
        #[arg(long)]
        interface_tokens: Option<usize>,
        #[arg(long, value_parser = ["default", "none", "low", "medium", "high"])]
        reasoning_effort: Option<String>,
    },
    ConfigureAuth {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    ConfigShow,
    ConfigureToolWorker {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    ConfigureSqlWorker {
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
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
        #[arg(long)]
        explain: bool,
    },
    Serve,
    ServiceStatus,
    Modules {
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
    },
    Compare {
        file: PathBuf,
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
    Candidate {
        #[command(subcommand)]
        action: CandidateAction,
    },
}
#[derive(Subcommand)]
enum CandidateAction {
    List,
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
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
    let cfg = Config::load(cli.home, cli.backend)?;
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
    cfg.prepare()?;
    let store = Arc::new(Store::open_with_limit(&cfg.db(), cfg.max_database_bytes)?);
    store::bootstrap(&store)?;
    let engine = Engine::new(cfg.clone(), store.clone());
    let response: Value = match cli.command {
        Command::InferenceWorker { .. }
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
            probe_registered_models,
        } => {
            let mut report =
                cogfabric::diagnostics::diagnose(&cfg, &store, probe_inference).await?;
            report["gpu_telemetry"] = if probe_gpu {
                cogfabric::gpu::sample().await?
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
        Command::StorageStatus => serde_json::to_value(store.storage_stats()?)?,
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
            "skills":store.skills()?.len(),"candidates":store.candidates()?.len()})
        }
        Command::ConfigureLocal {
            model,
            api_base,
            role,
            temperature,
            interface_tokens,
            reasoning_effort,
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
        Command::ConfigureAuth { enabled } => {
            security::authorize("human-cli", "inference.auth.configure")?;
            let mut settings = cfg.clone();
            settings.inference_auth_scope = enabled.then(|| cfg.api_base.clone());
            settings.save()?;
            json!({"saved":true,"inference_auth_scope":settings.inference_auth_scope,"credential_environment_variable":cogfabric::credentials::ENVIRONMENT_VARIABLE,"credential_stored":false})
        }
        Command::ConfigShow => serde_json::to_value(&cfg)?,
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
        Command::ConfigureWorker { enabled } => {
            security::authorize("human-cli", "inference.worker.configure")?;
            let mut changed = cfg.clone();
            changed.inference_process = enabled;
            changed.save()?;
            json!({"inference_process":enabled,"sandboxed":false})
        }
        Command::ResumeTask { id, human_approve } => {
            cogfabric::recovery::continuation::resume(store.clone(), &id, human_approve, async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?
        }
        Command::ResidencyLoad {
            api_base,
            authenticate,
            model,
            context_length,
            human_approve,
        } => {
            cogfabric::residency::mutate(
                &store,
                &api_base,
                authenticate,
                cogfabric::residency::Mutation::Load {
                    model,
                    context_length,
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
            explain,
        } => {
            let options = cogfabric::language::TaskOptions {
                conversation_ref: conversation,
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
        Command::Serve => {
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
        Command::SkillRegister { manifest } => {
            cogfabric::registry::register(&store, &cogfabric::registry::read_manifest(&manifest)?)?
        }
        Command::SkillActivate { id, version } => {
            cogfabric::registry::activate(&store, &id, &version)?
        }
        Command::SkillDisable { id } => cogfabric::registry::disable(&store, &id)?,
        Command::SkillHealth => cogfabric::registry::health(&store)?,
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
        Command::Task { id } => store.task(&id)?,
        Command::TaskEvents { id, after } => store.task_events_since(&id, after)?,
        Command::GraphAudit { id } => store.graph_audit(&id)?,
        Command::InferenceAudit { id } => store.inference_audit(&id)?,
        Command::Bench { file, mode } => evaluation::benchmark(&engine, &file, &mode).await?,
        Command::Compare {
            file,
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
            let report = cogfabric::benchmarking::compare_until(
                &cfg,
                store.clone(),
                &file,
                &mode,
                larger_model.as_deref(),
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
                bail!("Comparison interrupted before all arms completed")
            }
            report
        }
        Command::Candidate { action } => match action {
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
