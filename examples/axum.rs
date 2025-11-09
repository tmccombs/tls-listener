use axum::{routing::get, Router};
use std::{io, net::SocketAddr, time::Duration};
use tls_listener::TlsListener;
use tokio::net::{TcpListener, TcpStream};

mod tls_config;
use tls_config::tls_acceptor;

/// An example of running an axum server with `TlsListener`.
///
/// One can also bypass `axum::serve` and use the `Router` with Hyper's `serve_connection` API
/// directly. The main advantages of using `axum::serve` are that
/// - graceful shutdown is made easy with axum's `.with_graceful_shutdown` API, and
/// - the Hyper server is configured by axum itself, allowing options specific to axum to be set
///   (for example, axum currently enables the `CONNECT` protocol in order to support HTTP/2
///   websockets).
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    let local_addr = "0.0.0.0:3000".parse::<SocketAddr>().unwrap();
    let tcp_listener = tokio::net::TcpListener::bind(local_addr).await.unwrap();
    let listener = Listener(TlsListener::new(tls_acceptor(), tcp_listener));

    axum::serve(listener, app).await.unwrap();
}

// We use a wrapper type to bridge axum's `Listener` trait to our `TlsListener` type.
struct Listener(TlsListener<TcpListener, tls_config::Acceptor>);

impl axum::serve::Listener for Listener {
    type Io = tls_config::Stream<TcpStream>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            // To change the TLS certificate dynamically, you could `select!` on this call with a
            // channel receiver, and call `self.inner.replace_acceptor` in the other branch.
            match self.0.accept().await {
                Ok(tuple) => break tuple,
                Err(tls_listener::Error::ListenerError(e)) if !is_connection_error(&e) => {
                    // See https://github.com/tokio-rs/axum/blob/da3539cb0e5eed381361b2e688a776da77c52cd6/axum/src/serve/listener.rs#L145-L157
                    // for the rationale.
                    tokio::time::sleep(Duration::from_secs(1)).await
                }
                Err(_) => continue,
            }
        }
    }
    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.0.local_addr()
    }
}

// Taken from https://github.com/tokio-rs/axum/blob/da3539cb0e5eed381361b2e688a776da77c52cd6/axum/src/serve/listener.rs#L160-L167
fn is_connection_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
    )
}
