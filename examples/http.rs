use hyper::rt::{self, Future};
use hyper::server::conn::AddrIncoming;
use hyper::service::service_fn_ok;
use hyper::{Body, Request, Response, Server};
use native_tls::Identity;
use tls_listener::Builder as TlsListenerBuilder;

fn hello_world(_req: Request<Body>) -> Response<Body> {
    Response::new(Body::from("Hello, World!"))
}

const CERT: &'static [u8] = include_bytes!("certificate.p12");

fn main() -> Result<(), Box<std::error::Error>> {
    let addr = ([127, 0, 0, 1], 3000).into();

    let new_svc = || service_fn_ok(hello_world);

    let identity = Identity::from_pkcs12(CERT, "")?;
    let incoming = TlsListenerBuilder::new(identity).build(AddrIncoming::bind(&addr)?)?;

    let server = Server::builder(incoming)
        .serve(new_svc)
        .map_err(|e| eprintln!("Server error: {}", e));

    rt::run(server);

    Ok(())
}
