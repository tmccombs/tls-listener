use futures_util::StreamExt;
use std::net::SocketAddr;
use tls_listener::{SpawningHandshakes, TlsListener};
use tokio::io::{copy, split};
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::ctrl_c;
#[cfg(all(feature = "native-tls", not(feature = "rustls")))]
use tokio_native_tls::TlsStream;
#[cfg(feature = "rustls")]
use tokio_rustls::server::TlsStream;

mod tls_config;
use tls_config::tls_acceptor;

#[inline]
async fn handle_stream(stream: TlsStream<TcpStream>, _remote_addr: SocketAddr) {
    let (mut reader, mut writer) = split(stream);
    match copy(&mut reader, &mut writer).await {
        Ok(cnt) => eprintln!("Processed {} bytes", cnt),
        Err(err) => eprintln!("Error: {}", err),
    };
}

/// For example try opening and closing a connection with:
/// `echo "Q" | openssl s_client -connect host:port`
#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = ([127, 0, 0, 1], 3000).into();

    let listener = TcpListener::bind(&addr).await?;

    TlsListener::new(SpawningHandshakes(tls_acceptor()), listener)
        .take_until(ctrl_c())
        .for_each_concurrent(None, |s| async {
            match s {
                Ok((stream, remote_addr)) => {
                    handle_stream(stream, remote_addr).await;
                }
                Err(e) => {
                    eprintln!("Error: {:?}", e);
                }
            }
        })
        .await;
    Ok(())
}
