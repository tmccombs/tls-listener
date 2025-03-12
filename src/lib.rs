#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Async TLS listener
//!
//! This library is intended to automatically initiate a TLS connection
//! for each new connection in a source of new streams (such as a listening
//! TCP or unix domain socket).
//!
//! # Features:
//! - `tokio-net`: Implementations for tokio socket types (default)
//! - `rt`: Features that depend on the tokio runtime, such as [`SpawningHandshakes`]
//! - `rustls-core`: Support the tokio-rustls backend for tls
//! - `rustls-aws-lc`: Include the aws-lc provider for rustls
//! - `rustls-ring`: Include the ring provider for rustls
//! - `rustls-fips`: Include enabling the "fips" feature for rustls
//! - `native-tls`: support the tokio-native-tls backend for tls

use futures_util::stream::{FuturesUnordered, Stream, StreamExt, TryStreamExt};
use pin_project_lite::pin_project;
#[cfg(feature = "rt")]
pub use spawning_handshake::SpawningHandshakes;
use std::fmt::Debug;
use std::future::{poll_fn, Future};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::{timeout, Timeout};
#[cfg(feature = "native-tls")]
pub use tokio_native_tls as native_tls;
#[cfg(feature = "openssl")]
pub use tokio_openssl as openssl;
#[cfg(feature = "rustls-core")]
pub use tokio_rustls as rustls;

#[cfg(feature = "rt")]
mod spawning_handshake;

#[cfg(feature = "tokio-net")]
mod net;

/// Default number of connections to accept in a batch before trying to
pub const DEFAULT_ACCEPT_BATCH_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(64) };
/// Default timeout for the TLS handshake.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Trait for TLS implementation.
///
/// Implementations are provided by the rustls and native-tls features.
pub trait AsyncTls<C: AsyncRead + AsyncWrite>: Clone {
    /// The type of the TLS stream created from the underlying stream.
    type Stream;
    /// Error type for completing the TLS handshake
    type Error: std::error::Error;
    /// Type of the Future for the TLS stream that is accepted.
    type AcceptFuture: Future<Output = Result<Self::Stream, Self::Error>>;

    /// Accept a TLS connection on an underlying stream
    fn accept(&self, stream: C) -> Self::AcceptFuture;
}

/// Asynchronously accept connections.
pub trait AsyncAccept {
    /// The type of the connection that is accepted.
    type Connection: AsyncRead + AsyncWrite;
    /// The type of the remote address, such as [`std::net::SocketAddr`].
    ///
    /// If no remote address can be determined (such as for mock connections),
    /// `()` or a similar dummy type can be used.
    type Address: Debug;
    /// The type of error that may be returned.
    type Error: std::error::Error;

    /// Poll to accept the next connection.
    ///
    /// On success return the new connection, and the address of the peer.
    #[allow(clippy::type_complexity)]
    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::Connection, Self::Address), Self::Error>>;
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
    /// `TlsListener` will return an `Err`. If the error is not handled, then an invalid handshake can
    /// cause the server to stop accepting connections.
    /// See [`http-stream.rs`][2] or [`http-low-level`][3] examples, for examples of how to avoid this.
    ///
    /// Note that if the maximum number of pending connections is greater than 1, the resulting
    /// [`T::Stream`][4] connections may come in a different order than the connections produced by the
    /// underlying listener.
    ///
    /// [2]: https://github.com/tmccombs/tls-listener/blob/main/examples/http-stream.rs
    /// [3]: https://github.com/tmccombs/tls-listener/blob/main/examples/http-low-level.rs
    /// [4]: AsyncTls::Stream
    ///
    pub struct TlsListener<A: AsyncAccept, T: AsyncTls<A::Connection>> {
        #[pin]
        listener: A,
        tls: T,
        waiting: FuturesUnordered<Waiting<A, T>>,
        accept_batch_size: NonZeroUsize,
        timeout: Duration,
    }
}

/// Builder for `TlsListener`.
#[derive(Clone)]
pub struct Builder<T> {
    tls: T,
    accept_batch_size: NonZeroUsize,
    handshake_timeout: Duration,
}

