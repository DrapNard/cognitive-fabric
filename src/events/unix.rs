//! Host-granted, task-scoped event delivery to authenticated local processes.
use super::{
    modules::{EventModule, ModuleSubscription},
    Progress, Stage,
};
use crate::store::Store;
use anyhow::{bail, Context, Result};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/event_stream_generated.rs"));
}
use generated::event_stream as fb;
const MAX_FRAME: usize = 8192;

#[derive(Clone)]
pub struct Grant {
    task: String,
    module: EventModule,
}
impl Grant {
    pub fn new(task: &str, module: EventModule) -> Result<Self> {
        uuid::Uuid::parse_str(task)?;
        module.validate()?;
        Ok(Self {
            task: task.into(),
            module,
        })
    }
}
fn authenticate(stream: &UnixStream, uid: u32, pid: u32) -> Result<()> {
    let peer = stream.peer_cred()?;
    if pid == 0 || peer.uid() != uid || peer.pid().map(|p| p as u32) != Some(pid) {
        bail!("Event peer differs from host-approved process")
    }
    Ok(())
}
#[cfg(test)]
fn encode(
    grant: &Grant,
    kind: fb::Kind,
    cause: fb::Cause,
    event: Option<&Progress>,
) -> Result<Vec<u8>> {
    encode_cursor(grant, kind, cause, event, 0, 1)
}
fn encode_cursor(
    grant: &Grant,
    kind: fb::Kind,
    cause: fb::Cause,
    event: Option<&Progress>,
    cursor: u64,
    version: u16,
) -> Result<Vec<u8>> {
    let event = event.map(Progress::encode).transpose()?;
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let args = fb::PacketArgs {
        version,
        cursor,
        kind,
        cause,
        module_id: Some(builder.create_string(&grant.module.id)),
        module_version: Some(builder.create_string(&grant.module.version)),
        task_id: Some(builder.create_string(&grant.task)),
        event: event.as_ref().map(|e| builder.create_vector(e)),
    };
    let packet = fb::Packet::create(&mut builder, &args);
    fb::finish_packet_buffer(&mut builder, packet);
    let bytes = builder.finished_data().to_vec();
    if bytes.len() > MAX_FRAME {
        bail!("Event module frame exceeds bounds")
    }
    Ok(bytes)
}
struct Frame {
    kind: fb::Kind,
    cause: fb::Cause,
    event: Option<Progress>,
    cursor: u64,
    version: u16,
}
fn decode(grant: &Grant, bytes: &[u8]) -> Result<Frame> {
    if !(8..=MAX_FRAME).contains(&bytes.len()) || !fb::packet_buffer_has_identifier(bytes) {
        bail!("Invalid event module frame")
    }
    let packet = fb::root_as_packet(bytes)?;
    if !matches!(packet.version(), 1 | 2)
        || packet.module_id() != grant.module.id
        || packet.module_version() != grant.module.version
        || packet.task_id() != grant.task
    {
        bail!("Event module grant or protocol mismatch")
    }
    if packet.version() == 1 && (packet.cursor() != 0 || packet.kind() == fb::Kind::Cursor) {
        bail!("Version-one event stream cannot carry replay cursors")
    }
    if packet.cursor() > i64::MAX as u64 {
        bail!("Event replay cursor exceeds bounds")
    }
    let kind = packet.kind();
    let cause = packet.cause();
    let event = packet
        .event()
        .map(|e| Progress::decode(e.bytes()))
        .transpose()?;
    match kind {
        fb::Kind::Open if cause == fb::Cause::None && event.is_none() => (),
        fb::Kind::Progress if cause == fb::Cause::None && event.is_some() => (),
        fb::Kind::Cursor
            if packet.version() == 2 && cause == fb::Cause::None && event.is_none() => {}
        fb::Kind::Closed
            if event.is_none()
                && matches!(
                    cause,
                    fb::Cause::Completed
                        | fb::Cause::Cancelled
                        | fb::Cause::Timeout
                        | fb::Cause::Limit
                ) => {}
        fb::Kind::Error
            if event.is_none() && matches!(cause, fb::Cause::Lost | fb::Cause::Protocol) => {}
        _ => bail!("Invalid event module packet shape"),
    }
    if event
        .as_ref()
        .is_some_and(|e| e.task_id != grant.task || !grant.module.stages.contains(&e.stage))
    {
        bail!("Event exceeds granted task or stage scope")
    }
    if packet.version() == 2
        && kind == fb::Kind::Progress
        && event
            .as_ref()
            .is_some_and(|e| e.sequence != packet.cursor())
    {
        bail!("Event replay cursor differs from event sequence")
    }
    Ok(Frame {
        kind,
        cause,
        event,
        cursor: packet.cursor(),
        version: packet.version(),
    })
}
async fn write(stream: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(2), async {
        stream.write_u32(u32::try_from(bytes.len())?).await?;
        stream.write_all(bytes).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("Event module write deadline exhausted")?
}
async fn read(stream: &mut UnixStream) -> Result<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let size = stream.read_u32().await? as usize;
        if !(8..=MAX_FRAME).contains(&size) {
            bail!("Invalid event module frame length")
        }
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes).await?;
        Ok(bytes)
    })
    .await
    .context("Event module read deadline exhausted")?
}

