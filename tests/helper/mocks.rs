use futures_util::future::{self, Ready};
use futures_util::ready;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{self, AtomicU32};
use std::task::{Context, Poll};
use tokio::io::{
    duplex, split, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf,
};
use tokio::sync::mpsc;

use tls_listener::{AsyncAccept, AsyncTls};

type ConnResult = io::Result<(DuplexStream, MockAddress)>;

pub struct MockAccept {
    chan: mpsc::Receiver<ConnResult>,
}

pub struct MockConnect {
    chan: mpsc::Sender<ConnResult>,
    counter: AtomicU32,
}

#[derive(Clone, Copy, Debug)]
pub struct MockAddress(pub u32);

pub fn accepting() -> (MockConnect, MockAccept) {
    let (tx, rx) = mpsc::channel(32);
    (
        MockConnect {
            chan: tx,
            counter: AtomicU32::new(42),
        },
        MockAccept { chan: rx },
    )
}

impl MockConnect {
    pub async fn connect(&self) -> DuplexStream {
        let (tx, rx) = duplex(1024);
        let count = self.counter.fetch_add(1, atomic::Ordering::Relaxed);
        self.chan.send(Ok((rx, MockAddress(count)))).await.unwrap();
        tx
    }

    pub async fn send_error(&self, err: io::Error) {
        self.chan.send(Err(err)).await.unwrap();
    }

    pub async fn send_data(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        let stream = self.connect().await;
        let (mut read, mut write) = split(stream);
        let mut buf = Vec::new();

        tokio::try_join!(
            async move {
                write.write_all(data).await?;
                write.shutdown().await?;
                Ok(())
            },
            read.read_to_end(&mut buf),
        )?;
        Ok(buf)
    }
}

impl AsyncAccept for MockAccept {
    type Connection = DuplexStream;
    type Error = io::Error;
    type Address = MockAddress;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<ConnResult>> {
        Pin::into_inner(self).chan.poll_recv(cx)
    }
}

#[derive(Clone)]
pub struct MockTls;

impl AsyncTls<DuplexStream> for MockTls {
    type Stream = MockTlsStream;
    type Error = io::Error;
    type AcceptFuture = Ready<io::Result<MockTlsStream>>;

    fn accept(&self, conn: DuplexStream) -> Self::AcceptFuture {
        future::ready(Ok(MockTlsStream(conn)))
    }
}

#[derive(Debug)]
pub struct MockTlsStream(DuplexStream);

impl MockTlsStream {
    fn inner(self: Pin<&mut Self>) -> Pin<&mut DuplexStream> {
        Pin::new(&mut Pin::into_inner(self).0)
    }
}

impl AsyncRead for MockTlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        ready!(self.inner().poll_read(cx, buf))?;
        buf.filled_mut().make_ascii_uppercase();
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for MockTlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let data = buf.to_ascii_lowercase();
        self.inner().poll_write(cx, &*data)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.inner().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.inner().poll_shutdown(cx)
    }
}