/// Wraps errors from either the listener or the TLS Acceptor
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error<LE: std::error::Error, TE: std::error::Error, Addr> {
    /// An error that arose from the listener ([AsyncAccept::Error])
    #[error("{0}")]
    ListenerError(#[source] LE),
    /// An error that occurred during the TLS accept handshake
    #[error("{error}")]
    #[non_exhaustive]
    TlsAcceptError {
        /// The original error that occurred
        #[source]
        error: TE,

        /// Address of the other side of the connection
        peer_addr: Addr,
    },
    /// The TLS handshake timed out
    #[error("Timeout during TLS handshake")]
    #[non_exhaustive]
    HandshakeTimeout {
        /// Address of the other side of the connection
        peer_addr: Addr,
    },
}

impl<A: AsyncAccept, T> TlsListener<A, T>
where
    T: AsyncTls<A::Connection>,
{
    /// Create a `TlsListener` with default options.
    pub fn new(tls: T, listener: A) -> Self {
        builder(tls).listen(listener)
    }
}

/// Convenience type alias to get the proper error type from the type of the [`AsyncAccept`] and
/// [`AsyncTls`] used.
type TlsListenerError<A, T> = Error<
    <A as AsyncAccept>::Error,
    <T as AsyncTls<<A as AsyncAccept>::Connection>>::Error,
    <A as AsyncAccept>::Address,
>;

impl<A, T> TlsListener<A, T>
where
    A: AsyncAccept,
    T: AsyncTls<A::Connection>,
{
    /// Poll accepting a connection.
    ///
    /// This will return ready once the TLS handshake has completed on an incoming
    /// connection and return the connection and the source address.
    pub fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<<Self as Stream>::Item> {
        let mut this = self.project();

        loop {
            let mut empty_listener = false;
            for _ in 0..this.accept_batch_size.get() {
                match this.listener.as_mut().poll_accept(cx) {
                    Poll::Pending => {
                        empty_listener = true;
                        break;
                    }
                    Poll::Ready(Ok((conn, addr))) => {
                        this.waiting.push(Waiting {
                            inner: timeout(*this.timeout, this.tls.accept(conn)),
                            peer_addr: Some(addr),
                        });
                    }
                    Poll::Ready(Err(e)) => {
                        return Poll::Ready(Err(Error::ListenerError(e)));
                    }
                }
            }

            match this.waiting.poll_next_unpin(cx) {
                Poll::Ready(Some(result)) => return Poll::Ready(result),
                // If we don't have anything waiting yet,
                // then we are still pending,
                Poll::Ready(None) | Poll::Pending => {
                    if empty_listener {
                        return Poll::Pending;
                    }
                }
            }
        }
    }

    /// Accept the next connection
    ///
    /// This is similar to `self.next()`, but doesn't return an `Option` because
    /// there isn't an end condition on accepting connections,
    /// and has a more domain-appropriate name.
    ///
    /// The future returned is "cancellation safe".
    pub fn accept(&mut self) -> impl Future<Output = <Self as Stream>::Item> + '_
    where
        Self: Unpin,
    {
        let mut pinned = Pin::new(self);
        poll_fn(move |cx| pinned.as_mut().poll_accept(cx))
    }

    /// Replaces the Tls Acceptor configuration, which will be used for new connections.
    ///
    /// This can be used to change the certificate used at runtime.
    pub fn replace_acceptor(&mut self, acceptor: T) {
        self.tls = acceptor;
    }

    /// Replaces the Tls Acceptor configuration from a pinned reference to `Self`.
    ///
    /// This is useful if your listener is `!Unpin`.
    ///
    /// This can be used to change the certificate used at runtime.
    pub fn replace_acceptor_pin(self: Pin<&mut Self>, acceptor: T) {
        *self.project().tls = acceptor;
    }

    /// Convert into a Stream of connections.
    ///
    /// This drops the address of the connection, but provides a more convenient API
    /// if the address isn't needed.
    ///
    /// The address will still be included in errors.
    pub fn connections(self) -> impl Stream<Item = Result<T::Stream, TlsListenerError<A, T>>> {
        self.map_ok(|(conn, _addr)| conn)
    }

    /// Get a reference to the underlying connection listener
    ///
    /// Can be useful to get metadata about the listener, such as the
    /// local address.
    pub fn listener(&self) -> &A {
        &self.listener
    }
}

impl<A, T> Stream for TlsListener<A, T>
where
    A: AsyncAccept,
    T: AsyncTls<A::Connection>,
{
    type Item = Result<(T::Stream, A::Address), TlsListenerError<A, T>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.poll_accept(cx).map(Some)
    }
}

#[cfg(feature = "rustls-core")]
#[cfg_attr(docsrs, doc(cfg(feature = "rustls-core")))]
impl<C: AsyncRead + AsyncWrite + Unpin> AsyncTls<C> for tokio_rustls::TlsAcceptor {
    type Stream = tokio_rustls::server::TlsStream<C>;
    type Error = std::io::Error;
    type AcceptFuture = tokio_rustls::Accept<C>;

    fn accept(&self, conn: C) -> Self::AcceptFuture {
        tokio_rustls::TlsAcceptor::accept(self, conn)
    }
}

#[cfg(feature = "native-tls")]
#[cfg_attr(docsrs, doc(cfg(feature = "native-tls")))]
impl<C> AsyncTls<C> for tokio_native_tls::TlsAcceptor
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Stream = tokio_native_tls::TlsStream<C>;
    type Error = tokio_native_tls::native_tls::Error;
    type AcceptFuture = Pin<Box<dyn Future<Output = Result<Self::Stream, Self::Error>> + Send>>;

    fn accept(&self, conn: C) -> Self::AcceptFuture {
        let tls = self.clone();
        Box::pin(async move { tokio_native_tls::TlsAcceptor::accept(&tls, conn).await })
    }
}