pub struct EventSession {
    store: Arc<Store>,
    grant: Grant,
    subscription: ModuleSubscription,
    stream: UnixStream,
}
impl EventSession {
    /// The embedding host supplies the peer identity and registration; IPC never self-registers.
    pub fn connected(
        store: Arc<Store>,
        task: &str,
        module: &str,
        stream: UnixStream,
        uid: u32,
        pid: u32,
    ) -> Result<Self> {
        authenticate(&stream, uid, pid)?;
        let subscription = store.subscribe_module_progress(task, module)?;
        let grant = Grant::new(task, subscription.descriptor().clone())?;
        Ok(Self {
            store,
            grant,
            subscription,
            stream,
        })
    }
    /// Restore a host-retained grant even after registry replacement/unregistration.
    pub fn reconnect(
        store: Arc<Store>,
        grant: Grant,
        stream: UnixStream,
        uid: u32,
        pid: u32,
    ) -> Result<Self> {
        authenticate(&stream, uid, pid)?;
        let subscription = ModuleSubscription::restore(&store, &grant.task, grant.module.clone())?;
        Ok(Self {
            store,
            grant,
            subscription,
            stream,
        })
    }
    pub fn grant(&self) -> Grant {
        self.grant.clone()
    }
    pub async fn serve(
        self,
        seconds: u64,
        cancel: impl std::future::Future<Output = ()>,
    ) -> Result<()> {
        self.serve_inner(seconds, None, cancel).await
    }
    pub async fn serve_from(
        self,
        seconds: u64,
        after: u64,
        cancel: impl std::future::Future<Output = ()>,
    ) -> Result<()> {
        self.serve_inner(seconds, Some(after), cancel).await
    }
    async fn serve_inner(
        mut self,
        seconds: u64,
        after: Option<u64>,
        cancel: impl std::future::Future<Output = ()>,
    ) -> Result<()> {
        if !(1..=300).contains(&seconds) {
            bail!("Event session lifetime exceeds bounds")
        }
        if let Some(after) = after {
            self.subscription.progress_since(&self.store, after)?;
        }
        self.store.event(&self.grant.task,"module_event_session","fabric",serde_json::json!({"state":"started","module_id":self.grant.module.id,"module_version":self.grant.module.version,"seconds":seconds,"capabilities_granted":false}))?;
        write(
            &mut self.stream,
            &encode_cursor(
                &self.grant,
                fb::Kind::Open,
                fb::Cause::None,
                None,
                after.unwrap_or(0),
                if after.is_some() { 2 } else { 1 },
            )?,
        )
        .await?;
        tokio::pin!(cancel);
        let deadline = tokio::time::sleep(Duration::from_secs(seconds));
        tokio::pin!(deadline);
        let mut count = 0;
        let version = if after.is_some() { 2 } else { 1 };
        let mut watermark = after.unwrap_or(0);
        let mut replay_active = after.is_some();
        let mut pending = std::collections::VecDeque::<Progress>::new();
        let mut next_cursor = None;
        let cause = loop {
            enum Next {
                Cancel,
                Timeout,
                Peer(std::io::Result<u8>),
                Event(Result<Option<Progress>>),
                Replay(Result<super::ProgressPage>),
                Cursor(u64),
            }
            let next = tokio::select! {biased;
                _=&mut cancel=>Next::Cancel,
                _=&mut deadline=>Next::Timeout,
                data=self.stream.read_u8()=>Next::Peer(data),
                next=async {
                    if let Some(event)=pending.pop_front() {Next::Event(Ok(Some(event)))}
                    else if let Some(cursor)=next_cursor.take() {Next::Cursor(cursor)}
                    else if replay_active {Next::Replay(self.subscription.progress_since(&self.store,watermark))}
                    else {Next::Event(self.subscription.receive().await)}
                }=>next,
            };
            match next {
                Next::Replay(Ok(page)) => {
                    pending = page.events.into();
                    next_cursor = Some(page.cursor);
                    replay_active = page.more;
                }
                Next::Replay(Err(_)) => break fb::Cause::Lost,
                Next::Cursor(cursor) => {
                    write(
                        &mut self.stream,
                        &encode_cursor(
                            &self.grant,
                            fb::Kind::Cursor,
                            fb::Cause::None,
                            None,
                            cursor,
                            2,
                        )?,
                    )
                    .await?;
                    watermark = cursor;
                    count += 1;
                    if count >= 4096 {
                        break fb::Cause::Limit;
                    }
                }
                Next::Cancel => break fb::Cause::Cancelled,
                Next::Timeout => break fb::Cause::Timeout,
                Next::Peer(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.subscription.cancel();
                    self.store.event(&self.grant.task,"module_event_session","fabric",serde_json::json!({"state":"peer_closed","module_id":self.grant.module.id,"module_version":self.grant.module.version,"frames":count}))?;
                    return Ok(());
                }
                Next::Peer(_) => break fb::Cause::Protocol,
                Next::Event(Err(_)) => break fb::Cause::Lost,
                Next::Event(Ok(None)) => break fb::Cause::Cancelled,
                Next::Event(Ok(Some(event))) => {
                    if after.is_some() && event.sequence <= watermark {
                        continue;
                    }
                    watermark = event.sequence;
                    let terminal = event.stage == Stage::Finished;
                    write(
                        &mut self.stream,
                        &encode_cursor(
                            &self.grant,
                            fb::Kind::Progress,
                            fb::Cause::None,
                            Some(&event),
                            if version == 2 { event.sequence } else { 0 },
                            version,
                        )?,
                    )
                    .await?;
                    count += 1;
                    if terminal {
                        break fb::Cause::Completed;
                    }
                    if count >= 4096 {
                        break fb::Cause::Limit;
                    }
                }
            }
        };
        self.subscription.cancel();
        let kind = if matches!(cause, fb::Cause::Lost | fb::Cause::Protocol) {
            fb::Kind::Error
        } else {
            fb::Kind::Closed
        };
        write(
            &mut self.stream,
            &encode_cursor(
                &self.grant,
                kind,
                cause,
                None,
                if version == 2 { watermark } else { 0 },
                version,
            )?,
        )
        .await?;
        self.store.event(&self.grant.task,"module_event_session","fabric",serde_json::json!({"state":"stopped","module_id":self.grant.module.id,"module_version":self.grant.module.version,"frames":count,"cause":cause.0}))?;
        if kind == fb::Kind::Error {
            bail!("Event module session lost delivery or violated protocol")
        }
        Ok(())
    }
}

