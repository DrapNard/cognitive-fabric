//! One-task native RPC over a connection supplied by trusted host setup.
//! This transport never launches processes or executes proposed client tools.
use super::{
    protocol::{self, Identity, Packet, Payload},
    ChatBackend, Completion, Turn,
};
use crate::{security::TaskPolicy, store::Store};
use anyhow::{anyhow, bail, Result};
use serde_json::json;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::Mutex,
};
#[derive(Clone, Copy)]
pub struct Limits {
    pub output_tokens: usize,
    pub prompt_bytes: usize,
    pub tool_proposals: u32,
    pub seconds: u64,
}
/// Not deserializable. A packet cannot grant its own task, budgets or peer identity.
pub struct Grant {
    task: String,
    uid: u32,
    pid: u32,
    limits: Limits,
    deadline: Instant,
    expires: u64,
    used: bool,
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
impl Grant {
    pub fn new(task: &str, uid: u32, pid: u32, limits: Limits) -> Result<Self> {
        uuid::Uuid::parse_str(task)?;
        TaskPolicy::agent(task)?;
        if pid == 0
            || !(1..=16384).contains(&limits.output_tokens)
            || !(1..=1_000_000).contains(&limits.prompt_bytes)
            || limits.tool_proposals > 64
            || !(1..=300).contains(&limits.seconds)
        {
            bail!("Invalid native worker grant")
        }
        Ok(Self {
            task: task.into(),
            uid,
            pid,
            limits,
            deadline: Instant::now() + Duration::from_secs(limits.seconds),
            expires: now_ms().saturating_add(limits.seconds * 1000),
            used: false,
        })
    }
    pub(crate) fn restrict_deadline(&mut self, deadline: Instant) {
        self.deadline = self.deadline.min(deadline);
        self.expires = self.expires.min(
            now_ms().saturating_add(
                self.deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(u64::MAX as u128) as u64,
            ),
        );
    }
    fn charge(&mut self, packet: &Packet) -> Result<(usize, usize)> {
        let Payload::Request { turn, allocation } = &packet.payload else {
            bail!("Native worker expects a request")
        };
        if self.used
            || packet.identity.task != self.task
            || packet.identity.expires_at_ms <= now_ms()
            || packet.identity.expires_at_ms > self.expires
            || Instant::now() >= self.deadline
        {
            bail!("Native task, lifetime or one-call grant denied")
        }
        TaskPolicy::agent(&self.task)?.authorize(&packet.identity.task, "jev-r", "llm.generate")?;
        turn.validate()?;
        let bytes = serde_json::to_vec(turn)?.len();
        if *allocation == 0
            || *allocation > self.limits.output_tokens
            || bytes > self.limits.prompt_bytes
            || (self.limits.tool_proposals == 0
                && !turn.tools.is_empty()
                && turn.tool_choice.as_ref().is_none_or(|v| v != "none"))
        {
            bail!("Native worker request exceeds its grant")
        }
        self.used = true;
        Ok((*allocation, bytes))
    }
}
fn authenticate(stream: &UnixStream, grant: &Grant) -> Result<()> {
    let peer = stream.peer_cred()?;
    if peer.uid() != grant.uid || peer.pid().map(|p| p as u32) != Some(grant.pid) {
        bail!("Native peer differs from trusted setup")
    }
    Ok(())
}
async fn read(stream: &mut UnixStream, deadline: Instant) -> Result<Packet> {
    read_with_frame_timeout(stream, deadline, Duration::from_secs(10)).await
}
async fn read_with_frame_timeout(
    stream: &mut UnixStream,
    deadline: Instant,
    frame_timeout: Duration,
) -> Result<Packet> {
    let deadline = tokio::time::Instant::from_std(deadline);
    tokio::time::timeout_at(deadline, async {
        // Generation may use the entire grant before the first response byte.
        // Once a frame starts, its remaining prefix/body must arrive promptly.
        let mut prefix = [0; 4];
        stream.read_exact(&mut prefix[..1]).await?;
        tokio::time::timeout_at(
            deadline.min(tokio::time::Instant::now() + frame_timeout),
            async {
                stream.read_exact(&mut prefix[1..]).await?;
                let length = u32::from_be_bytes(prefix) as usize;
                if !(8..=protocol::MAX_FRAME).contains(&length) {
                    bail!("Invalid native Unix frame length")
                }
                let mut bytes = vec![0; length];
                stream.read_exact(&mut bytes).await?;
                protocol::decode(&bytes)
            },
        )
        .await
        .map_err(|_| anyhow!("Native partial frame deadline exhausted"))?
    })
    .await
    .map_err(|_| anyhow!("Native frame read deadline exhausted"))?
}
async fn write(stream: &mut UnixStream, packet: &Packet, deadline: Instant) -> Result<()> {
    let bytes = protocol::encode(packet)?;
    tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline.min(Instant::now() + Duration::from_secs(2))),
        async {
            stream.write_u32(u32::try_from(bytes.len())?).await?;
            stream.write_all(&bytes).await?;
            Ok(())
        },
    )
    .await
    .map_err(|_| anyhow!("Native frame write deadline exhausted"))?
}
pub struct UnixChat {
    task: String,
    stream: Mutex<Option<(UnixStream, Grant)>>,
}
impl UnixChat {
    pub fn connected(stream: UnixStream, grant: Grant) -> Result<Self> {
        authenticate(&stream, &grant)?;
        Ok(Self {
            task: grant.task.clone(),
            stream: Mutex::new(Some((stream, grant))),
        })
    }
    async fn exchange(&self, request: &Turn, tokens: usize) -> Result<Completion> {
        // One use, even on cancellation/failure. Never reuse uncertain response state.
        let (mut stream, mut grant) = self
            .stream
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow!("Native connection already consumed or closed"))?;
        let packet = Packet {
            identity: Identity {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: grant.task.clone(),
                expires_at_ms: grant.expires,
            },
            payload: Payload::Request {
                turn: request.clone(),
                allocation: tokens,
            },
        };
        grant.charge(&packet)?;
        write(&mut stream, &packet, grant.deadline).await?;
        let response = read(&mut stream, grant.deadline).await?;
        if response.identity != packet.identity
            || Instant::now() >= grant.deadline
            || now_ms() >= packet.identity.expires_at_ms
        {
            bail!("Native response correlation or deadline mismatch")
        }
        let Payload::Response(completion) = response.payload else {
            bail!("Native worker did not return a completion")
        };
        completion.validate(request, grant.limits.tool_proposals)?;
        if completion.output_tokens.is_some_and(|n| n > tokens as u64) {
            bail!("Native worker exceeded output allocation")
        }
        Ok(completion)
    }
}
impl ChatBackend for UnixChat {
    fn complete_for<'a>(
        &'a self,
        task: &'a str,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(async move {
            if task != self.task {
                self.stream.lock().await.take();
                bail!("Native adapter is pinned to another task")
            };
            self.exchange(request, tokens).await
        })
    }
    fn complete<'a>(
        &'a self,
        request: &'a Turn,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Completion>> + Send + 'a>> {
        Box::pin(self.exchange(request, tokens))
    }
}
struct Audit {
    store: Arc<Store>,
    task: String,
    session: String,
    seconds: u64,
    tokens: usize,
    bytes: usize,
    charged: bool,
    finished: bool,
}
impl Audit {
    fn record(&self, state: &str, identity: Option<&Identity>) -> Result<()> {
        self.store.worker_audit(&self.task,json!({"format_version":1,"session_id":self.session,"module_id":"inference.native","module_version":"1.0.0","approved_tasks":1,"session_seconds":self.seconds,"state":state,"charged_calls":u32::from(self.charged),"reserved_output_tokens":self.tokens,"charged_prompt_bytes":self.bytes,"request":identity.map(|i|json!({"task_id":i.task,"correlation_id":i.correlation}))}))
    }
}
impl Drop for Audit {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.record("session_failed", None);
        }
    }
}
/// Serve exactly one native generation under a host-created grant and durable audit.
/// The caller owns process provisioning and must retain/reap its own child.
pub async fn serve_connected(
    mut stream: UnixStream,
    mut grant: Grant,
    backend: Arc<dyn ChatBackend>,
    store: Arc<Store>,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    authenticate(&stream, &grant)?;
    let _lease = store.execution_lease()?;
    let running: bool = store.lock()?.query_row(
        "SELECT status='running' FROM tasks WHERE id=?1",
        [&grant.task],
        |r| r.get(0),
    )?;
    if !running {
        bail!("Native worker requires a running audit anchor")
    }
    let mut audit = Audit {
        store,
        task: grant.task.clone(),
        session: uuid::Uuid::new_v4().to_string(),
        seconds: grant.limits.seconds,
        tokens: 0,
        bytes: 0,
        charged: false,
        finished: false,
    };
    audit.record("session_started", None)?;
    let result=async {
        tokio::pin!(shutdown);
        let packet=tokio::select! {biased; _=&mut shutdown=>return Ok(()),result=read(&mut stream,grant.deadline)=>result?};
        let (tokens,bytes)=match grant.charge(&packet) {Ok(r)=>r,Err(e)=>{audit.record("denied",Some(&packet.identity))?;return Err(e)}};
        audit.tokens=tokens;audit.bytes=bytes;audit.charged=true;
        audit.record("authorized",Some(&packet.identity))?;
        let Payload::Request{turn,..}=&packet.payload else{unreachable!()};
        let deadline=grant.deadline.min(Instant::now()+Duration::from_millis(packet.identity.expires_at_ms.saturating_sub(now_ms())));
        if Instant::now()>=deadline {bail!("Native authorization deadline exhausted")}
        let mut extra=[0];
        let completion=tokio::select! {biased;_=&mut shutdown=>return Ok(()),_=stream.read(&mut extra)=>bail!("Native host disconnected or pipelined data"),result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),async { backend.complete_for(&grant.task,turn,tokens).await })=>result.map_err(|_|anyhow!("Native inference deadline exhausted"))?};
        let payload=match completion {
            Ok(c)=>{c.validate(turn,grant.limits.tool_proposals)?;if c.output_tokens.is_some_and(|n|n>tokens as u64){bail!("Native backend exceeded output allocation")};Payload::Response(c)},
            Err(_)=>Payload::Error("Native backend inference failed".into()),
        };
        let response=Packet{identity:packet.identity,payload};
        protocol::encode(&response)?;
        audit.record(if matches!(response.payload,Payload::Response(_)){"inference_completed"}else{"inference_failed"},Some(&response.identity))?;
        tokio::select! {biased;_=&mut shutdown=>Ok(()),result=write(&mut stream,&response,deadline)=>result}
    }.await;
    audit.record(
        if result.is_ok() {
            "session_stopped"
        } else {
            "session_failed"
        },
        None,
    )?;
    audit.finished = true;
    result
}
#[cfg(test)]
#[path = "unix/tests.rs"]
mod tests;
