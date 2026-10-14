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
    sync::{Arc, Mutex},
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
    active: Mutex<Option<String>>,
    execution: Semaphore,
    subscriptions: Semaphore,
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

/// Stop accepting, cancel tasks, join handlers, stop the shared broker, then remove our socket.
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
        active: Mutex::new(None),
        execution: Semaphore::new(1),
        subscriptions: Semaphore::new(2),
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
    let broker_result = engine.broker.shutdown();
    let tool_result = engine.tools.shutdown();
    result.and(broker_result).and(tool_result)
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
    if let Ok(Ok(Message::Control {
        id,
        action: wire::ControlAction::SubscribeProgress,
        task_id: Some(task),
        after,
    })) = &request
    {
        let result = stream_progress(
            &mut reader,
            &mut writer,
            &engine,
            &controls,
            &mut stopping,
            (id, task, *after),
        )
        .await;
        if let Err(error) = result {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                wire::write_message(
                    &mut writer,
                    &Message::Error {
                        id: id.clone(),
                        message: error.to_string().chars().take(300).collect(),
                    },
                ),
            )
            .await;
        }
        return;
    }
    let response = match request {
        Ok(Ok(Message::Control {
            id,
            action,
            task_id,
            after,
        })) => match control_response(&engine, &controls, &id, action, task_id.as_deref(), after) {
            Ok(response) => response,
            Err(error) => Message::Error {
                id,
                message: error.to_string(),
            },
        },
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
            let runtime_task = uuid::Uuid::new_v4().to_string();
            *controls.active.lock().expect("Active task mutex poisoned") =
                Some(runtime_task.clone());
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
                .run_service_task(
                    &runtime_task,
                    &query,
                    &mode,
                    crate::language::TaskOptions {
                        conversation_ref,
                        explain,
                    },
                    cancel,
                )
                .await;
            *controls.active.lock().expect("Active task mutex poisoned") = None;
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

fn control_response(
    engine: &Engine,
    controls: &Controls,
    id: &str,
    action: wire::ControlAction,
    task: Option<&str>,
    after: u64,
) -> Result<Message> {
    let active_task = controls
        .active
        .lock()
        .map_err(|_| anyhow::anyhow!("Active task mutex poisoned"))?;
    let active = active_task.is_some();
    let mut modules = Vec::new();
    let mut tool_modules = Vec::new();
    let mut inference_modules = Vec::new();
    let mut progress = None;
    let accepted = match action {
        wire::ControlAction::Status => true,
        wire::ControlAction::Modules => {
            modules = engine.broker.discover()?;
            true
        }
        wire::ControlAction::InferenceModules => {
            inference_modules = engine.broker.statuses()?;
            true
        }
        wire::ControlAction::ToolModules => {
            tool_modules = engine.tools.statuses()?;
            true
        }
        wire::ControlAction::TaskProgress => {
            let task = task.ok_or_else(|| anyhow::anyhow!("Missing task UUID"))?;
            progress = Some((task.into(), engine.store.progress_since(task, after)?));
            true
        }
        wire::ControlAction::SubscribeProgress => {
            bail!("Subscription requires a streaming connection")
        }
        wire::ControlAction::CancelActive | wire::ControlAction::CancelTask => {
            let matched = active
                && (action == wire::ControlAction::CancelActive || active_task.as_deref() == task);
            if matched {
                controls
                    .cancellation
                    .send_modify(|generation| *generation = generation.wrapping_add(1));
            }
            matched
        }
        wire::ControlAction::Stop => {
            controls.stopping.send_replace(true);
            true
        }
    };
    Ok(Message::ControlReply {
        id: id.into(),
        active,
        accepted,
        modules,
        tool_modules,
        inference_modules,
        active_task_id: active_task.clone(),
        progress,
        stream_state: None,
    })
}

async fn send_progress(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    id: &str,
    task: &str,
    page: crate::events::ProgressPage,
    state: &str,
) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(2),
        wire::write_message(
            writer,
            &Message::ControlReply {
                id: id.into(),
                active: false,
                accepted: true,
                modules: Vec::new(),
                tool_modules: Vec::new(),
                inference_modules: Vec::new(),
                active_task_id: None,
                progress: Some((task.into(), page)),
                stream_state: Some(state.into()),
            },
        ),
    )
    .await??;
    Ok(())
}

