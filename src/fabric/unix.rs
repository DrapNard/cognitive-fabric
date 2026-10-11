//! Host-authorized inference RPC over an already connected local Unix socket.
use super::{
    actor, decode, encode, now_ms, same_header, Header, ModuleDescriptor, Packet, Payload,
};
use crate::backend::{Generation, InferenceBackend};
use anyhow::{anyhow, bail, Context, Result};
use std::{future::Future, pin::Pin, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::Mutex,
};
pub mod bootstrap;
pub mod process;
pub mod worker;

/// An immutable connected peer, supplied by trusted host code, never a model path.
/// Failed or cancelled exchanges permanently close this adapter to avoid reusing
/// a stream whose next response could belong to a different task.
pub struct UnixInference {
    descriptor: ModuleDescriptor,
    stream: Mutex<Option<UnixStream>>,
}

fn authenticate(stream: &UnixStream, uid: u32, pid: u32) -> Result<()> {
    let credentials = stream.peer_cred()?;
    if pid == 0 || credentials.uid() != uid || credentials.pid().map(|p| p as u32) != Some(pid) {
        bail!("Inference peer credentials do not match the host-approved process")
    }
    Ok(())
}

impl UnixInference {
    pub fn connected(
        stream: UnixStream,
        descriptor: ModuleDescriptor,
        uid: u32,
        pid: u32,
    ) -> Result<Self> {
        descriptor.validate()?;
        authenticate(&stream, uid, pid)?;
        Ok(Self {
            descriptor,
            stream: Mutex::new(Some(stream)),
        })
    }
}

async fn read(stream: &mut UnixStream) -> Result<Packet> {
    read_optional(stream)
        .await?
        .context("Unix module peer closed")
}
async fn read_optional(stream: &mut UnixStream) -> Result<Option<Packet>> {
    let mut prefix = [0u8; 4];
    if stream.read(&mut prefix[..1]).await? == 0 {
        return Ok(None);
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        stream.read_exact(&mut prefix[1..]).await?;
        let length = u32::from_be_bytes(prefix) as usize;
        if !(8..=super::protocol::MAX_FRAME).contains(&length) {
            bail!("Invalid Unix module frame length")
        }
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).await?;
        Ok(Some(decode(&bytes)?))
    })
    .await
    .map_err(|_| anyhow!("Incomplete Unix module frame deadline exhausted"))?
}

async fn write(stream: &mut UnixStream, packet: &Packet) -> Result<()> {
    let bytes = encode(packet)?;
    stream.write_u32(u32::try_from(bytes.len())?).await?;
    stream.write_all(&bytes).await?;
    Ok(())
}

