use futures_util::StreamExt;
use std::net::SocketAddr;
use tls_listener::TlsListener;
use tokio::io::{copy, split};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::server::TlsStream;

mod tls_config;
use tls_config::tls_config;

#[inline]
async fn handle_stream(stream: TlsStream<TcpStream>) {
    let (mut reader, mut writer) = split(stream);
    match copy(&mut reader, &mut writer).await {
        Ok(cnt) => eprintln!("Processed {} bytes", cnt),
        Err(err) => eprintln!("Error: {}", err),
    };
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = ([127, 0, 0, 1], 3000).into();

    let listener = TcpListener::bind(&addr).await?;

    TlsListener::new(tls_config(), listener)
        .for_each_concurrent(None, |s| async {
            match s {
                Ok(stream) => {
                    handle_stream(stream).await;
                }
                Err(e) => {
                    eprintln!("Error: {:?}", e);
                }
            }
        })
        .await;
    Ok(())
}
