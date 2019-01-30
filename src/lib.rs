#![deny(missing_docs)]

//! Async TLS listener
//!
//! This library is intended to automatically initiate a TLS connection
//! as for each new connection in a source of new streams (such as a listening
//! TCP or unix domain socket).

use futures::sync::mpsc::{channel, Receiver, Sender};
use futures::{try_ready, Async, Future, Poll, Sink, Stream};
use native_tls::{Identity, Protocol, TlsAcceptorBuilder};
use std::fmt;
use tokio_executor::spawn;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_tls::{TlsAcceptor, TlsStream};

type StreamItem<S> = Result<TlsStream<S>, native_tls::Error>;

/**
 * A `Stream` of `TlsStream`s.
 *
 * This wraps another `Stream` of connections (implementing `AsyncRead` and `AsyncWrite`), and
 * wraps each of those connections in a `TlsStream`.
 *
 * It is similar to:
 *
 * ```ignore
 * tcpListener.incoming().and_then(|s| tlsAcceptor.accept(s))
 * ```
 *
 * except that it has the ability to accept multiple transport-level connections
 * simultaneously while the TLS handshake is pending for other connections. It accomplishes
 * this by spawning a new tokio task for each incoming connection as long as the queue isn't full.
 *
 * Note that if the maximum number of pending connections is greater than 1, the resulting
 * `TlsStream` connections may come in a different order than the connections produced by the
 * underlying listener.
 */
pub struct TlsListener<I>
where
    I: Stream,
{
    listener: I,
    tls: TlsAcceptor,
    rcv: Receiver<StreamItem<I::Item>>,
    tx: Sender<StreamItem<I::Item>>,
    max_pending: usize,
    pending_connections: usize,
}

/**
 * A builder for `TlsListener`.
 */
pub struct Builder {
    tls: TlsAcceptorBuilder,
    max_pending: usize,
}

/**
 * An error that occurs while accepting connections.
 *
 * `E` is the type of the incoming connections of the underlying `Stream`.
 */
#[derive(Debug)]
pub enum Error<E> {
    /**
     * An error that occurred during the TLS handshake.
     */
    Tls(native_tls::Error),
    /**
     * An error that occured while accepting a new connection. This
     */
    Accept(E),
}

impl<E> fmt::Display for Error<E>
where
    E: fmt::Display,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Tls(err) => err.fmt(fmt),
            Error::Accept(err) => err.fmt(fmt),
        }
    }
}

impl<E> ::std::error::Error for Error<E>
where
    E: ::std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> {
        match self {
            Error::Tls(err) => Some(err),
            Error::Accept(err) => Some(err),
        }
    }
}

impl<E> From<native_tls::Error> for Error<E> {
    fn from(err: native_tls::Error) -> Self {
        Error::Tls(err)
    }
}

impl<I> TlsListener<I>
where
    I: Stream,
    I::Item: AsyncRead + AsyncWrite + Send,
{
    /**
     * Create a new `TlsListener`
     *
     * Create a `TlsListener` that accpets incoming connections from `listener`, then performs the
     * TLS handshake using `tls` and returns the resulted `TlsStream`.
     *
     * `max_pending` is the maximum number of pending connections allowed. That is, how many
     * connections are allowed in the queue between being accepted by the underlying listener and
     * completing the hadshake. This value must be non-zero. A value of one ensures that a single
     * connection completes the handshake before the next connection is recieved. If `max_pending`
     * is higher than one, the order of the output is not garanteed to be the same as the order of
     * the input.
     */
    pub fn new(listener: I, tls: TlsAcceptor, max_pending: usize) -> Self {
        assert!(max_pending > 0, "max_pending must be non-zero");
        let (tx, rcv) = channel(max_pending);
        TlsListener {
            listener,
            tls,
            rcv,
            tx,
            max_pending: max_pending,
            pending_connections: 0,
        }
    }
}

impl<I> Stream for TlsListener<I>
where
    I: Stream,
    I::Item: AsyncRead + AsyncWrite + Send + 'static,
{
    type Item = TlsStream<I::Item>;
    type Error = Error<I::Error>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // prioritize ready connections over new connections
        loop {
            match self.rcv.poll().unwrap() {
                Async::Ready(Some(stream)) => {
                    self.pending_connections -= 1;
                    return Ok(Async::Ready(Some(stream?)));
                }
                Async::NotReady => {
                    if self.pending_connections < self.max_pending {
                        if let Some(conn) = try_ready!(self.listener.poll().map_err(Error::Accept))
                        {
                            let accept = self.tls.accept(conn);
                            let tx = self.tx.clone();
                            self.pending_connections += 1;
                            spawn(accept.then(move |result| tx.send(result)).then(|_| Ok(())))
                        } else {
                            // this is unlikely to happen
                            return Ok(Async::Ready(None));
                        }
                    } else {
                        // We've filled up the queue of connections that still need a tls handshake,
                        // so return not-ready
                        return Ok(Async::NotReady);
                    }
                }
                Async::Ready(None) => unreachable!(),
            }
        }
    }
}

const DEFAULT_MAX_PENDING: usize = 64;

impl Builder {
    /**
     * Create a new builder for a `TlsListener`.
     *
     * The `identity` acts as the server's private key/certificate chain.
     */
    pub fn new(identity: Identity) -> Self {
        Builder {
            tls: native_tls::TlsAcceptor::builder(identity),
            max_pending: DEFAULT_MAX_PENDING,
        }
    }

    /**
     * Sets the minimum supported TLS protocol version.
     */
    pub fn min_protocol_version(&mut self, protocol: Option<Protocol>) -> &mut Self {
        self.tls.min_protocol_version(protocol);
        self
    }

    /**
     * Sets the maximum supported TLS protocol version.
     */
    pub fn max_protocol_version(&mut self, protocol: Option<Protocol>) -> &mut Self {
        self.tls.max_protocol_version(protocol);
        self
    }

    /**
     * Set the maximum number of connections to accept from the underlying source while waiting
     * for a TLS handshake to complete.
     */
    pub fn max_pending_connections(&mut self, max: usize) -> &mut Self {
        self.max_pending = max;
        self
    }

    /**
     * Build a new `TlsListener`.
     *
     * `incoming` is the underlying stream of incoming connections.
     */
    pub fn build<I>(&self, incoming: I) -> native_tls::Result<TlsListener<I>>
    where
        I: Stream,
        I::Item: AsyncRead + AsyncWrite + Send,
    {
        self.tls
            .build()
            .map(|tls| TlsListener::new(incoming, tls.into(), self.max_pending))
    }
}
