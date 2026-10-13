//! Bound accepted HTTP connections and their lifetimes before header parsing.
use axum::serve::Listener;
use std::{
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Sleep,
};
pub(crate) struct BoundedListener {
    inner: TcpListener,
    slots: Arc<Semaphore>,
}
impl BoundedListener {
    pub(crate) fn new(inner: TcpListener) -> Self {
        Self {
            inner,
            slots: Arc::new(Semaphore::new(32)),
        }
    }
}
pub(crate) struct BoundedIo {
    inner: TcpStream,
    _slot: OwnedSemaphorePermit,
    deadline: Pin<Box<Sleep>>,
}
impl BoundedIo {
    fn check_deadline(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        use std::future::Future;
        if self.deadline.as_mut().poll(cx).is_ready() {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP connection lifetime exhausted",
            ))
        } else {
            Ok(())
        }
    }
}
impl Listener for BoundedListener {
    type Io = BoundedIo;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let slot = self
                .slots
                .clone()
                .acquire_owned()
                .await
                .expect("Connection semaphore is never closed");
            match self.inner.accept().await {
                Ok((inner, addr)) => {
                    return (
                        BoundedIo {
                            inner,
                            _slot: slot,
                            deadline: Box::pin(tokio::time::sleep(Duration::from_secs(180))),
                        },
                        addr,
                    )
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}
impl AsyncRead for BoundedIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.check_deadline(cx)?;
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl AsyncWrite for BoundedIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.check_deadline(cx)?;
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.check_deadline(cx)?;
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    #[tokio::test]
    async fn connection_slots_release_and_expired_io_fails_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut listener = BoundedListener::new(listener);
        let mut clients = Vec::new();
        let mut accepted = Vec::new();
        for _ in 0..32 {
            clients.push(TcpStream::connect(addr).await.unwrap());
            accepted.push(listener.accept().await.0);
        }
        assert_eq!(listener.slots.available_permits(), 0);
        clients.push(TcpStream::connect(addr).await.unwrap());
        assert!(
            tokio::time::timeout(Duration::from_millis(10), listener.accept())
                .await
                .is_err()
        );
        drop(accepted.pop());
        let mut io = tokio::time::timeout(Duration::from_secs(1), listener.accept())
            .await
            .unwrap()
            .0;
        io.deadline = Box::pin(tokio::time::sleep(Duration::ZERO));
        let mut byte = [0];
        assert_eq!(
            io.read(&mut byte).await.unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        drop(io);
        drop(accepted);
        assert_eq!(listener.slots.available_permits(), 32);
    }
}