impl InferenceBackend for UnixInference {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        prompt: &'a str,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            self.generate_for(&uuid::Uuid::new_v4().to_string(), role, prompt, tokens)
                .await
        })
    }
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        Box::pin(async move {
            if !self.descriptor.roles.iter().any(|r| r == role)
                || prompt.len() > 32_000
                || tokens == 0
                || tokens > 16_384
            {
                bail!("Unix module request exceeds registered scope or bounds")
            }
            let sender = actor(role)?;
            crate::security::TaskPolicy::reasoning(task)?.authorize(
                task,
                sender,
                "llm.generate",
            )?;
            let header = Header {
                correlation: uuid::Uuid::new_v4().to_string(),
                task: task.into(),
                sender: sender.into(),
                destination: self.descriptor.id.clone(),
                module_version: self.descriptor.version.clone(),
                expires: now_ms().saturating_add(300_000),
            };
            let packet = Packet {
                header: header.clone(),
                payload: Payload::Request {
                    role: role.into(),
                    prompt: prompt.into(),
                    tokens,
                },
            };
            tokio::time::timeout(Duration::from_secs(300), async {
                let mut slot = self.stream.lock().await;
                let mut stream = slot.take().context("Unix inference connection is closed")?;
                write(&mut stream, &packet).await?;
                let response = read(&mut stream).await?;
                if !same_header(&header, &response.header) || now_ms() >= header.expires {
                    bail!("Unix module response identity or deadline mismatch")
                }
                match response.payload {
                    Payload::Response(report)
                        if !report.output_tokens.is_some_and(|n| n > tokens as u64) =>
                    {
                        *slot = Some(stream);
                        Ok(report)
                    }
                    Payload::Error(message) => {
                        *slot = Some(stream);
                        bail!("Unix inference failed: {message}")
                    }
                    _ => bail!("Unexpected Unix module response or exceeded token budget"),
                }
            })
            .await
            .map_err(|_| anyhow!("Unix inference deadline exhausted"))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_process_fixture() {
        use std::io::{Read, Write};
        let Some(path) = std::env::var_os("COGFABRIC_TEST_MODULE_SOCKET") else {
            return;
        };
        let mut stream = std::os::unix::net::UnixStream::connect(path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut prefix = [0u8; 4];
        stream.read_exact(&mut prefix).unwrap();
        let length = u32::from_be_bytes(prefix) as usize;
        assert!(length <= super::super::protocol::MAX_FRAME);
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).unwrap();
        let request = decode(&bytes).unwrap();
        let encoded = encode(&response(request.header)).unwrap();
        stream
            .write_all(&(encoded.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&encoded).unwrap();
    }

    #[tokio::test]
    async fn separate_process_credentials_and_broker_dispatch() {
        use std::os::unix::fs::DirBuilderExt;
        let directory =
            std::env::temp_dir().join(format!("fabric-module-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let socket = directory.join("worker.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fabric::unix::tests::child_process_fixture"])
            .env("COGFABRIC_TEST_MODULE_SOCKET", &socket)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let (stream, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let uid = stream.peer_cred().unwrap().uid();
        let adapter = UnixInference::connected(stream, descriptor(), uid, child.id()).unwrap();
        let broker = super::super::Broker::with_inference(std::sync::Arc::new(adapter));
        assert_eq!(
            broker.generate("solver", "goal", 128).await.unwrap().text,
            "worker answer"
        );
        assert!(child.wait().unwrap().success());
        drop(listener);
        std::fs::remove_dir_all(directory).unwrap();
    }
    fn descriptor() -> ModuleDescriptor {
        ModuleDescriptor {
            id: "inference.worker".into(),
            version: "1.0.0".into(),
            roles: vec!["solver".into()],
        }
    }
    fn peer(stream: UnixStream) -> UnixInference {
        let uid = stream.peer_cred().unwrap().uid();
        UnixInference::connected(stream, descriptor(), uid, std::process::id()).unwrap()
    }
    fn response(header: Header) -> Packet {
        Packet {
            header,
            payload: Payload::Response(Generation {
                text: "worker answer".into(),
                model: "worker-fixture".into(),
                input_tokens: None,
                output_tokens: Some(2),
                elapsed_ms: 1.0,
                queue_wait_ms: None,
            }),
        }
    }
    #[tokio::test]
    async fn authenticates_peer_and_correlates_reusable_task_exchanges() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let uid = client.peer_cred().unwrap().uid();
        assert!(UnixInference::connected(client, descriptor(), uid, 0).is_err());
        let (client, mut server2) = UnixStream::pair().unwrap();
        let adapter = peer(client);
        let task = uuid::Uuid::new_v4().to_string();
        let expected = task.clone();
        let worker = tokio::spawn(async move {
            for _ in 0..2 {
                let request = read(&mut server2).await.unwrap();
                assert_eq!(request.header.task, expected);
                write(&mut server2, &response(request.header))
                    .await
                    .unwrap();
            }
        });
        for _ in 0..2 {
            assert_eq!(
                adapter
                    .generate_for(&task, "solver", "goal", 128)
                    .await
                    .unwrap()
                    .text,
                "worker answer"
            );
        }
        worker.await.unwrap();
        assert!(adapter
            .generate_for("invalid", "solver", "goal", 128)
            .await
            .is_err());
        assert!(adapter
            .generate_for(&task, "critic", "goal", 128)
            .await
            .is_err());
        // The rejected constructor consumed and closed its connection.
        assert_eq!(
            server.read_u8().await.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }
    #[tokio::test]
    async fn forged_response_closes_connection_permanently() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let adapter = peer(client);
        let worker = tokio::spawn(async move {
            let mut request = read(&mut server).await.unwrap();
            request.header.task = uuid::Uuid::new_v4().to_string();
            write(&mut server, &response(request.header)).await.unwrap();
        });
        assert!(adapter.generate("solver", "goal", 128).await.is_err());
        worker.await.unwrap();
        assert!(adapter.stream.lock().await.is_none());
        assert!(adapter
            .generate("solver", "goal", 128)
            .await
            .unwrap_err()
            .to_string()
            .contains("closed"));
    }
    #[tokio::test]
    async fn wrong_credentials_and_oversized_frames_fail_closed() {
        for mismatch in 0..2 {
            let (client, _server) = UnixStream::pair().unwrap();
            let uid = client.peer_cred().unwrap().uid();
            let expected_uid = if mismatch == 0 {
                uid.wrapping_add(1)
            } else {
                uid
            };
            let pid = if mismatch == 1 {
                std::process::id().wrapping_add(1)
            } else {
                std::process::id()
            };
            assert!(UnixInference::connected(client, descriptor(), expected_uid, pid).is_err());
        }
        let (client, mut server) = UnixStream::pair().unwrap();
        let adapter = peer(client);
        let worker = tokio::spawn(async move {
            read(&mut server).await.unwrap();
            server
                .write_u32(super::super::protocol::MAX_FRAME as u32 + 1)
                .await
                .unwrap();
        });
        assert!(adapter.generate("solver", "goal", 128).await.is_err());
        worker.await.unwrap();
        assert!(adapter.stream.lock().await.is_none());
    }
    #[tokio::test]
    async fn cancellation_discards_stream_instead_of_reusing_stale_response() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let adapter = std::sync::Arc::new(peer(client));
        let caller_adapter = adapter.clone();
        let caller =
            tokio::spawn(async move { caller_adapter.generate("solver", "goal", 128).await });
        read(&mut server).await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert!(adapter.stream.lock().await.is_none());
        assert_eq!(
            server.read_u8().await.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }
}
