//! Single-owner local socket service with bounded handlers and cancellation.
use crate::{
    config::Config,
    engine::{Engine, Outcome},
    wire::{self, Message},
};
use anyhow::{bail, Result};
use std::{
    future::Future,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{watch, Semaphore},
    task::JoinSet,
};

struct SocketGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
}

struct Controls {
    stopping: watch::Sender<bool>,
    cancellation: watch::Sender<u64>,
    active: AtomicBool,
    execution: Semaphore,
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        // Never unlink a socket another process substituted during shutdown.
        if let Ok(meta) = std::fs::symlink_metadata(&self.path) {
            if meta.dev() == self.device && meta.ino() == self.inode {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

pub async fn serve(engine: Engine) -> Result<()> {
    serve_until(engine, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

/// Stop accepting, cancel in-flight tasks, join handlers, then remove our socket.
pub async fn serve_until<F>(engine: Engine, shutdown: F) -> Result<()>
where
    F: Future<Output = ()>,
{
    let socket = engine.cfg.socket();
    if let Ok(meta) = std::fs::symlink_metadata(&socket) {
        if !meta.file_type().is_socket() {
            bail!("Configured socket path is not a socket")
        }
        bail!("Socket exists; refusing to delete a possible active daemon socket");
    }
    let listener = UnixListener::bind(&socket)?;
    let meta = std::fs::symlink_metadata(&socket)?;
    let _guard = SocketGuard {
        path: socket.clone(),
        device: meta.dev(),
        inode: meta.ino(),
    };
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let owner = meta.uid();
    let engine = Arc::new(engine);
    let (cancel, _) = watch::channel(false);
    let controls = Arc::new(Controls {
        stopping: cancel.clone(),
        cancellation: watch::channel(0).0,
        active: AtomicBool::new(false),
        execution: Semaphore::new(1),
    });
    let mut remote_stop = cancel.subscribe();
    let mut handlers = JoinSet::new();
    tokio::pin!(shutdown);
    println!("Listening on {}", socket.display());
    let result = loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break Ok(()),
            _ = stopped(&mut remote_stop) => break Ok(()),
            Some(result) = handlers.join_next(), if !handlers.is_empty() => {
                if let Err(err) = result { eprintln!("Socket handler failed: {err}"); }
            }
            accepted = listener.accept() => {
                let (stream, _) = match accepted { Ok(value) => value, Err(err) => break Err(err.into()) };
                // OS credentials, never message-supplied actor names, establish the owner.
                if !stream.peer_cred().is_ok_and(|credentials| credentials.uid() == owner) || handlers.len() >= 8 {
                    drop(stream);
                    continue;
                }
                let engine = engine.clone();
                let stopping = cancel.subscribe();
                let controls = controls.clone();
                handlers.spawn(async move { handle(stream, engine, stopping, controls).await });
            }
        }
    };
    cancel.send_replace(true);
    drop(listener);
    // Each handler has bounded reads/writes; inference cancellation persists state.
    while let Some(joined) = handlers.join_next().await {
        if let Err(err) = joined {
            eprintln!("Socket handler failed during shutdown: {err}");
        }
    }
    result
}

async fn stopped(receiver: &mut watch::Receiver<bool>) {
    if *receiver.borrow() {
        return;
    }
    let _ = receiver.changed().await;
}

async fn handle(
    stream: UnixStream,
    engine: Arc<Engine>,
    mut stopping: watch::Receiver<bool>,
    controls: Arc<Controls>,
) {
    let (mut reader, mut writer) = stream.into_split();
    let request = tokio::select! {
        biased;
        _ = stopped(&mut stopping) => return,
        result = tokio::time::timeout(Duration::from_secs(10), wire::read_message(&mut reader)) => result,
    };
    let response = match request {
        Ok(Ok(Message::Control { id, action })) => {
            let active = controls.active.load(Ordering::Acquire);
            let accepted = match action {
                wire::ControlAction::Status => true,
                wire::ControlAction::Modules => true,
                wire::ControlAction::CancelActive => {
                    if active {
                        controls
                            .cancellation
                            .send_modify(|generation| *generation = generation.wrapping_add(1));
                    }
                    active
                }
                wire::ControlAction::Stop => {
                    controls.stopping.send_replace(true);
                    true
                }
            };
            match if action == wire::ControlAction::Modules {
                engine.broker.discover()
            } else {
                Ok(Vec::new())
            } {
                Ok(modules) => Message::ControlReply {
                    id,
                    active,
                    accepted,
                    modules,
                },
                Err(error) => Message::Error {
                    id,
                    message: error.to_string(),
                },
            }
        }
        Ok(Ok(Message::Ask {
            id,
            query,
            mode,
            conversation_ref,
            explain,
        })) => {
            let Ok(_permit) = controls.execution.try_acquire() else {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    wire::write_message(
                        &mut writer,
                        &Message::Error {
                            id,
                            message: "Service busy; retry later".into(),
                        },
                    ),
                )
                .await;
                return;
            };
            let mut cancellation = controls.cancellation.subscribe();
            controls.active.store(true, Ordering::Release);
            let cancel = async {
                let mut extra = [0];
                tokio::select! {
                    _ = stopped(&mut stopping) => {},
                    _ = cancellation.changed() => {},
                    // EOF cancels the request; extra input violates one-request-per-connection.
                    _ = reader.read(&mut extra) => {},
                }
            };
            let result = engine
                .run_with_options(
                    &query,
                    &mode,
                    crate::language::TaskOptions {
                        conversation_ref,
                        explain,
                    },
                    cancel,
                )
                .await;
            controls.active.store(false, Ordering::Release);
            match result {
                Ok(outcome) => Message::Reply { id, outcome },
                Err(err) => Message::Error {
                    id,
                    message: err.to_string(),
                },
            }
        }
        Ok(Ok(_)) => Message::Error {
            id: "invalid".into(),
            message: "Expected Ask message".into(),
        },
        Ok(Err(err)) => Message::Error {
            id: "invalid".into(),
            message: err.to_string(),
        },
        Err(_) => Message::Error {
            id: "timeout".into(),
            message: "Request timed out".into(),
        },
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        wire::write_message(&mut writer, &response).await?;
        writer.shutdown().await?;
        Ok::<(), anyhow::Error>(())
    })
    .await;
}

pub async fn control(cfg: &Config, action: wire::ControlAction) -> Result<(bool, bool)> {
    let (active, accepted, _) = exchange_control(cfg, action).await?;
    Ok((active, accepted))
}

pub async fn modules(cfg: &Config) -> Result<Vec<crate::fabric::ModuleDescriptor>> {
    let (_, accepted, modules) = exchange_control(cfg, wire::ControlAction::Modules).await?;
    if !accepted {
        bail!("Module discovery unavailable")
    }
    Ok(modules)
}

async fn exchange_control(
    cfg: &Config,
    action: wire::ControlAction,
) -> Result<(bool, bool, Vec<crate::fabric::ModuleDescriptor>)> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = UnixStream::connect(cfg.socket()).await?;
        let id = uuid::Uuid::new_v4().to_string();
        wire::write_message(
            &mut stream,
            &Message::Control {
                id: id.clone(),
                action,
            },
        )
        .await?;
        match wire::read_message(&mut stream).await? {
            Message::ControlReply {
                id: reply_id,
                active,
                accepted,
                modules,
            } if reply_id == id => Ok((active, accepted, modules)),
            _ => bail!("Unexpected control response"),
        }
    })
    .await?
}

