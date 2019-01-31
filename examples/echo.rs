use futures::{Future, Stream};
use native_tls::Identity;
use tls_listener::Builder as TlsListenerBuilder;
use tokio::executor::spawn;
use tokio::io::{copy, AsyncRead};
use tokio::net::{TcpListener, TcpStream};
use tokio_tls::TlsStream;

const CERT: &[u8] = include_bytes!("certificate.p12");

#[inline]
fn handle_stream(stream: TlsStream<TcpStream>) -> impl Future<Item = (), Error = ()> {
    let (reader, writer) = stream.split();
    copy(reader, writer)
        .map(|(cnt, _, _)| {
            eprintln!("Processed {} bytes", cnt);
        })
        .map_err(|err| {
            eprintln!("Error: {}", err);
        })
}

fn main() -> Result<(), Box<std::error::Error>> {
    let addr = ([127, 0, 0, 1], 3000).into();
    let identity = Identity::from_pkcs12(CERT, "")?;

    let listener = TcpListener::bind(&addr)?;

    let server = TlsListenerBuilder::new(identity)
        .build(listener.incoming())?
        .map_err(|err| {
            eprintln!("Error: {}", err);
        })
        .for_each(|stream| spawn(handle_stream(stream)));

    tokio::run(server);
    Ok(())
}
