use super::AsyncTls;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinHandle;

/// Convert an [`AsyncTls`] into one that will spawn a new task for each new connection.
///
/// This will wrap each call to [`accept`](AsyncTls::accept) with a call to [`tokio::spawn`]. This
/// is especially useful when using a multi-threaded runtime, so that the TLS handshakes
/// are distributed between multiple threads.
#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
#[derive(Clone, Debug)]
pub struct SpawningHandshakes<T>(pub T);

impl<C, T> AsyncTls<C> for SpawningHandshakes<T>
where
    T: AsyncTls<C>,
    C: AsyncRead + AsyncWrite,
    T::AcceptFuture: Send + 'static,
    T::Stream: Send + 'static,
    T::Error: Send + 'static,
{
    type Stream = T::Stream;
    type Error = T::Error;
    type AcceptFuture = HandshakeJoin<T::Stream, T::Error>;

    fn accept(&self, stream: C) -> Self::AcceptFuture {
        HandshakeJoin {
            inner: tokio::spawn(self.0.accept(stream)),
        }
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
pin_project! {
    /// Future type returned by [`SpawningHandshakeTls::accept`];
    pub struct HandshakeJoin<Stream, Error>{
        #[pin]
        inner: JoinHandle<Result<Stream, Error>>
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
impl<Stream, Error> Future for HandshakeJoin<Stream, Error> {
    type Output = Result<Stream, Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().inner.poll(cx) {
            Poll::Ready(Ok(v)) => Poll::Ready(v),
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => {
                if e.is_panic() {
                    std::panic::resume_unwind(e.into_panic());
                } else {
                    panic!("Tls handshake was aborted: {:?}", e);
                }
            }
        }
    }
}
