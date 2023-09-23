use hyper::server::conn::{AddrIncoming, Http};
use hyper::service::service_fn;
use hyper::{Body, Request, Response};
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

mod tls_config;
use tls_config::{tls_acceptor, tls_acceptor2, Acceptor};
use tokio::sync::mpsc;

/// To view the current certificate try:
/// `echo "Q" |openssl s_client -showcerts -connect 127.0.0.1:3000 | grep subject=CN`
///
/// To change the certificate make a HTTP request:
/// `curl https://127.0.0.1:3000 -k`
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let addr = ([127, 0, 0, 1], 3000).into();
    let counter = Arc::new(AtomicU64::new(0));

    let mut listener = tls_listener::builder(tls_acceptor())
        .max_handshakes(10)
        .listen(AddrIncoming::bind(&addr).unwrap());

    let (tx, mut rx) = mpsc::channel::<Acceptor>(1);

    let http = Http::new();
    loop {
        tokio::select! {
            conn = listener.accept() => {
                match conn.expect("Tls listener stream should be infinite") {
                    Ok((conn, remote_addr)) => {
                        let http = http.clone();
                        let tx = tx.clone();
                        let counter = counter.clone();
                        tokio::spawn(async move {
                            let svc = service_fn(move |request| handle_request(tx.clone(), counter.clone(), request));
                            if let Err(err) = http.serve_connection(conn, svc).await {
                                eprintln!("Application error (client address: {remote_addr}): {}", err);
                            }
                        });
                    },
                    Err(e) => {
                        if let Some(remote_addr) = e.remote_addr() {
                            eprint!("[client {remote_addr}] ");
                        }

                        eprintln!("Bad connection: {}", e);
                    }
                }
            },
            message = rx.recv() => {
                // Certificate is loaded on another task; we don't want to block the listener loop
                let acceptor = message.expect("Channel should not be closed");
                println!("Rotating certificate...");
                listener.replace_acceptor(acceptor);
            }
        }
    }
}

async fn handle_request(
    change_certificate: mpsc::Sender<Acceptor>,
    counter: Arc<AtomicU64>,
    _request: Request<Body>,
) -> Result<Response<Body>, Infallible> {
    let counter = counter.fetch_add(1, Ordering::Relaxed) + 1;
    let new_cert = if counter % 2 == 0 {
        tls_acceptor()
    } else {
        tls_acceptor2()
    };
    change_certificate.send(new_cert).await.ok();
    Ok(Response::new(Body::from("Changing certificate...")))
}
