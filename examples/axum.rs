use axum::{routing::get, Router};
use std::{io, net::SocketAddr, time::Duration};
use tls_listener::TlsListener;
use tokio::net::{TcpListener, TcpStream};

mod tls_config;
use tls_config::tls_acceptor;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    let local_addr = "0.0.0.0:3000".parse::<SocketAddr>().unwrap();
    let tcp_listener = tokio::net::TcpListener::bind(local_addr).await.unwrap();
    let listener = Listener {
        inner: TlsListener::new(tls_acceptor(), tcp_listener),
        local_addr,
    };

    axum::serve(listener, app).await.unwrap();
}

// We use a wrapper type to bridge axum's `Listener` trait to our `TlsListener` type.
struct Listener {
    inner: TlsListener<TcpListener, tls_config::Acceptor>,
    local_addr: SocketAddr,
}

impl axum::serve::Listener for Listener {
    type Io = tls_config::Stream<TcpStream>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.inner.accept().await {
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
        Ok(self.local_addr)
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
