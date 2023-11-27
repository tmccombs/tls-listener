use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::tokio::TokioIo;
use std::convert::Infallible;
use tls_listener::TlsListener;
use tokio::net::TcpListener;

mod tls_config;
use tls_config::tls_acceptor;

async fn hello(_: Request<impl hyper::body::Body>) -> Result<Response<String>, Infallible> {
    Ok(Response::new("Hello, World!".into()))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: std::net::SocketAddr = ([127, 0, 0, 1], 3000).into();

    let mut listener = TlsListener::new(tls_acceptor(), TcpListener::bind(addr).await?);

    // We start a loop to continuously accept incoming connections
    loop {
        match listener.accept().await.unwrap() {
            Ok((stream, _)) => {
                let io = TokioIo::new(stream);

                tokio::task::spawn(async move {
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(io, service_fn(hello))
                        .await
                    {
                        println!("Error serving connection: {:?}", err);
                    }
                });
            }
            Err(err) => {
                if let Some(remote_addr) = err.peer_addr() {
                    eprint!("[client {remote_addr}] ");
                }

                eprintln!("Error accepting connection: {}", err);
            }
        }
    }
}
