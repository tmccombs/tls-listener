#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Async TLS listener
//!
//! This library is intended to automatically initiate a TLS connection
//! for each new connection in a source of new streams (such as a listening
//! TCP or unix domain socket).

use futures_util::stream::{FuturesUnordered, Stream, StreamExt};
use pin_project::pin_project;
use std::error::Error;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

/// Asynchronously accept connections.
pub trait AsyncAccept {
    /// The type of the connection that is accepted.
    type Connection;
    /// The type of error that may be returned.
    type Error;

    /// Poll to accept the next connection.
    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Connection, Self::Error>>;
}

///
/// Wraps a `Stream` of connections (such as a TCP listener) so that each connection is itself
/// encrypted using TLS.
///
/// It is similar to:
///
/// ```ignore
/// tcpListener.and_then(|s| tlsAcceptor.accept(s))
/// ```
///
/// except that it has the ability to accept multiple transport-level connections
/// simultaneously while the TLS handshake is pending for other connections.
///
/// At most `buffer_size` connections will be accepted from `listener` and buffered while waiting
/// for the TLS handshake to complete.
///
/// The configuration for the TLS session comes from the `tls` argument.
///
/// Note that if the maximum number of pending connections is greater than 1, the resulting
/// `TlsStream` connections may come in a different order than the connections produced by the
/// underlying listener.
///
#[pin_project]
pub struct TlsListener<A: AsyncAccept> {
    #[pin]
    listener: A,
    tls: TlsAcceptor,
    waiting: FuturesUnordered<tokio_rustls::Accept<A::Connection>>,
    max_handshakes: usize,
}

impl<A> TlsListener<A>
where
    A: AsyncAccept,
    A::Connection: AsyncRead + AsyncWrite + Unpin,
{
    /// Create a new TlsListener.
    ///
    /// At most `max_handshakes` handshakes will be concurrently processed. If that limit is
    /// reached, this stream will stop polling `listener` until a handshake completes and the
    /// encrypted stream has been returned.
    ///
    /// `server_config` provides configuration for the TLS sessions.
    pub fn new(listener: A, server_config: ServerConfig, max_handshakes: usize) -> Self {
        Self {
            listener,
            tls: Arc::new(server_config).into(),
            waiting: FuturesUnordered::new(),
            max_handshakes,
        }
    }
}

impl<A> Stream for TlsListener<A>
where
    A: AsyncAccept,
    A::Connection: AsyncRead + AsyncWrite + Unpin,
    A::Error: Into<Box<dyn Error + Send + Sync>>,
{
    type Item = io::Result<TlsStream<A::Connection>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        while this.waiting.len() < *this.max_handshakes {
            match this.listener.as_mut().poll_accept(cx) {
                Poll::Pending => break,
                Poll::Ready(Ok(conn)) => {
                    this.waiting.push(this.tls.accept(conn));
                }
                Poll::Ready(Err(e)) => {
                    // Ideally we'd be able to do this match at compile time, but afaik,
                    // there isn't a way to do that with current rust.
                    let error = match e.into().downcast::<io::Error>() {
                        Ok(err) => *err,
                        Err(err) => io::Error::new(io::ErrorKind::ConnectionAborted, err),
                    };
                    return Poll::Ready(Some(Err(error)));
                }
            }
        }

        match this.waiting.poll_next_unpin(cx) {
            Poll::Ready(None) => Poll::Pending,
            x => x,
        }
    }
}

#[cfg(feature = "tokio-net")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio-net")))]
impl AsyncAccept for tokio::net::TcpListener {
    type Connection = tokio::net::TcpStream;
    type Error = io::Error;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Connection, Self::Error>> {
        match (*self).poll_accept(cx) {
            Poll::Ready(Ok((stream, _))) => Poll::Ready(Ok(stream)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(all(unix, feature = "tokio-net"))]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio-net")))]
impl AsyncAccept for tokio::net::UnixListener {
    type Connection = tokio::net::UnixStream;
    type Error = io::Error;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Connection, Self::Error>> {
        match (*self).poll_accept(cx) {
            Poll::Ready(Ok((stream, _))) => Poll::Ready(Ok(stream)),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

// Possibly make a blanket implementation for hyper::server::accept::Accept?

#[cfg(any(feature = "hyper-h1", feature = "hyper-h2"))]
mod hyper_impl {
    use super::*;
    use hyper::server::accept::Accept as HyperAccept;
    use hyper::server::conn::{AddrIncoming, AddrStream};

    #[cfg_attr(docsrs, doc(cfg(any(feature = "hyper-h1", feature = "hyper-h2"))))]
    impl AsyncAccept for AddrIncoming {
        type Connection = AddrStream;
        type Error = io::Error;

        fn poll_accept(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<Self::Connection, Self::Error>> {
            match <AddrIncoming as HyperAccept>::poll_accept(self, cx) {
                Poll::Ready(Some(res)) => Poll::Ready(res),
                Poll::Pending => Poll::Pending,
                Poll::Ready(None) => unreachable!("None returned from AddrIncoming"),
            }
        }
    }

    #[cfg_attr(docsrs, doc(cfg(any(feature = "hyper-h1", feature = "hyper-h2"))))]
    impl<A> HyperAccept for TlsListener<A>
    where
        A: AsyncAccept,
        A::Connection: AsyncRead + AsyncWrite + Unpin,
        A::Error: Into<Box<dyn Error + Send + Sync>>,
    {
        type Conn = TlsStream<A::Connection>;
        type Error = io::Error;

        fn poll_accept(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
            self.poll_next(cx)
        }
    }
}