pub async fn request(cfg: &Config, query: String, mode: String) -> Result<Outcome> {
    request_with_options(
        cfg,
        query,
        mode,
        crate::language::TaskOptions {
            explain: cfg.synthesize_results,
            ..Default::default()
        },
    )
    .await
}

pub async fn request_with_options(
    cfg: &Config,
    query: String,
    mode: String,
    options: crate::language::TaskOptions,
) -> Result<Outcome> {
    tokio::time::timeout(
        Duration::from_secs(cfg.max_wall_seconds.saturating_add(10)),
        async {
            let mut stream = UnixStream::connect(cfg.socket()).await?;
            let mut message = wire::new_request(query, mode);
            if let Message::Ask {
                conversation_ref,
                explain,
                ..
            } = &mut message
            {
                *conversation_ref = options.conversation_ref;
                *explain = options.explain;
            }
            let id = match &message {
                Message::Ask { id, .. } => id.clone(),
                _ => unreachable!(),
            };
            wire::write_message(&mut stream, &message).await?;
            match wire::read_message(&mut stream).await? {
                Message::Reply {
                    id: reply_id,
                    outcome,
                } if reply_id == id => Ok(outcome),
                Message::Error {
                    id: reply_id,
                    message,
                } if reply_id == id => bail!("Server error: {message}"),
                _ => bail!("Unexpected or mismatched server reply"),
            }
        },
    )
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{self, Store};
    use tokio::sync::oneshot;

    fn setup() -> (Config, Engine) {
        let root = std::env::temp_dir().join(format!("fabric-service-{}", uuid::Uuid::new_v4()));
        let cfg = Config::load(Some(root), Some("demo".into())).unwrap();
        cfg.prepare().unwrap();
        let store = Arc::new(Store::open(&cfg.db()).unwrap());
        store::bootstrap(&store).unwrap();
        (cfg.clone(), Engine::new(cfg, store))
    }

    async fn start(engine: Engine) -> (oneshot::Sender<()>, tokio::task::JoinHandle<Result<()>>) {
        let socket = engine.cfg.socket();
        let (stop, receiver) = oneshot::channel();
        let task = tokio::spawn(serve_until(engine, async {
            let _ = receiver.await;
        }));
        tokio::time::timeout(Duration::from_secs(3), async {
            while !socket.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("Service did not bind");
        (stop, task)
    }

    #[tokio::test]
    async fn socket_roundtrip_permissions_malformed_frames_and_shutdown() {
        let (cfg, engine) = setup();
        let (stop, task) = start(engine.clone()).await;
        assert_eq!(
            std::fs::metadata(cfg.socket())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(serve_until(engine, std::future::ready(())).await.is_err());
        assert!(cfg.socket().exists());
        let result = request(&cfg, "calc: 6 * 7".into(), "fast".into())
            .await
            .unwrap();
        assert_eq!(result.answer, "42");
        assert_eq!(result.status, "completed");
        let discovered = modules(&cfg).await.unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].id, "inference.local");
        assert!(discovered[0].roles.iter().any(|role| role == "solver"));
        assert_eq!(
            control(&cfg, wire::ControlAction::Status).await.unwrap(),
            (false, true)
        );
        assert_eq!(
            control(&cfg, wire::ControlAction::CancelActive)
                .await
                .unwrap(),
            (false, false)
        );
        // Join completion is processed before the next accepted connection.
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut malformed = UnixStream::connect(cfg.socket()).await.unwrap();
        malformed.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
        let response = wire::read_message(&mut malformed).await.unwrap();
        assert!(matches!(response, Message::Error { .. }));
        drop(malformed);
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!cfg.socket().exists());
        std::fs::remove_dir_all(cfg.home).unwrap();
    }

    #[tokio::test]
    async fn shutdown_interrupts_incomplete_frames() {
        let (cfg, engine) = setup();
        let (stop, task) = start(engine).await;
        let mut client = UnixStream::connect(cfg.socket()).await.unwrap();
        client.write_all(&[0, 0]).await.unwrap();
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!cfg.socket().exists());
        std::fs::remove_dir_all(cfg.home).unwrap();
    }

    #[tokio::test]
    async fn typed_stop_joins_incomplete_clients() {
        let (cfg, engine) = setup();
        let (_stop, task) = start(engine).await;
        let mut incomplete = UnixStream::connect(cfg.socket()).await.unwrap();
        incomplete.write_all(&[0, 0]).await.unwrap();
        assert_eq!(
            control(&cfg, wire::ControlAction::Stop).await.unwrap(),
            (false, true)
        );
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!cfg.socket().exists());
        std::fs::remove_dir_all(cfg.home).unwrap();
    }

    #[tokio::test]
    async fn disconnect_and_shutdown_cancel_inference_durably() {
        for cancellation_mode in 0..3 {
            let (mut cfg, _) = setup();
            let http = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            cfg.backend = "openai-local".into();
            cfg.api_base = format!("http://{}/v1", http.local_addr().unwrap());
            let store = Arc::new(Store::open(&cfg.db()).unwrap());
            let engine = Engine::new(cfg.clone(), store);
            let (accepted, received) = oneshot::channel();
            let backend = tokio::spawn(async move {
                let (stream, _) = http.accept().await.unwrap();
                accepted.send(()).unwrap();
                std::future::pending::<()>().await;
                drop(stream);
            });
            let (stop, service) = start(engine).await;
            let mut stop = Some(stop);
            let mut client = UnixStream::connect(cfg.socket()).await.unwrap();
            wire::write_message(
                &mut client,
                &wire::new_request("Explain Tokio".into(), "fast".into()),
            )
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(3), received)
                .await
                .unwrap()
                .unwrap();
            if cancellation_mode == 0 {
                drop(client);
            } else if cancellation_mode == 2 {
                assert_eq!(
                    control(&cfg, wire::ControlAction::Status).await.unwrap(),
                    (true, true)
                );
                assert!(request(&cfg, "calc: 1+1".into(), "fast".into())
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("busy"));
                assert_eq!(
                    control(&cfg, wire::ControlAction::CancelActive)
                        .await
                        .unwrap(),
                    (true, true)
                );
                let result = wire::read_message(&mut client).await.unwrap();
                assert!(
                    matches!(result, Message::Reply { outcome, .. } if outcome.status == "cancelled")
                );
                drop(client);
            } else {
                stop.take().unwrap().send(()).unwrap();
            }
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let db = rusqlite::Connection::open(cfg.db()).unwrap();
                    let status: String = db
                        .query_row("SELECT status FROM tasks LIMIT 1", [], |row| row.get(0))
                        .unwrap();
                    if status == "cancelled" {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("Cancellation was not persisted");
            if cancellation_mode != 1 {
                stop.take().unwrap().send(()).unwrap();
            }
            service.await.unwrap().unwrap();
            backend.abort();
            let _ = backend.await;
            std::fs::remove_dir_all(cfg.home).unwrap();
        }
    }
}
