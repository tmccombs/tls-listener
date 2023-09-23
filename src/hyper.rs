use super::*;
use ::hyper::server::accept::Accept as HyperAccept;
use ::hyper::server::conn::{AddrIncoming, AddrStream};
use std::ops::{Deref, DerefMut};

#[cfg_attr(docsrs, doc(cfg(any(feature = "hyper-h1", feature = "hyper-h2"))))]
impl AsyncAccept for AddrIncoming {
    type Connection = AddrStream;
    type Error = std::io::Error;
    type Address = std::net::SocketAddr;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<(Self::Connection, Self::Address), Self::Error>>> {
        <AddrIncoming as HyperAccept>::poll_accept(self, cx).map_ok(|conn| {
            let remote_addr = conn.remote_addr();
            (conn, remote_addr)
        })
    }
}

pin_project! {
    /// newtype for a [`::hyper::server::accept::Accept`] to impl [`AsyncAccept`]
    ///
    /// Unfortunately, it isn't possible to use a blanket impl, due to coherence rules.
    /// At least until [RFC 1210](https://rust-lang.github.io/rfcs/1210-impl-specialization.html)
    /// (specialization) is stabilized.
    ///
    /// Note that, because `hyper::server::accept::Accept` does not expose the
    /// remote address, the implementation of `AsyncAccept` for `WrappedAccept`
    /// doesn't expose it either. That is, [`AsyncAccept::Address`] is `()` in
    /// this case.
    //#[cfg_attr(docsrs, doc(cfg(any(feature = "hyper-h1", feature = "hyper-h2"))))]
    pub struct WrappedAccept<A> {
        // sadly, pin-project-lite doesn't suport tuple structs :(

        #[pin]
        inner: A
    }
}

/// Wrap any[`::hyper::server::accept::Accept`] as an [`AsyncAccept`].
///
/// This allows you to use any type that implements the hyper `Accept` interface
/// in a [`TlsListener`].
pub fn wrap<A: HyperAccept>(acceptor: A) -> WrappedAccept<A> {
    WrappedAccept { inner: acceptor }
}

#[cfg_attr(docsrs, doc(cfg(any(feature = "hyper-h1", feature = "hyper-h2"))))]
impl<A: HyperAccept> AsyncAccept for WrappedAccept<A>
where
    A::Conn: AsyncRead + AsyncWrite,
{
    type Connection = A::Conn;
    type Error = A::Error;
    type Address = ();

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<(Self::Connection, Self::Address), Self::Error>>> {
        self.project()
            .inner
            .poll_accept(cx)
            .map_ok(|conn| (conn, ()))
    }
}

impl<A: HyperAccept> Deref for WrappedAccept<A> {
    type Target = A;
    fn deref(&self) -> &A {
        &self.inner
    }
}

impl<A: HyperAccept> DerefMut for WrappedAccept<A> {
    fn deref_mut(&mut self) -> &mut A {
        &mut self.inner
    }
}

impl<A: HyperAccept> WrappedAccept<A> {
    /// Conver to the object wrapped by this `WrappedAccept`
    pub fn into_inner(self) -> A {
        self.inner
    }
}

impl<A: HyperAccept, T> TlsListener<WrappedAccept<A>, T>
where
    A::Conn: AsyncWrite + AsyncRead,
    T: AsyncTls<A::Conn>,
{
    /// Create a `TlsListener` from a hyper [`Accept`](::hyper::server::accept::Accept) and tls
    /// acceptor.
    pub fn new_hyper(tls: T, listener: A) -> Self {
        Self::new(tls, wrap(listener))
    }
}

#[cfg_attr(docsrs, doc(cfg(any(feature = "hyper-h1", feature = "hyper-h2"))))]
impl<A, T> HyperAccept for TlsListener<A, T>
where
    A: AsyncAccept,
    A::Error: std::error::Error,
    T: AsyncTls<A::Connection>,
{
    type Conn = T::Stream;
    type Error = Error<A::Error, T::Error, A::Address>;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        self.poll_next(cx).map_ok(|(conn, _)| conn)
    }
}