async fn stream_progress(
    reader: &mut tokio::net::unix::OwnedReadHalf,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    engine: &Engine,
    controls: &Controls,
    stopping: &mut watch::Receiver<bool>,
    request: (&str, &str, u64),
) -> Result<()> {
    let (id, task, mut cursor) = request;
    let _slot = controls
        .subscriptions
        .try_acquire()
        .map_err(|_| anyhow::anyhow!("Progress subscription capacity exhausted"))?;
    let mut subscription = engine.store.subscribe_progress(task)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if *stopping.borrow() {
            bail!("Service shutting down; resume from the last cursor")
        }
        let page = engine.store.progress_since(task, cursor)?;
        cursor = page.cursor;
        let more = page.more;
        let complete:bool = engine.store.lock()?.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind IN ('outcome','recovery') AND id<=?2)",rusqlite::params![task,cursor],|r|r.get(0))?;
        send_progress(
            writer,
            id,
            task,
            page,
            if complete && !more {
                "complete"
            } else {
                "update"
            },
        )
        .await?;
        if complete && !more {
            return Ok(());
        }
        if !more {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("Progress catch-up deadline exhausted")
        }
    }
    loop {
        let mut extra = [0u8; 1];
        let event = tokio::select! {
            biased;
            _=stopped(stopping)=>bail!("Service shutting down; resume from the last cursor"),
            _=reader.read(&mut extra)=>return Ok(()),
            _=tokio::time::sleep_until(deadline)=> {
                send_progress(writer,id,task,crate::events::ProgressPage{cursor,more:false,events:Vec::new()},"timeout").await?;
                return Ok(())
            },
            event=subscription.receive()=>event?,
        };
        let event = event.ok_or_else(|| {
            anyhow::anyhow!("Progress subscription closed; resume from durable history")
        })?;
        if event.sequence <= cursor {
            continue;
        }
        cursor = event.sequence;
        let complete = matches!(
            event.stage,
            crate::events::Stage::Finished | crate::events::Stage::Recovery
        );
        send_progress(
            writer,
            id,
            task,
            crate::events::ProgressPage {
                cursor,
                more: false,
                events: vec![event],
            },
            if complete { "complete" } else { "update" },
        )
        .await?;
        if complete {
            return Ok(());
        }
    }
}

pub struct ProgressStream {
    stream: UnixStream,
    id: String,
    task: String,
    cursor: u64,
    finished: bool,
}
#[derive(Debug, serde::Serialize)]
pub struct ProgressUpdate {
    pub task_id: String,
    pub state: String,
    pub page: crate::events::ProgressPage,
}
impl ProgressStream {
    pub async fn receive(&mut self) -> Result<Option<ProgressUpdate>> {
        if self.finished {
            return Ok(None);
        }
        let response = tokio::time::timeout(
            Duration::from_secs(65),
            wire::read_message(&mut self.stream),
        )
        .await??;
        match response {
            Message::ControlReply {
                id,
                accepted: true,
                progress: Some((task, page)),
                stream_state: Some(state),
                ..
            } if id == self.id
                && task == self.task
                && page.cursor >= self.cursor
                && page.events.iter().all(|e| e.sequence > self.cursor) =>
            {
                self.cursor = page.cursor;
                self.finished = state != "update";
                Ok(Some(ProgressUpdate {
                    task_id: task,
                    state,
                    page,
                }))
            }
            Message::Error { id, message } if id == self.id => bail!("{message}"),
            _ => bail!("Invalid correlated progress subscription response"),
        }
    }
}
pub async fn subscribe_progress(cfg: &Config, task: &str, after: u64) -> Result<ProgressStream> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = UnixStream::connect(cfg.socket()).await?;
        let id = uuid::Uuid::new_v4().to_string();
        wire::write_message(
            &mut stream,
            &Message::Control {
                id: id.clone(),
                action: wire::ControlAction::SubscribeProgress,
                task_id: Some(task.into()),
                after,
            },
        )
        .await?;
        Ok(ProgressStream {
            stream,
            id,
            task: task.into(),
            cursor: after,
            finished: false,
        })
    })
    .await?
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

pub async fn inference_modules(cfg: &Config) -> Result<Vec<crate::fabric::ModuleStatus>> {
    match exchange_task_control(cfg, wire::ControlAction::InferenceModules, None, 0).await? {
        Message::ControlReply {
            accepted: true,
            inference_modules,
            ..
        } => Ok(inference_modules),
        _ => bail!("Inference module inventory unavailable"),
    }
}

pub async fn tool_modules(cfg: &Config) -> Result<Vec<crate::fabric::tools::ModuleStatus>> {
    match exchange_task_control(cfg, wire::ControlAction::ToolModules, None, 0).await? {
        Message::ControlReply {
            accepted: true,
            tool_modules,
            ..
        } => Ok(tool_modules),
        _ => bail!("Tool module discovery unavailable"),
    }
}

