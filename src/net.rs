use super::{AsyncAccept, AsyncListener};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::{TcpListener, TcpStream};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

#[cfg_attr(docsrs, doc(cfg(feature = "tokio-net")))]
impl AsyncAccept for TcpListener {
    type Connection = TcpStream;
    type Error = io::Error;
    type Address = std::net::SocketAddr;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::Connection, Self::Address), Self::Error>> {
        match (*self).poll_accept(cx) {
            Poll::Ready(Ok(conn)) => Poll::Ready(Ok(conn)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "tokio-net")))]
impl AsyncListener for TcpListener {
    #[inline]
    fn local_addr(&self) -> Result<Self::Address, Self::Error> {
        TcpListener::local_addr(self)
    }
}

#[cfg(unix)]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio-net")))]
impl AsyncAccept for UnixListener {
    type Connection = UnixStream;
    type Error = io::Error;
    type Address = tokio::net::unix::SocketAddr;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::Connection, Self::Address), Self::Error>> {
        match (*self).poll_accept(cx) {
            Poll::Ready(Ok(conn)) => Poll::Ready(Ok(conn)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "tokio-net")))]
impl AsyncListener for UnixListener {
    #[inline]
    fn local_addr(&self) -> Result<Self::Address, Self::Error> {
        UnixListener::local_addr(self)
    }
}
