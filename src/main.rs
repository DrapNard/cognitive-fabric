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
    Init,
    Status,
    SelfTest,
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
    },
    ConfigShow,
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
    Task {
        id: String,
    },
    Bench {
        file: PathBuf,
        #[arg(long, default_value = "fast")]
        mode: String,
    },
    Candidate {
        #[command(subcommand)]
        action: CandidateAction,
    },
}
#[derive(Subcommand)]
enum CandidateAction {
    List,
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
    let cfg = Config::load(cli.home, cli.backend)?;
    cfg.prepare()?;
    let store = Arc::new(Store::open(&cfg.db())?);
    store::bootstrap(&store)?;
    let engine = Engine::new(cfg.clone(), store.clone());
    let response: Value = match cli.command {
        Command::Init => {
            json!({"home":cfg.home,"version":env!("CARGO_PKG_VERSION"),"skills":store.skills()?.len()})
        }
        Command::Status => json!({"home":cfg.home,"backend":cfg.backend,"model":cfg.model,
            "skills":store.skills()?.len(),"candidates":store.candidates()?.len()}),
        Command::ConfigureLocal {
            model,
            api_base,
            role,
            temperature,
        } => {
            let mut settings = cfg.clone();
            settings.backend = "openai-local".into();
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
        Command::ConfigShow => serde_json::to_value(&cfg)?,
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
        Command::Skills => serde_json::to_value(store.skills()?)?,
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
        Command::Task { id } => store.task(&id)?,
        Command::Bench { file, mode } => evaluation::benchmark(&engine, &file, &mode).await?,
        Command::Candidate { action } => match action {
            CandidateAction::List => serde_json::to_value(store.candidates()?)?,
            CandidateAction::Create {
                skill,
                kind,
                artifact,
                source,
                license,
            } => {
                security::authorize("human-cli", "candidate.create")?;
                security::validate_provenance(&source, &license)?;
                let meta = std::fs::symlink_metadata(&artifact)?;
                if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 64_000_000 {
                    bail!("Invalid candidate artifact")
                }
                if kind == "rag" {
                    security::read_reviewed_document(&artifact)?;
                }
                let canonical = artifact.canonicalize()?;
                let checksum =
                    store::sha256(&security::read_regular_bytes(&canonical, 64_000_000)?);
                serde_json::to_value(
                    store.create_candidate(
                        &skill,
                        &kind,
                        canonical
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("Non-UTF8 candidate path"))?,
                        &checksum,
                        &source,
                        &license,
                    )?,
                )?
            }
            CandidateAction::Evaluate { id, suite, dataset } => {
                evaluation::evaluate(&store, &id, &suite, dataset.as_deref())?
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
