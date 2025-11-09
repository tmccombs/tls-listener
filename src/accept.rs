use pin_project_lite::pin_project;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite};

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
    struct AcceptGenerator<F, A>
    {
        accept: A,
        #[pin]
        current: F,
    }
}

impl<Conn, Addr, E, F, A> AsyncAccept for AcceptGenerator<F, A>
where
    A: FnMut() -> F,
    Conn: AsyncRead + AsyncWrite,
    E: std::error::Error,
    Addr: Debug,
    F: Future<Output = Result<(Conn, Addr), E>>,
{
    type Connection = Conn;
    type Address = Addr;
    type Error = E;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(Self::Connection, Self::Address), Self::Error>> {
        let mut this = self.project();

        let result = this.current.as_mut().poll(cx);
        if result.is_ready() {
            // Prime the future for the next poll
            let next = (this.accept)();
            this.current.set(next);
        }
        result
    }
}

/// Create a new `AsyncAccept` from a generator function
///
/// This allows you to create an `AsyncAccept` implementation from any
/// function that can generate futures of connections.
///
/// `accept_fn` will be called immediately, to create the initial future, then again
/// whenever polling the `Future` returned by the previous call returns a ready status.
///
/// This function is experimental, and may be changed or removed in future versions
pub fn accept_generator<Acc, Conn, Addr, F, E>(
    mut accept_fn: Acc,
) -> impl AsyncAccept<Connection = Conn, Error = E, Address = Addr> + Send
where
    Acc: (FnMut() -> F) + Send,
    Conn: AsyncRead + AsyncWrite + 'static,
    Addr: Debug + 'static,
    F: Future<Output = Result<(Conn, Addr), E>> + Send,
    E: std::error::Error + 'static,
{
    let first_future = (accept_fn)();
    AcceptGenerator {
        accept: accept_fn,
        current: first_future,
    }
}

///An AsyncListener that can also report its local address
pub trait AsyncListener: AsyncAccept {
    /// The local address of the listener, if available.
    fn local_addr(&self) -> Result<Self::Address, Self::Error>;
}
