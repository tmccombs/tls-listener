use tokio_rustls::rustls::{Certificate, NoClientAuth, PrivateKey, ServerConfig};

const CERT: &[u8] = include_bytes!("local.cert");
const PKEY: &[u8] = include_bytes!("local.key");

pub fn tls_config() -> ServerConfig {
    let key = PrivateKey(PKEY.into());
    let cert = Certificate(CERT.into());
    let mut config = ServerConfig::new(NoClientAuth::new());
    config.set_single_cert(vec![cert], key).unwrap();
    config
}
