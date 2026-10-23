use anyhow::Result;
use clap::{Parser, Subcommand};
use cogfabric::{agent::AgentRuntime, config::Config, engine::Engine, store::Store};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
#[derive(Parser)]
#[command(about = "Loopback OpenAI-compatible server built on the Cognitive Fabric core")]
struct Args {
    #[arg(long)]
    home: Option<PathBuf>,
    #[arg(long, default_value = "127.0.0.1:8234")]
    bind: SocketAddr,
    /// Execute supported typed tools through the compiled same-image module worker.
    #[arg(long, global = true)]
    typed_module_worker: bool,
    /// Pin reviewed workflow utility evidence for this HTTP service lifetime.
    #[arg(long, requires = "workflow_thresholds")]
    workflow_study: Option<PathBuf>,
    #[arg(long, requires = "workflow_study")]
    workflow_thresholds: Option<PathBuf>,
    /// Explicit operator policy for task-driven local model lifecycle.
    #[arg(long, global = true)]
    native_lifecycle_policy: Option<PathBuf>,
    #[arg(long, global = true, requires = "native_lifecycle_policy")]
    approve_native_lifecycle: bool,
    #[command(subcommand)]
    worker: Option<Worker>,
}
// Managed collective tools relaunch the running image. Keep only their typed
// worker entry points here; all worker implementation remains in the core.
#[derive(Subcommand)]
enum Worker {
    #[command(hide = true, name = "module-worker")]
    Module {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    #[command(hide = true, name = "agent-worker")]
    Agent {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    #[command(hide = true, name = "inference-worker")]
    Inference {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    #[command(hide = true, name = "arithmetic-worker")]
    Arithmetic {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
    #[command(hide = true, name = "sql-worker")]
    Sql {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        host_pid: u32,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(worker) = args.worker {
        if args.workflow_study.is_some()
            || args.workflow_thresholds.is_some()
            || args.native_lifecycle_policy.is_some()
            || args.approve_native_lifecycle
            || args.typed_module_worker
        {
            anyhow::bail!("Policy options are not valid for worker commands")
        }
        if let Worker::Module { socket, host_pid } = &worker {
            return cogfabric::fabric::modules::worker_entrypoint(socket, *host_pid).await;
        }
        let home = args
            .home
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Worker requires an explicit home"))?;
        return match worker {
            Worker::Module { .. } => unreachable!("Module worker returned above"),
            Worker::Agent { socket, host_pid } => {
                cogfabric::agent::process::run_worker(home, &socket, host_pid).await
            }
            Worker::Inference { socket, host_pid } => {
                cogfabric::fabric::unix::process::run_worker(home, &socket, host_pid).await
            }
            Worker::Arithmetic { socket, host_pid } => {
                cogfabric::fabric::tools::unix::process::run_worker(home, &socket, host_pid, false)
                    .await
            }
            Worker::Sql { socket, host_pid } => {
                cogfabric::fabric::tools::unix::process::run_worker(home, &socket, host_pid, true)
                    .await
            }
        };
    }
    if args.native_lifecycle_policy.is_some() && !args.approve_native_lifecycle {
        anyhow::bail!("Native lifecycle policy requires explicit operator approval")
    }
    if args.native_lifecycle_policy.is_some() && args.typed_module_worker {
        anyhow::bail!("Set typed_module_worker inside the approved native lifecycle policy")
    }
    // Read and validate reviewed policy before configuration or runtime state
    // is loaded. HTTP requests cannot replace this pinned policy.
    let native_lifecycle = args
        .native_lifecycle_policy
        .as_ref()
        .map(cogfabric::residency::lifecycle::NativeLifecyclePolicy::load)
        .transpose()?;
    // Validate and pin the optional policy before loading or preparing state and
    // before constructing either inference backend. Requests cannot replace it.
    let workflow_router: Option<Arc<dyn cogfabric::routing::ExpertRouter>> = match (
        args.workflow_study.as_deref(),
        args.workflow_thresholds.as_deref(),
    ) {
        (Some(study), Some(thresholds)) => Some(Arc::new(
            cogfabric::routing::empirical::load_files(study, thresholds)?,
        )),
        (None, None) => None,
        _ => anyhow::bail!("Workflow study and thresholds must be provided together"),
    };
    cogfabric_server::validate_bind(args.bind)?;
    let token = std::env::var("COGFABRIC_SERVER_API_KEY").map_err(|_| {
        anyhow::anyhow!("Set a dedicated COGFABRIC_SERVER_API_KEY (32+ characters)")
    })?;
    let cfg = Config::load(args.home, None)?;
    cfg.prepare()?;
    let store = Arc::new(Store::open_with_limit(&cfg.db(), cfg.max_database_bytes)?);
    cogfabric::store::bootstrap(&store)?;
    let agent = AgentRuntime::local(cfg.clone(), store.clone())?;
    let engine = Engine::new(cfg, store);
    let engine = if args.typed_module_worker {
        engine.with_builtin_module_worker()?
    } else {
        engine
    };
    let engine = match workflow_router {
        Some(router) => engine.with_expert_router(router)?,
        None => engine,
    };
    let engine = match native_lifecycle {
        Some(policy) => {
            engine.with_approved_native_lifecycle(policy, args.approve_native_lifecycle)?
        }
        None => engine,
    };
    let server = cogfabric_server::Server::new(agent, engine, &token)?;
    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    eprintln!(
        "Cognitive Fabric HTTP API listening on {}",
        listener.local_addr()?
    );
    cogfabric_server::serve_until(server, listener, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}
