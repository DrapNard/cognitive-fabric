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
    /// Pin reviewed workflow utility evidence for this HTTP service lifetime.
    #[arg(long, requires = "workflow_thresholds")]
    workflow_study: Option<PathBuf>,
    #[arg(long, requires = "workflow_study")]
    workflow_thresholds: Option<PathBuf>,
    #[command(subcommand)]
    worker: Option<Worker>,
}
// Managed collective tools relaunch the running image. Keep only their typed
// worker entry points here; all worker implementation remains in the core.
#[derive(Subcommand)]
enum Worker {
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
        if args.workflow_study.is_some() || args.workflow_thresholds.is_some() {
            anyhow::bail!("Workflow policy options are not valid for worker commands")
        }
        let home = args
            .home
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Worker requires an explicit home"))?;
        return match worker {
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
    let engine = match workflow_router {
        Some(router) => engine.with_expert_router(router)?,
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
