use futures_util::future::{self, Ready};
use std::io::{Error, ErrorKind, Result};

mod helper;

use helper::*;
use helper::{assert_ascii_eq, assert_err};
use tokio::io::{AsyncWriteExt, DuplexStream};
use tokio::spawn;
use tokio::sync::oneshot;

use futures_util::StreamExt;

use tls_listener::Error::*;
use tls_listener::{AsyncTls, TlsListener};

#[tokio::test]
async fn accept_connections() {
    let (connecter, listener) = setup();

    spawn(listener.for_each_concurrent(None, |s| async {
        s.expect("unexpected error")
            .0
            .write_all(b"HELLO, WORLD!")
            .await
            .unwrap();
    }));

    assert_ascii_eq!(
        connecter.send_data(b"hello, bob.").await.unwrap(),
        b"hello, world!"
    );
    assert_ascii_eq!(
        connecter.send_data(b"hello, orange.").await.unwrap(),
        b"hello, world!"
    );
    assert_ascii_eq!(
        connecter.send_data(b"hello, banana.").await.unwrap(),
        b"hello, world!"
    );
}

#[tokio::test]
async fn stream_error() {
    let (connecter, mut listener) = setup();

    connecter
        .send_error(Error::new(ErrorKind::ConnectionReset, "test"))
        .await;
    assert_err!(listener.accept().await, ListenerError(_));
}

#[tokio::test]
async fn tls_error() {
    #[derive(Clone)]
    struct ErrTls;
    impl AsyncTls<DuplexStream> for ErrTls {
        type Stream = DuplexStream;
        type Error = Error;
        type AcceptFuture = Ready<Result<DuplexStream>>;

        fn accept(&self, _: DuplexStream) -> Self::AcceptFuture {
            future::ready(Err(Error::new(ErrorKind::ConnectionReset, "test")))
        }
    }
    let (connect, accept) = accepting();
    spawn(async move { connect.send_data(b"foo").await });
    let mut listener = TlsListener::new(ErrTls, accept);

    assert_err!(
        listener.accept().await,
        TlsAcceptError {
            peer_addr: MockAddress(42),
            ..
        }
    );
}

static LONG_TEXT: &'static [u8] = include_bytes!("long_text.txt");

#[tokio::test]
async fn echo() {
    let (ender, ended) = oneshot::channel();
    let (connector, listener) = setup_echo(ended);

    async fn check_message(c: &MockConnect, msg: &[u8]) -> () {
        let resp = c.send_data(msg).await;
        assert_ascii_eq!(resp.unwrap(), msg.to_ascii_lowercase());
    }

    let c = &connector;

    tokio::join!(
        check_message(c, b"test"),
        check_message(c, b"blue CheEse"),
        check_message(
            c,
            b"This is some text, that is a little longer than the other ones."
        ),
        check_message(c, LONG_TEXT),
        check_message(c, LONG_TEXT),
    );
    ender.send(()).unwrap();

    if let Err(e) = listener.await {
        std::panic::resume_unwind(e.into_panic());
    }
}

#[tokio::test]
async fn addr() {
    let (connector, mut listener) = setup();

    spawn(async move {
        connector.send_data(b"hi").await.unwrap();
        connector.send_data(b"boo").await.unwrap();
        connector.send_data(b"test").await.unwrap();
    });

    for i in 42..44 {
        assert_eq!(listener.accept().await.unwrap().1, MockAddress(i));
    }
}
