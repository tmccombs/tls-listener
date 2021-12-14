use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};

const CERT: &[u8] = include_bytes!("local.cert");
const PKEY: &[u8] = include_bytes!("local.key");

pub fn tls_config() -> ServerConfig {
    let key = PrivateKey(PKEY.into());
    let cert = Certificate(CERT.into());

    ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap()
}
