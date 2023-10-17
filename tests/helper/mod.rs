use futures_util::StreamExt;
use tls_listener::TlsListener;
use tokio::io::{copy, split};
use tokio::task::JoinHandle;

mod asserts;
pub(crate) use asserts::*;

mod mocks;
pub use mocks::*;

pub fn setup() -> (MockConnect, TlsListener<MockAccept, MockTls>) {
    let (connect, accept) = accepting();
    (connect, TlsListener::new(MockTls, accept))
}

pub fn setup_echo() -> (MockConnect, JoinHandle<()>) {
    let (connector, listener) = setup();

    let handle = tokio::spawn(listener.for_each_concurrent(None, |s| async {
        let (mut reader, mut writer) = split(s.expect("Unexpected error").0);
        copy(&mut reader, &mut writer)
            .await
            .expect("Failed to copy");
    }));
    (connector, handle)
}
