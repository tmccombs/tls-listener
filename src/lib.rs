#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Async TLS listener
//!
//! This library is intended to automatically initiate a TLS connection
//! for each new connection in a source of new streams (such as a listening
//! TCP or unix domain socket).

mod compile_time_checks {
    #[cfg(not(any(feature = "rustls", feature = "native-tls")))]
    compile_error!("tls-listener requires either the `rustls` or `native-tls` feature");

    #[cfg(all(feature = "rustls", feature = "native-tls"))]
    compile_error!("The `rustls` and `native-tls` features in tls-listener are mutually exclusive");
}

use futures_util::stream::{FuturesUnordered, Stream, StreamExt};
use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::{timeout, Timeout};
#[cfg(feature = "native-tls")]
use tokio_native_tls::{TlsAcceptor, TlsStream};
#[cfg(feature = "rustls")]
use tokio_rustls::{server::TlsStream, TlsAcceptor};

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

#[cfg(feature = "rustls")]
type TlsAcceptFuture<C> = tokio_rustls::Accept<C>;
#[cfg(feature = "native-tls")]
type TlsAcceptFuture<C> =
    Pin<Box<dyn Future<Output = tokio_native_tls::native_tls::Result<TlsStream<C>>> + Send + Sync>>;

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
        waiting: FuturesUnordered<Timeout<TlsAcceptFuture<A::Connection>>>,
        max_handshakes: usize,
        timeout: Duration,
    }
}

/// Builder for `TlsListener`.
#[derive(Clone)]
pub struct Builder {
    acceptor: TlsAcceptor,
    max_handshakes: usize,
    handshake_timeout: Duration,
}

/// Wraps errors from either the listener or the TLS Acceptor
#[derive(Debug, Error)]
pub enum Error<A: std::error::Error> {
    /// An error that arose from the listener ([AsyncAccept::Error])
    #[error("{0}")]
    ListenerError(#[source] A),
    /// An error that occurred during the TLS accept handshake
    #[cfg(feature = "rustls")]
    #[error("{0}")]
    TlsAcceptError(#[source] std::io::Error),
    /// An error that occurred during the TLS accept handshake
    #[cfg(feature = "native-tls")]
    #[error("{0}")]
    TlsAcceptError(#[source] tokio_native_tls::native_tls::Error),
}

impl<A: AsyncAccept> TlsListener<A> {
    /// Create a `TlsListener` with default options.
    pub fn new(acceptor: TlsAcceptor, listener: A) -> Self {
        builder(acceptor).listen(listener)
    }
}

impl<A> TlsListener<A>
where
    A: AsyncAccept,
    A::Connection: AsyncRead + AsyncWrite + Unpin + Sync + Send + 'static,
    A::Error: std::error::Error,
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
    A::Connection: AsyncRead + AsyncWrite + Unpin + Sync + Send + 'static,
    A::Error: std::error::Error,
{
    type Item = Result<TlsStream<A::Connection>, Error<A::Error>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        while this.waiting.len() < *this.max_handshakes {
            match this.listener.as_mut().poll_accept(cx) {
                Poll::Pending => break,
                Poll::Ready(Ok(conn)) => {
                    #[cfg(feature = "rustls")]
                    this.waiting
                        .push(timeout(*this.timeout, this.tls.accept(conn)));
                    #[cfg(feature = "native-tls")]
                    {
                        let tls = this.tls.clone();
                        this.waiting.push(timeout(
                            *this.timeout,
                            Box::pin(async move { tls.accept(conn).await }),
                        ));
                    }
                }
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Some(Err(Error::ListenerError(e))));
                }
            }
        }

        loop {
            return match this.waiting.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(conn))) => {
                    Poll::Ready(Some(conn.map_err(Error::TlsAcceptError)))
                }
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
            tls: self.acceptor.clone(),
            waiting: FuturesUnordered::new(),
            max_handshakes: self.max_handshakes,
            timeout: self.handshake_timeout,
        }
    }
}

/// Create a new Builder for a TlsListener
///
/// `server_config` will be used to configure the TLS sessions.
pub fn builder(acceptor: TlsAcceptor) -> Builder {
    Builder {
        acceptor,
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
        A::Connection: AsyncRead + AsyncWrite + Unpin + Sync + Send + 'static,
        A::Error: std::error::Error,
    {
        type Conn = TlsStream<A::Connection>;
        type Error = Error<A::Error>;

        fn poll_accept(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
            self.poll_next(cx)
        }
    }
}
