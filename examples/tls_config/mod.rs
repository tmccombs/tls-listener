#[cfg(feature = "rustls")]
mod cert {
    pub const CERT: &[u8] = include_bytes!("local.cert");
    pub const PKEY: &[u8] = include_bytes!("local.key");
}
#[cfg(feature = "native-tls")]
const PFX: &[u8] = include_bytes!("local.pfx");

#[cfg(feature = "rustls")]
pub fn tls_acceptor() -> tokio_rustls::TlsAcceptor {
    use std::sync::Arc;
    use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};

    let key = PrivateKey(cert::PKEY.into());
    let cert = Certificate(cert::CERT.into());

    Arc::new(
        ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap(),
    )
    .into()
}

#[cfg(all(feature = "native-tls", not(feature = "rustls")))]
pub fn tls_acceptor() -> tokio_native_tls::TlsAcceptor {
    use tokio_native_tls::native_tls::{Identity, TlsAcceptor};

    let identity = Identity::from_pkcs12(PFX, "").unwrap();
    TlsAcceptor::builder(identity).build().unwrap().into()
}
