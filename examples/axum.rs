use axum::{Router, routing::get};
use std::net::SocketAddr;
use tls_listener::TlsListener;
use tokio::net::TcpListener;

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
    let tcp_listener = TcpListener::bind(local_addr).await.unwrap();
    let listener = TlsListener::new(tls_acceptor(), tcp_listener);

    axum::serve(listener, app).await.unwrap();
}