#[cfg(feature = "openssl")]
#[cfg_attr(docsrs, doc(cfg(feature = "openssl")))]
impl<C> AsyncTls<C> for openssl_impl::ssl::SslContext
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Stream = tokio_openssl::SslStream<C>;
    type Error = openssl_impl::ssl::Error;
    type AcceptFuture = Pin<Box<dyn Future<Output = Result<Self::Stream, Self::Error>> + Send>>;

    fn accept(&self, conn: C) -> Self::AcceptFuture {
        let ssl = match openssl_impl::ssl::Ssl::new(self) {
            Ok(s) => s,
            Err(e) => {
                return Box::pin(futures_util::future::err(e.into()));
            }
        };
        let mut stream = match tokio_openssl::SslStream::new(ssl, conn) {
            Ok(s) => s,
            Err(e) => {
                return Box::pin(futures_util::future::err(e.into()));
            }
        };
        Box::pin(async move {
            Pin::new(&mut stream).accept().await?;
            Ok(stream)
        })
    }
}

impl<T> Builder<T> {
    /// Set the size of batches of incoming connections to accept at once
    ///
    /// When polling for a new connection, the `TlsListener` will first check
    /// for incomming connections on the listener that need to start a TLS handshake.
    /// This specifies the maximum number of connections it will accept before seeing if any
    /// TLS connections are ready.
    ///
    /// Having a limit for this ensures that ready TLS conections aren't starved if there are a
    /// large number of incoming connections.
    ///
    /// Defaults to `DEFAULT_ACCEPT_BATCH_SIZE`.
    pub fn accept_batch_size(&mut self, size: NonZeroUsize) -> &mut Self {
        self.accept_batch_size = size;
        self
    }

    /// Set the timeout for handshakes.
    ///
    /// If a timeout takes longer than `timeout`, then the handshake will be
    /// aborted and the underlying connection will be dropped.
    ///
    /// The default is fairly conservative, to avoid dropping connections. It is
    /// recommended that you adjust this to meet the specific needs of your use case
    /// in production deployments.
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
    pub fn listen<A: AsyncAccept>(&self, listener: A) -> TlsListener<A, T>
    where
        T: AsyncTls<A::Connection>,
    {
        TlsListener {
            listener,
            tls: self.tls.clone(),
            waiting: FuturesUnordered::new(),
            accept_batch_size: self.accept_batch_size,
            timeout: self.handshake_timeout,
        }
    }
}

impl<LE: std::error::Error, TE: std::error::Error, A> Error<LE, TE, A> {
    /// Get the peer address from the connection that caused the error, if applicable.
    ///
    /// This will only return Some for errors that occur after an initial connection
    /// is established, such as TlsAcceptError and HandshakeTimeout. And only if
    /// the [`AsyncAccept`] implementation implements [`peer_addr`](AsyncAccept::peer_addr)
    pub fn peer_addr(&self) -> Option<&A> {
        match self {
            Error::TlsAcceptError { peer_addr, .. } | Self::HandshakeTimeout { peer_addr, .. } => {
                Some(peer_addr)
            }
            _ => None,
        }
    }
}

/// Create a new Builder for a TlsListener
///
/// `server_config` will be used to configure the TLS sessions.
pub fn builder<T>(tls: T) -> Builder<T> {
    Builder {
        tls,
        accept_batch_size: DEFAULT_ACCEPT_BATCH_SIZE,
        handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
    }
}

pin_project! {
    struct Waiting<A, T>
    where
        A: AsyncAccept,
        T: AsyncTls<A::Connection>
    {
        #[pin]
        inner: Timeout<T::AcceptFuture>,
        peer_addr: Option<A::Address>,
    }
}

impl<A, T> Future for Waiting<A, T>
where
    A: AsyncAccept,
    T: AsyncTls<A::Connection>,
{
    type Output = Result<(T::Stream, A::Address), TlsListenerError<A, T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        let res = ready!(this.inner.as_mut().poll(cx));
        let addr = this
            .peer_addr
            .take()
            .expect("this future has already been polled to completion");
        match res {
            // We succesfully got a connection
            Ok(Ok(conn)) => Poll::Ready(Ok((conn, addr))),
            // The handshake failed
            Ok(Err(e)) => Poll::Ready(Err(Error::TlsAcceptError {
                error: e,
                peer_addr: addr,
            })),
            // The handshake timed out
            Err(_) => Poll::Ready(Err(Error::HandshakeTimeout { peer_addr: addr })),
        }
    }
}
