use hyper::server::accept;
use hyper::server::conn::AddrIncoming;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server};
use futures_util::stream::StreamExt;
use std::convert::Infallible;
use std::future::ready;

use tls_listener::TlsListener;

mod tls_config;
use tls_config::tls_config;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = ([127, 0, 0, 1], 3000).into();

    let new_svc = make_service_fn(|_| async {
        Ok::<_, Infallible>(service_fn(|_| async {
            Ok::<_, Infallible>(Response::new(Body::from("Hello, World!")))
        }))
    });

    // This uses a filter to handle errors with connecting
    let incoming = TlsListener::new(tls_config(), AddrIncoming::bind(&addr)?).filter(|conn| {
        if let Err(err) = conn {
            eprintln!("Error: {:?}", err);
            ready(false)
        } else {
            ready(true)
        }
    });

    let server = Server::builder(accept::from_stream(incoming)).serve(new_svc);
    server.await?;
    Ok(())
}
