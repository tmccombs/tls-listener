use futures_util::stream::StreamExt;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::tokio::TokioIo;
use std::convert::Infallible;
use std::future::ready;
use tokio::net::TcpListener;

use tls_listener::TlsListener;

mod tls_config;
use tls_config::tls_acceptor;

async fn hello(_: Request<impl hyper::body::Body>) -> Result<Response<String>, Infallible> {
    Ok(Response::new("Hello, World!".into()))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: std::net::SocketAddr = ([127, 0, 0, 1], 3000).into();

    // This uses a filter to handle errors with connecting
    TlsListener::new(tls_acceptor(), TcpListener::bind(addr).await?)
        .connections()
        .filter_map(|conn| {
            ready(match conn {
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    None
                }
                Ok(c) => Some(TokioIo::new(c)),
            })
        })
        .for_each_concurrent(None, |conn| async {
            if let Err(err) = http1::Builder::new()
                .serve_connection(conn, service_fn(hello))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        })
        .await;

    Ok(())
}
