#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Async TLS listener
//!
//! This library is intended to automatically initiate a TLS connection
//! for each new connection in a source of new streams (such as a listening
//! TCP or unix domain socket).

use futures_util::stream::{FuturesUnordered, Stream, StreamExt};
use pin_project_lite::pin_project;
use std::error::Error;
use std::future::Future;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::{timeout, Timeout};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

/// Default number of concurrent handshakes
pub const DEFAULT_MAX_HANDSHAKES: usize = 64;
/// Default timeout for the TLS handshake.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(200);

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

pin_project! {
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
    /// By default, if a client fails the TLS handshake, that is treated as an error, and the
    /// `TlsListener` will return an `Err`. If the `TlsListener` is passed directly to a hyper
    /// `Server`, then an invalid handshake can cause the server to stop accepting connections.
    /// See `http-stream.rs` or `http-low-level` examples, for examples of how to avoid this.
    ///
    /// Note that if the maximum number of pending connections is greater than 1, the resulting
    /// `TlsStream` connections may come in a different order than the connections produced by the
    /// underlying listener.
    ///
    pub struct TlsListener<A: AsyncAccept> {
        #[pin]
        listener: A,
        tls: TlsAcceptor,
        waiting: FuturesUnordered<Timeout<tokio_rustls::Accept<A::Connection>>>,
        max_handshakes: usize,
        timeout: Duration,
    }
}

/// Builder for `TlsListener`.
#[derive(Clone)]
pub struct Builder {
    server_config: Arc<ServerConfig>,
    max_handshakes: usize,
    handshake_timeout: Duration,
}

impl<A: AsyncAccept> TlsListener<A> {
    /// Create a `TlsListener` with default options.
    pub fn new(server_config: ServerConfig, listener: A) -> Self {
        builder(server_config).listen(listener)
    }
}

impl<A> TlsListener<A>
where
    A: AsyncAccept,
    A::Connection: AsyncRead + AsyncWrite + Unpin,
    A::Error: Into<Box<dyn Error + Send + Sync>>,
    Self: Unpin,
{
    /// Accept the next connection
    ///
    /// This is essentially an alias to `self.next()` with a more domain-appropriate name.
    pub fn accept(&mut self) -> impl Future<Output = Option<<Self as Stream>::Item>> + '_ {
        self.next()
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
                    this.waiting
                        .push(timeout(*this.timeout, this.tls.accept(conn)));
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

        loop {
            return match this.waiting.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(conn))) => Poll::Ready(Some(conn)),
                // The handshake timed out, try getting another connection from the
                // queue
                Poll::Ready(Some(Err(_))) => continue,
                _ => Poll::Pending,
            };
        }
    }
}

impl Builder {
    /// Set the maximum number of concurrent handshakes.
    ///
    /// At most `max` handshakes will be concurrently processed. If that limit is
    /// reached, the `TlsListener` will stop polling the underlying listener until a
    /// handshake completes and the encrypted stream has been returned.
    ///
    /// Defaults to `DEFAULT_MAX_HANDSHAKES`.
    pub fn max_handshakes(&mut self, max: usize) -> &mut Self {
        self.max_handshakes = max;
        self
    }

    /// Set the timeout for handshakes.
    ///
    /// If a timeout takes longer than `timeout`, then the handshake will be
    /// aborted and the underlying connection will be dropped.
    ///
    /// Defaults to `DEFAULT_HANDSHAKE_TIMEOUT`.
    pub fn handshake_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.handshake_timeout = timeout;
        self
    }

    /// Create a `TlsListener` from the builder
    ///
    /// Actually build the `TlsListener`. The `listener` argument should be
    /// an implementation of the `AsyncAccept` trait that accepts new connections
    /// that the `TlsListener` will  encrypt using TLS.
    pub fn listen<A: AsyncAccept>(&self, listener: A) -> TlsListener<A> {
        TlsListener {
            listener,
            tls: self.server_config.clone().into(),
            waiting: FuturesUnordered::new(),
            max_handshakes: self.max_handshakes,
            timeout: self.handshake_timeout,
        }
    }
}

/// Create a new Builder for a TlsListener
///
/// `server_config` will be used to configure the TLS sessions.
pub fn builder(server_config: ServerConfig) -> Builder {
    Builder {
        server_config: Arc::new(server_config),
        max_handshakes: DEFAULT_MAX_HANDSHAKES,
        handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
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
