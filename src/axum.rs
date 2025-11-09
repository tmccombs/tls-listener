use super::{AsyncAccept, AsyncListener, AsyncTls, Error, TlsListener};
use std::io;
use std::marker::Unpin;

use axum::serve::Listener;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::error;

#[cfg_attr(docsrs, doc(cfg(feature = "axum")))]
impl<A, T> Listener for TlsListener<A, T>
where
    Self: Unpin + Send + 'static,
    A: AsyncAccept<Error = std::io::Error> + AsyncListener,
    A::Address: Send,
    T: AsyncTls<A::Connection>,
    T::Error: Send,
    T::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Io = T::Stream;
    type Addr = A::Address;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match TlsListener::accept(self).await {
                Ok(conn) => break conn,
                // Is there something better we could do here?
                // log with tracing library?
                Err(Error::ListenerError(e)) => handle_accept_error(e).await,
                Err(e) => error!("TLS accept error: {}", e),
            }
        }
    }

    fn local_addr(&self) -> tokio::io::Result<Self::Addr> {
        self.listener().local_addr()
    }
}

// This mirrors https://github.com/tokio-rs/axum/blob/8954d7922a7e81de0cff50078c47381299776897/axum/src/serve/listener.rs#L245
// which in turn references https://github.com/hyperium/hyper/blob/v0.14.27/src/server/tcp.rs#L186
//
// > A possible scenario is that the process has hit the max open files
// > allowed, and so trying to accept a new connection will fail with
// > `EMFILE`. In some cases, it's preferable to just wait for some time, if
// > the application will likely close some files (or connections), and try
// > to accept the connection again. If this option is `true`, the error
// > will be logged at the `error` level, since it is still a big deal,
// > and then the listener will sleep for 1 second.
//
// TODO: have a way to customize error handling
async fn handle_accept_error(e: io::Error) {
    if is_connection_error(&e) {
        return;
    }
    error!("accept error: {e}");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}

fn is_connection_error(e: &io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        e.kind(),
        ConnectionRefused | ConnectionAborted | ConnectionReset
    )
}
