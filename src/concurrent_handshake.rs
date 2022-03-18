use super::AsyncTls;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinHandle;

/// See [`AsyncTls::concurrent_handshakes`]
#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
#[derive(Clone, Debug)]
pub struct ConcurrentHandshakeTls<T>(pub(crate) T);
impl<C, T> AsyncTls<C> for ConcurrentHandshakeTls<T>
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

impl<T> ConcurrentHandshakeTls<T> {
    /// Convert a [`ConcurrentHandshakeTls`] back into the inner type that it wraps.
    pub fn into_inner(self) -> T {
        self.0
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
pin_project! {
    /// Future type returned by [`ConcurrentHandshakeTls::accept`];
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