/// A receiver pinned to one trusted grant; cancellation drops its connection permanently.
pub struct EventReceiver {
    stream: Option<UnixStream>,
    grant: Grant,
    opened: bool,
    last: u64,
    end: Option<String>,
    version: u16,
    frames: u32,
    deadline: tokio::time::Instant,
}
impl EventReceiver {
    pub fn connected(stream: UnixStream, grant: Grant, uid: u32, pid: u32) -> Result<Self> {
        authenticate(&stream, uid, pid)?;
        Ok(Self {
            stream: Some(stream),
            grant,
            opened: false,
            last: 0,
            end: None,
            version: 0,
            frames: 0,
            deadline: tokio::time::Instant::now() + Duration::from_secs(305),
        })
    }
    pub fn cursor(&self) -> u64 {
        self.last
    }
    pub fn end_reason(&self) -> Option<&str> {
        self.end.as_deref()
    }
    pub async fn receive(&mut self) -> Result<Option<Progress>> {
        let Some(mut stream) = self.stream.take() else {
            return Ok(None);
        };
        loop {
            let bytes = tokio::time::timeout_at(self.deadline, read(&mut stream))
                .await
                .context("Event receiver session deadline exhausted")??;
            self.frames += 1;
            if self.frames > 4098 {
                bail!("Event receiver frame allowance exhausted")
            }
            let Frame {
                kind,
                cause,
                event,
                cursor,
                version,
            } = decode(&self.grant, &bytes)?;
            if self.opened && version != self.version {
                bail!("Event stream changed protocol version")
            }
            if !self.opened {
                if kind != fb::Kind::Open {
                    bail!("Event module stream did not open")
                }
                self.opened = true;
                self.version = version;
                self.last = cursor;
                continue;
            }
            match kind {
                fb::Kind::Cursor => {
                    if cursor < self.last {
                        bail!("Replay cursor moved backwards")
                    }
                    self.last = cursor;
                }
                fb::Kind::Progress => {
                    let event = event.expect("Validated progress packet");
                    if event.sequence <= self.last {
                        bail!("Event module sequence did not advance")
                    }
                    self.last = event.sequence;
                    self.stream = Some(stream);
                    return Ok(Some(event));
                }
                fb::Kind::Closed => {
                    if version == 2 && cursor < self.last {
                        bail!("Terminal cursor moved backwards")
                    }
                    self.last = self.last.max(cursor);
                    self.end = Some(format!("{cause:?}"));
                    return Ok(None);
                }
                fb::Kind::Error => bail!("Event module stream reported {cause:?}"),
                _ => bail!("Unexpected event module packet"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn module(version: &str) -> EventModule {
        EventModule {
            id: "observer.local".into(),
            version: version.into(),
            stages: vec![Stage::Finished],
        }
    }
    fn setup() -> (Arc<Store>, String) {
        let store = Arc::new(Store::in_memory().unwrap());
        let task = store.create_task("secret objective", "fast").unwrap();
        store.register_event_module(module("1.0.0")).unwrap();
        (store, task)
    }
    fn identity() -> (u32, u32) {
        (unsafe { libc::geteuid() }, std::process::id())
    }
    #[tokio::test]
    async fn authenticated_delivery_pins_task_version_and_excludes_private_bodies() {
        let (store, task) = setup();
        let (host, peer) = UnixStream::pair().unwrap();
        let (uid, pid) = identity();
        let session =
            EventSession::connected(store.clone(), &task, "observer.local", host, uid, pid)
                .unwrap();
        let grant = session.grant();
        let mut receiver = EventReceiver::connected(peer, grant, uid, pid).unwrap();
        store.register_event_module(module("2.0.0")).unwrap();
        store.unregister_event_module("observer.local").unwrap();
        let run = tokio::spawn(session.serve(5, std::future::pending()));
        let other = store.create_task("other private task", "fast").unwrap();
        store
            .event(
                &other,
                "outcome",
                "runtime",
                serde_json::json!({"status":"completed"}),
            )
            .unwrap();
        store
            .event(
                &task,
                "route",
                "jev-plus",
                serde_json::json!({"secret":"private"}),
            )
            .unwrap();
        store
            .event(
                &task,
                "outcome",
                "runtime",
                serde_json::json!({"status":"unverified","answer":"private answer"}),
            )
            .unwrap();
        let event = receiver.receive().await.unwrap().unwrap();
        assert_eq!(event.task_id, task);
        assert_eq!(event.stage, Stage::Finished);
        assert!(!serde_json::to_string(&event).unwrap().contains("private"));
        assert!(receiver.receive().await.unwrap().is_none());
        assert_eq!(receiver.end_reason(), Some("Completed"));
        run.await.unwrap().unwrap();
    }
    #[test]
    fn packet_validation_rejects_unknown_kinds_and_out_of_scope_events() {
        let task = uuid::Uuid::new_v4().to_string();
        let grant = Grant::new(&task, module("1.0.0")).unwrap();
        assert!(decode(
            &grant,
            &encode(&grant, fb::Kind(255), fb::Cause::None, None).unwrap()
        )
        .is_err());
        assert!(decode(
            &grant,
            &encode(&grant, fb::Kind::Open, fb::Cause::Lost, None).unwrap()
        )
        .is_err());
        let mut event = Progress {
            sequence: 1,
            task_id: task,
            stage: Stage::Routing,
            source: "jev-plus".into(),
            state: "running".into(),
            role: None,
            subtask_id: None,
            model_calls: None,
            tool_calls: None,
            cycles: None,
            verified: None,
        };
        assert!(decode(
            &grant,
            &encode(&grant, fb::Kind::Progress, fb::Cause::None, Some(&event)).unwrap()
        )
        .is_err());
        event.stage = Stage::Finished;
        event.task_id = uuid::Uuid::new_v4().to_string();
        assert!(decode(
            &grant,
            &encode(&grant, fb::Kind::Progress, fb::Cause::None, Some(&event)).unwrap()
        )
        .is_err());
        let bytes = encode(&grant, fb::Kind::Open, fb::Cause::None, None).unwrap();
        let mut object = fb::root_as_packet(&bytes).unwrap().unpack();
        object.version = 3;
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let root = object.pack(&mut builder);
        fb::finish_packet_buffer(&mut builder, root);
        assert!(decode(&grant, builder.finished_data()).is_err());
    }
    #[tokio::test]
    async fn wrong_process_scope_and_malformed_frames_fail_closed() {
        let (store, task) = setup();
        let (uid, pid) = identity();
        for (wrong_uid, wrong_pid) in [
            (uid.wrapping_add(1), pid),
            (uid, pid.wrapping_add(1)),
            (uid, 0),
        ] {
            let (host, _peer) = UnixStream::pair().unwrap();
            assert!(EventSession::connected(
                store.clone(),
                &task,
                "observer.local",
                host,
                wrong_uid,
                wrong_pid
            )
            .is_err());
        }
        let grant = Grant::new(&task, module("1.0.0")).unwrap();
        for variant in 0..5 {
            let (peer, mut host) = UnixStream::pair().unwrap();
            let mut receiver = EventReceiver::connected(peer, grant.clone(), uid, pid).unwrap();
            let mut forged = grant.clone();
            if variant == 0 {
                forged.task = uuid::Uuid::new_v4().to_string()
            }
            if variant == 1 {
                forged.module.version = "2.0.0".into()
            }
            if variant < 2 {
                write(
                    &mut host,
                    &encode(&forged, fb::Kind::Open, fb::Cause::None, None).unwrap(),
                )
                .await
                .unwrap();
            } else if variant == 2 {
                host.write_u32(MAX_FRAME as u32 + 1).await.unwrap();
            } else if variant == 3 {
                write(&mut host, &[0; 16]).await.unwrap();
            } else {
                write(
                    &mut host,
                    &encode(&grant, fb::Kind::Closed, fb::Cause::Completed, None).unwrap(),
                )
                .await
                .unwrap();
            }
            assert!(receiver.receive().await.is_err());
            assert!(receiver.stream.is_none());
        }
    }
    #[tokio::test]
    async fn overflow_and_upstream_commands_are_explicit_errors() {
        for upstream in [false, true] {
            let (store, task) = setup();
            let (host, mut peer) = UnixStream::pair().unwrap();
            let (uid, pid) = identity();
            let session =
                EventSession::connected(store.clone(), &task, "observer.local", host, uid, pid)
                    .unwrap();
            let grant = session.grant();
            if upstream {
                peer.write_u8(1).await.unwrap();
            } else {
                for _ in 0..17 {
                    store
                        .event(
                            &task,
                            "outcome",
                            "runtime",
                            serde_json::json!({"status":"unverified"}),
                        )
                        .unwrap();
                }
            }
            let mut receiver = EventReceiver::connected(peer, grant, uid, pid).unwrap();
            let run = tokio::spawn(session.serve(5, std::future::pending()));
            assert!(receiver.receive().await.is_err());
            assert!(run.await.unwrap().is_err());
            assert_eq!(store.task(&task).unwrap()["task"]["status"], "running");
        }
    }
    #[tokio::test]
    async fn cancellation_closes_observation_and_releases_subscription_capacity() {
        let (store, task) = setup();
        let (host, peer) = UnixStream::pair().unwrap();
        let (uid, pid) = identity();
        let session =
            EventSession::connected(store.clone(), &task, "observer.local", host, uid, pid)
                .unwrap();
        let mut receiver = EventReceiver::connected(peer, session.grant(), uid, pid).unwrap();
        let run = tokio::spawn(session.serve(5, std::future::ready(())));
        assert!(receiver.receive().await.unwrap().is_none());
        assert_eq!(receiver.end_reason(), Some("Cancelled"));
        run.await.unwrap().unwrap();
        let mut subscriptions = Vec::new();
        for _ in 0..64 {
            subscriptions.push(store.subscribe_progress(&task).unwrap());
        }
        assert!(store.subscribe_progress(&task).is_err());
        assert_eq!(store.task(&task).unwrap()["task"]["status"], "running");
    }
    #[tokio::test(start_paused = true)]
    async fn session_and_partial_frame_deadlines_are_enforced() {
        let (store, task) = setup();
        let (uid, pid) = identity();
        let (host, peer) = UnixStream::pair().unwrap();
        let session =
            EventSession::connected(store.clone(), &task, "observer.local", host, uid, pid)
                .unwrap();
        let mut receiver = EventReceiver::connected(peer, session.grant(), uid, pid).unwrap();
        let run = tokio::spawn(session.serve(1, std::future::pending()));
        assert!(receiver.receive().await.unwrap().is_none());
        assert_eq!(receiver.end_reason(), Some("Timeout"));
        run.await.unwrap().unwrap();
        let (mut host, peer) = UnixStream::pair().unwrap();
        host.write_u32(64).await.unwrap();
        let mut receiver =
            EventReceiver::connected(peer, Grant::new(&task, module("1.0.0")).unwrap(), uid, pid)
                .unwrap();
        assert!(receiver
            .receive()
            .await
            .unwrap_err()
            .to_string()
            .contains("deadline"));
        assert!(receiver.stream.is_none());
        assert_eq!(store.task(&task).unwrap()["task"]["status"], "running");
    }
    #[tokio::test]
    async fn replay_switches_to_live_delivery_without_duplicate_events_or_changed_filters() {
        let (store, task) = setup();
        store
            .register_event_module(EventModule {
                id: "router.observer".into(),
                version: "1.0.0".into(),
                stages: vec![Stage::Routing],
            })
            .unwrap();
        for _ in 0..40 {
            store
                .event(
                    &task,
                    "verification",
                    "verifier-r",
                    serde_json::json!({"verified":false}),
                )
                .unwrap();
        }
        store
            .event(&task, "route", "jev-plus", serde_json::json!({}))
            .unwrap();
        let (host, peer) = UnixStream::pair().unwrap();
        let (uid, pid) = identity();
        let session =
            EventSession::connected(store.clone(), &task, "router.observer", host, uid, pid)
                .unwrap();
        let mut receiver = EventReceiver::connected(peer, session.grant(), uid, pid).unwrap();
        store
            .register_event_module(EventModule {
                id: "router.observer".into(),
                version: "2.0.0".into(),
                stages: vec![Stage::Finished],
            })
            .unwrap();
        store
            .event(&task, "route", "jev-plus", serde_json::json!({}))
            .unwrap();
        let (stop, cancel) = tokio::sync::oneshot::channel();
        let run = tokio::spawn(session.serve_from(5, 0, async {
            let _ = cancel.await;
        }));
        let first = receiver.receive().await.unwrap().unwrap();
        let second = receiver.receive().await.unwrap().unwrap();
        assert!(second.sequence > first.sequence);
        store
            .event(&task, "route", "jev-plus", serde_json::json!({}))
            .unwrap();
        let third = receiver.receive().await.unwrap().unwrap();
        assert!(third.sequence > second.sequence);
        assert_eq!(third.stage, Stage::Routing);
        stop.send(()).unwrap();
        assert!(receiver.receive().await.unwrap().is_none());
        assert!(receiver.cursor() >= third.sequence);
        run.await.unwrap().unwrap();
    }
    #[tokio::test(start_paused = true)]
    async fn empty_filtered_replay_advances_cursor_and_reconnects_at_the_saved_position() {
        let (store, task) = setup();
        let (uid, pid) = identity();
        for _ in 0..40 {
            store
                .event(&task, "route", "jev-plus", serde_json::json!({}))
                .unwrap();
        }
        let (host, peer) = UnixStream::pair().unwrap();
        let session =
            EventSession::connected(store.clone(), &task, "observer.local", host, uid, pid)
                .unwrap();
        let mut receiver = EventReceiver::connected(peer, session.grant(), uid, pid).unwrap();
        let pinned = session.grant();
        let run = tokio::spawn(session.serve_from(1, 0, std::future::pending()));
        assert!(receiver.receive().await.unwrap().is_none());
        run.await.unwrap().unwrap();
        let cursor = receiver.cursor();
        assert!(cursor >= 40);
        assert_eq!(receiver.end_reason(), Some("Timeout"));
        let (host, peer) = UnixStream::pair().unwrap();
        store.unregister_event_module("observer.local").unwrap();
        let session = EventSession::reconnect(store.clone(), pinned, host, uid, pid).unwrap();
        let mut receiver = EventReceiver::connected(peer, session.grant(), uid, pid).unwrap();
        store
            .event(
                &task,
                "outcome",
                "runtime",
                serde_json::json!({"status":"unverified"}),
            )
            .unwrap();
        let run = tokio::spawn(session.serve_from(1, cursor, std::future::pending()));
        let event = receiver.receive().await.unwrap().unwrap();
        assert!(event.sequence > cursor);
        assert_eq!(event.stage, Stage::Finished);
        assert!(receiver.receive().await.unwrap().is_none());
        run.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn receiver_rejects_backwards_cursors_and_midstream_version_changes() {
        let task = uuid::Uuid::new_v4().to_string();
        let grant = Grant::new(&task, module("1.0.0")).unwrap();
        let (uid, pid) = identity();
        for downgrade in [false, true] {
            let (mut host, peer) = UnixStream::pair().unwrap();
            let mut receiver = EventReceiver::connected(peer, grant.clone(), uid, pid).unwrap();
            write(
                &mut host,
                &encode_cursor(&grant, fb::Kind::Open, fb::Cause::None, None, 10, 2).unwrap(),
            )
            .await
            .unwrap();
            let bytes = if downgrade {
                encode(&grant, fb::Kind::Closed, fb::Cause::Completed, None).unwrap()
            } else {
                encode_cursor(&grant, fb::Kind::Cursor, fb::Cause::None, None, 9, 2).unwrap()
            };
            write(&mut host, &bytes).await.unwrap();
            assert!(receiver.receive().await.is_err());
            assert!(receiver.stream.is_none());
        }
    }
    #[tokio::test(start_paused = true)]
    async fn cursor_only_peers_cannot_exceed_receiver_frame_or_session_limits() {
        let task = uuid::Uuid::new_v4().to_string();
        let grant = Grant::new(&task, module("1.0.0")).unwrap();
        let (uid, pid) = identity();
        let (mut host, peer) = UnixStream::pair().unwrap();
        let mut receiver = EventReceiver::connected(peer, grant.clone(), uid, pid).unwrap();
        let sent = grant.clone();
        let writer = tokio::spawn(async move {
            write(
                &mut host,
                &encode_cursor(&sent, fb::Kind::Open, fb::Cause::None, None, 0, 2).unwrap(),
            )
            .await
            .unwrap();
            for cursor in 1..=4098 {
                if write(
                    &mut host,
                    &encode_cursor(&sent, fb::Kind::Cursor, fb::Cause::None, None, cursor, 2)
                        .unwrap(),
                )
                .await
                .is_err()
                {
                    break;
                }
            }
        });
        assert!(receiver
            .receive()
            .await
            .unwrap_err()
            .to_string()
            .contains("frame allowance"));
        assert!(receiver.stream.is_none());
        writer.await.unwrap();
        let (mut host, peer) = UnixStream::pair().unwrap();
        let mut receiver = EventReceiver::connected(peer, grant.clone(), uid, pid).unwrap();
        assert_eq!(
            receiver.deadline - tokio::time::Instant::now(),
            Duration::from_secs(305)
        );
        receiver.deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        write(
            &mut host,
            &encode_cursor(&grant, fb::Kind::Open, fb::Cause::None, None, 0, 2).unwrap(),
        )
        .await
        .unwrap();
        assert!(receiver
            .receive()
            .await
            .unwrap_err()
            .to_string()
            .contains("session deadline"));
        assert!(receiver.stream.is_none());
    }
    #[test]
    fn version_one_layout_remains_valid_and_replay_cursors_require_version_two() {
        let task = uuid::Uuid::new_v4().to_string();
        let grant = Grant::new(&task, module("1.0.0")).unwrap();
        assert!(decode(
            &grant,
            &encode(&grant, fb::Kind::Open, fb::Cause::None, None).unwrap()
        )
        .is_ok());
        assert!(decode(
            &grant,
            &encode_cursor(&grant, fb::Kind::Cursor, fb::Cause::None, None, 1, 1).unwrap()
        )
        .is_err());
        assert!(decode(
            &grant,
            &encode_cursor(&grant, fb::Kind::Cursor, fb::Cause::None, None, u64::MAX, 2).unwrap()
        )
        .is_err());
        assert!(decode(
            &grant,
            &encode_cursor(&grant, fb::Kind::Cursor, fb::Cause::None, None, 1, 2).unwrap()
        )
        .is_ok());
    }
    #[tokio::test]
    async fn child_fixture() {
        let Some(path) = std::env::var_os("COGFABRIC_EVENT_FIXTURE_SOCKET") else {
            return;
        };
        let task = std::env::var("COGFABRIC_EVENT_FIXTURE_TASK").unwrap();
        let host_pid = std::env::var("COGFABRIC_EVENT_FIXTURE_PID")
            .unwrap()
            .parse()
            .unwrap();
        let stream = UnixStream::connect(path).await.unwrap();
        let grant = Grant::new(&task, module("1.0.0")).unwrap();
        let mut receiver =
            EventReceiver::connected(stream, grant, unsafe { libc::geteuid() }, host_pid).unwrap();
        let event = receiver.receive().await.unwrap().unwrap();
        assert_eq!(event.task_id, task);
        assert_eq!(event.stage, Stage::Finished);
        assert!(receiver.receive().await.unwrap().is_none());
        assert_eq!(receiver.end_reason(), Some("Completed"));
    }
    #[tokio::test]
    async fn separate_process_receives_only_its_host_granted_task() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let (store, task) = setup();
        let root = std::env::temp_dir().join(format!("fabric-event-peer-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let socket = root.join("events.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "events::unix::tests::child_fixture"])
            .env("COGFABRIC_EVENT_FIXTURE_SOCKET", &socket)
            .env("COGFABRIC_EVENT_FIXTURE_TASK", &task)
            .env(
                "COGFABRIC_EVENT_FIXTURE_PID",
                std::process::id().to_string(),
            )
            .stdout(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let (stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let session = EventSession::connected(
            store.clone(),
            &task,
            "observer.local",
            stream,
            unsafe { libc::geteuid() },
            child.id().unwrap(),
        )
        .unwrap();
        let run = tokio::spawn(session.serve(5, std::future::pending()));
        store
            .event(
                &task,
                "outcome",
                "runtime",
                serde_json::json!({"status":"unverified"}),
            )
            .unwrap();
        run.await.unwrap().unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success());
        drop(listener);
        std::fs::remove_dir_all(root).unwrap();
    }
}
