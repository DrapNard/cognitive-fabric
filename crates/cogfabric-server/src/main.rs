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
    #[command(subcommand)]
    worker: Option<Worker>,
}
// Managed collective tools relaunch the running image. Keep only their typed
// worker entry points here; all worker implementation remains in the core.
#[derive(Subcommand)]
enum Worker {
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
        let home = args
            .home
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Worker requires an explicit home"))?;
        return match worker {
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
    cogfabric_server::validate_bind(args.bind)?;
    let token = std::env::var("COGFABRIC_SERVER_API_KEY").map_err(|_| {
        anyhow::anyhow!("Set a dedicated COGFABRIC_SERVER_API_KEY (32+ characters)")
    })?;
    let cfg = Config::load(args.home, None)?;
    cfg.prepare()?;
    let store = Arc::new(Store::open_with_limit(&cfg.db(), cfg.max_database_bytes)?);
    cogfabric::store::bootstrap(&store)?;
    let agent = AgentRuntime::local(cfg.clone(), store.clone())?;
    let server = cogfabric_server::Server::new(agent, Engine::new(cfg, store), &token)?;
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