async fn exchange_control(
    cfg: &Config,
    action: wire::ControlAction,
) -> Result<(bool, bool, Vec<crate::fabric::ModuleDescriptor>)> {
    match exchange_task_control(cfg, action, None, 0).await? {
        Message::ControlReply {
            active,
            accepted,
            modules,
            ..
        } => Ok((active, accepted, modules)),
        _ => bail!("Unexpected control response"),
    }
}

pub async fn task_progress(
    cfg: &Config,
    task: &str,
    after: u64,
) -> Result<crate::events::ProgressPage> {
    match exchange_task_control(cfg, wire::ControlAction::TaskProgress, Some(task), after).await? {
        Message::ControlReply {
            accepted: true,
            progress: Some((identity, page)),
            ..
        } if identity == task
            && page.cursor >= after
            && page.events.iter().all(|event| event.sequence > after) =>
        {
            Ok(page)
        }
        _ => bail!("Unexpected task progress response"),
    }
}
pub async fn cancel_task(cfg: &Config, task: &str) -> Result<bool> {
    match exchange_task_control(cfg, wire::ControlAction::CancelTask, Some(task), 0).await? {
        Message::ControlReply {
            accepted,
            active_task_id,
            ..
        } if !accepted || active_task_id.as_deref() == Some(task) => Ok(accepted),
        _ => bail!("Unexpected task cancellation response"),
    }
}
pub async fn status(cfg: &Config) -> Result<(bool, Option<String>)> {
    match exchange_task_control(cfg, wire::ControlAction::Status, None, 0).await? {
        Message::ControlReply {
            active,
            active_task_id,
            accepted: true,
            ..
        } => Ok((active, active_task_id)),
        _ => bail!("Unexpected service status response"),
    }
}
pub async fn active_task(cfg: &Config) -> Result<Option<String>> {
    Ok(status(cfg).await?.1)
}
async fn exchange_task_control(
    cfg: &Config,
    action: wire::ControlAction,
    task: Option<&str>,
    after: u64,
) -> Result<Message> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = UnixStream::connect(cfg.socket()).await?;
        let id = uuid::Uuid::new_v4().to_string();
        wire::write_message(
            &mut stream,
            &Message::Control {
                id: id.clone(),
                action,
                task_id: task.map(String::from),
                after,
            },
        )
        .await?;
        let response = wire::read_message(&mut stream).await?;
        match &response {
            Message::ControlReply { id: reply, .. } if reply == &id => Ok(response),
            Message::Error { id: reply, message } if reply == &id => bail!("{message}"),
            _ => bail!("Unexpected correlated control response"),
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
    async fn progress_subscriptions_catch_up_stream_and_reserve_control_capacity() {
        let (cfg, engine) = setup();
        let identity = engine.store.create_task("watched task", "deep").unwrap();
        engine
            .store
            .event(&identity, "intent", "interface", serde_json::json!({}))
            .unwrap();
        let (stop, service) = start(engine.clone()).await;
        let mut first = subscribe_progress(&cfg, &identity, 0).await.unwrap();
        let initial = first.receive().await.unwrap().unwrap();
        assert_eq!(initial.state, "update");
        assert_eq!(initial.page.events.len(), 1);
        let mut second = subscribe_progress(&cfg, &identity, initial.page.cursor)
            .await
            .unwrap();
        assert_eq!(second.receive().await.unwrap().unwrap().state, "update");
        let mut third = subscribe_progress(&cfg, &identity, 0).await.unwrap();
        assert!(third
            .receive()
            .await
            .unwrap_err()
            .to_string()
            .contains("capacity"));
        assert!(status(&cfg).await.is_ok());
        drop(second);
        let unrelated = engine
            .store
            .create_task("unrelated secret", "fast")
            .unwrap();
        engine
            .store
            .event(
                &unrelated,
                "intent",
                "interface",
                serde_json::json!({"secret":"do not transmit"}),
            )
            .unwrap();
        engine
            .store
            .event(
                &identity,
                "model_dispatch",
                "jev-r",
                serde_json::json!({"role":"solver","call":1}),
            )
            .unwrap();
        let live = first.receive().await.unwrap().unwrap();
        assert!(live.page.cursor > initial.page.cursor);
        assert_eq!(live.page.events.len(), 1);
        assert!(live.page.events.iter().all(|e| e.task_id == identity));
        engine
            .store
            .finish_task(&identity, "unverified", "candidate")
            .unwrap();
        engine
            .store
            .event(
                &identity,
                "outcome",
                "jev-plus",
                serde_json::json!({"status":"unverified","resources":{"calls":1}}),
            )
            .unwrap();
        let terminal = first.receive().await.unwrap().unwrap();
        assert_eq!(terminal.state, "complete");
        assert!(first.receive().await.unwrap().is_none());
        let mut resumed = subscribe_progress(&cfg, &identity, initial.page.cursor)
            .await
            .unwrap();
        let catchup = resumed.receive().await.unwrap().unwrap();
        assert_eq!(catchup.state, "complete");
        assert_eq!(catchup.page.events.len(), 2);
        let mut unknown = subscribe_progress(&cfg, &uuid::Uuid::new_v4().to_string(), 0)
            .await
            .unwrap();
        assert!(unknown.receive().await.is_err());
        let mut pending = subscribe_progress(&cfg, &unrelated, 0).await.unwrap();
        assert_eq!(pending.receive().await.unwrap().unwrap().state, "update");
        let _ = stop.send(());
        tokio::time::timeout(Duration::from_secs(3), service)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(pending.receive().await.is_err());
        std::fs::remove_dir_all(cfg.home).unwrap();
    }

    #[tokio::test]
    async fn overflowing_socket_subscription_requires_durable_resume() {
        let (cfg, engine) = setup();
        let identity = engine.store.create_task("overflow task", "deep").unwrap();
        let (stop, service) = start(engine.clone()).await;
        let mut subscription = subscribe_progress(&cfg, &identity, 0).await.unwrap();
        let initial = subscription.receive().await.unwrap().unwrap();
        for _ in 0..40 {
            engine
                .store
                .event(
                    &identity,
                    "model_dispatch",
                    "jev-r",
                    serde_json::json!({"role":"solver","call":1}),
                )
                .unwrap();
        }
        assert!(subscription
            .receive()
            .await
            .unwrap_err()
            .to_string()
            .contains("lost"));
        let durable = task_progress(&cfg, &identity, initial.page.cursor)
            .await
            .unwrap();
        assert_eq!(durable.events.len(), 32);
        assert!(durable.more);
        let _ = stop.send(());
        tokio::time::timeout(Duration::from_secs(3), service)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        std::fs::remove_dir_all(cfg.home).unwrap();
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
        let inference_inventory = inference_modules(&cfg).await.unwrap();
        assert_eq!(inference_inventory.len(), 1);
        assert!(inference_inventory[0].active);
        assert_eq!(inference_inventory[0].descriptor.id, "inference.local");
        assert_eq!(inference_inventory[0].routing_rank, None);
        assert_eq!(inference_inventory[0].queued_requests, Some(0));
        assert_eq!(inference_inventory[0].processing_request, Some(false));
        let tool_inventory = tool_modules(&cfg).await.unwrap();
        assert_eq!(tool_inventory.len(), 2);
        assert!(tool_inventory.iter().all(|module| module.active));
        assert!(tool_inventory
            .iter()
            .any(|module| module.descriptor.capability == "math.evaluate"));
        assert!(tool_inventory
            .iter()
            .any(|module| module.descriptor.capability == "sql.query"));
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
        use crate::backend::InferenceBackend;
        let (cfg, engine) = setup();
        let broker = engine.broker.clone();
        let pinned = broker.pin().unwrap().unwrap();
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
        assert!(broker.discover().unwrap().is_empty());
        assert!(broker.pin().is_err());
        assert!(pinned
            .generate("solver", "after service stop", 128)
            .await
            .is_err());
        std::fs::remove_dir_all(cfg.home).unwrap();
    }

    #[tokio::test]
    async fn disconnect_and_shutdown_cancel_inference_durably() {
        for cancellation_case in 0..8 {
            let expert_advice = cancellation_case >= 4;
            let cancellation_mode = cancellation_case % 4;
            let (mut cfg, _) = setup();
            let http = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            cfg.backend = "openai-local".into();
            cfg.api_base = format!("http://{}/v1", http.local_addr().unwrap());
            let store = Arc::new(Store::open(&cfg.db()).unwrap());
            if expert_advice {
                cfg.interpret_goals = false;
                let manifest: crate::registry::Manifest = serde_json::from_value(serde_json::json!({
                    "id":"data.sort.s1","version":"1.0.0","kind":"workflow",
                    "description":"Pending expert advice fixture","tags":["sort"],
                    "source":"fixture","license":"CC0","workflow":"Offer optional sorting advice"
                }))
                .unwrap();
                crate::registry::register(&store, &manifest).unwrap();
                crate::registry::activate(&store, &manifest.id, &manifest.version).unwrap();
            }
            let engine = Engine::new(cfg.clone(), store.clone());
            let broker = engine.broker.clone();
            let (accepted, received) = oneshot::channel();
            let backend = tokio::spawn(async move {
                let (stream, _) = http.accept().await.unwrap();
                accepted.send(()).unwrap();
                std::future::pending::<()>().await;
                drop(stream);
            });
            let (stop, service) = start(engine).await;
            let mut stop = Some(stop);
            let prior_task = if cancellation_mode == 3 {
                Some(
                    request(&cfg, "calc: 1+1".into(), "fast".into())
                        .await
                        .unwrap()
                        .task_id,
                )
            } else {
                None
            };
            let mut client = UnixStream::connect(cfg.socket()).await.unwrap();
            wire::write_message(
                &mut client,
                &wire::new_request(
                    if expert_advice {
                        r#"sort:{"values":[3,1,2]}"#
                    } else {
                        "Explain Tokio"
                    }
                    .into(),
                    if expert_advice { "deep" } else { "fast" }.into(),
                ),
            )
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(3), received)
                .await
                .unwrap()
                .unwrap();
            if cancellation_mode == 0 {
                drop(client);
            } else if cancellation_mode >= 2 {
                assert_eq!(
                    control(&cfg, wire::ControlAction::Status).await.unwrap(),
                    (true, true)
                );
                let identity = active_task(&cfg).await.unwrap().unwrap();
                if let Some(prior) = &prior_task {
                    assert!(!cancel_task(&cfg, prior).await.unwrap());
                }
                assert!(!cancel_task(&cfg, &uuid::Uuid::new_v4().to_string())
                    .await
                    .unwrap());
                assert_eq!(
                    active_task(&cfg).await.unwrap().as_deref(),
                    Some(identity.as_str())
                );
                let page = task_progress(&cfg, &identity, 0).await.unwrap();
                assert!(page
                    .events
                    .iter()
                    .any(|event| event.stage == crate::events::Stage::ModelDispatched));
                assert!(task_progress(&cfg, &uuid::Uuid::new_v4().to_string(), 0)
                    .await
                    .is_err());
                assert!(request(&cfg, "calc: 1+1".into(), "fast".into())
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("busy"));
                if cancellation_mode == 2 {
                    assert_eq!(
                        control(&cfg, wire::ControlAction::CancelActive)
                            .await
                            .unwrap(),
                        (true, true)
                    );
                } else {
                    assert!(cancel_task(&cfg, &identity).await.unwrap());
                }
                let result = wire::read_message(&mut client).await.unwrap();
                assert!(
                    matches!(result, Message::Reply { outcome, .. } if outcome.status == "cancelled" && outcome.task_id==identity)
                );
                assert!(!cancel_task(&cfg, &identity).await.unwrap());
                let terminal = task_progress(&cfg, &identity, page.cursor).await.unwrap();
                assert!(terminal
                    .events
                    .iter()
                    .any(|event| event.state == "cancelled"));
                drop(client);
            } else {
                stop.take().unwrap().send(()).unwrap();
            }
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let db = rusqlite::Connection::open(cfg.db()).unwrap();
                    let status: String = db
                        .query_row(
                            "SELECT status FROM tasks ORDER BY created DESC LIMIT 1",
                            [],
                            |row| row.get(0),
                        )
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
            assert!(broker.discover().unwrap().is_empty());
            if expert_advice {
                let db = store.lock().unwrap();
                let task: String = db
                    .query_row(
                        "SELECT id FROM tasks ORDER BY created DESC LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                let counts: (u64,u64,u64,u64) = db.query_row(
                    "SELECT sum(kind='model_dispatch'),sum(kind='expert_phase_budget'),sum(kind='expert_failure'),sum(kind='model_dispatch' AND json_extract(payload,'$.role')='planner') FROM events WHERE task_id=?1",
                    [&task], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
                ).unwrap();
                assert_eq!(counts, (1, 1, 0, 0));
                drop(db);
                let audit = store.inference_audit(&task).unwrap();
                assert_eq!(audit["reserved_calls"], 1);
                assert_eq!(audit["reserved_output_tokens"], 192);
                assert_eq!(audit["reported_usage_complete"], false);
            }

            backend.abort();
            let _ = backend.await;
            std::fs::remove_dir_all(cfg.home).unwrap();
        }
    }
}
