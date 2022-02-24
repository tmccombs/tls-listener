use hyper::server::conn::AddrIncoming;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server};
use std::convert::Infallible;
use tls_listener::TlsListener;

mod tls_config;
use tls_config::tls_acceptor;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = ([127, 0, 0, 1], 3000).into();

    let new_svc = make_service_fn(|_| async {
        Ok::<_, Infallible>(service_fn(|_| async {
            Ok::<_, Infallible>(Response::new(Body::from("Hello, World!")))
        }))
    });

    // WARNING: invalid data in the request will cause the Server to shut down.
    // This could be handled by adding a filter to the stream to filter out
    // unwanted errors (and possibly log them), then use `hyper::server::accept::from_stream`,
    // or by doing something similar to the http-low-level.rs example.
    let incoming = TlsListener::new(tls_acceptor(), AddrIncoming::bind(&addr)?);

    let server = Server::builder(incoming).serve(new_svc);
    server.await?;
    Ok(())
}
