#[cfg(feature = "rustls")]
mod config {
    use std::sync::Arc;
    use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};

    const CERT: &[u8] = include_bytes!("local.cert");
    const PKEY: &[u8] = include_bytes!("local.key");
    #[allow(dead_code)]
    const CERT2: &[u8] = include_bytes!("local2.cert");
    #[allow(dead_code)]
    const PKEY2: &[u8] = include_bytes!("local2.key");

    pub type Acceptor = tokio_rustls::TlsAcceptor;

    fn tls_acceptor_impl(cert_der: &[u8], key_der: &[u8]) -> Acceptor {
        let key = PrivateKey(cert_der.into());
        let cert = Certificate(key_der.into());
        Arc::new(
            ServerConfig::builder()
                .with_safe_defaults()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .unwrap(),
        )
        .into()
    }

    pub fn tls_acceptor() -> Acceptor {
        tls_acceptor_impl(PKEY, CERT)
    }

    #[allow(dead_code)]
    pub fn tls_acceptor2() -> Acceptor {
        tls_acceptor_impl(PKEY2, CERT2)
    }
}

#[cfg(all(
    feature = "native-tls",
    not(any(feature = "rustls", feature = "openssl"))
))]
mod config {
    use tokio_native_tls::native_tls::{Identity, TlsAcceptor};

    const PFX: &[u8] = include_bytes!("local.pfx");
    const PFX2: &[u8] = include_bytes!("local2.pfx");

    pub type Acceptor = tokio_native_tls::TlsAcceptor;

    fn tls_acceptor_impl(pfx: &[u8]) -> Acceptor {
        let identity = Identity::from_pkcs12(pfx, "").unwrap();
        TlsAcceptor::builder(identity).build().unwrap().into()
    }

    pub fn tls_acceptor() -> Acceptor {
        tls_acceptor_impl(PFX)
    }

    pub fn tls_acceptor2() -> Acceptor {
        tls_acceptor_impl(PFX2)
    }
}

#[cfg(all(
    feature = "openssl",
    not(any(feature = "rustls", feature = "native-tls"))
))]
mod config {
    use openssl_impl::ssl::{SslContext, SslFiletype, SslMethod};
    use std::path::Path;

    pub type Acceptor = openssl_impl::ssl::SslContext;

    fn tls_acceptor_impl<P: AsRef<Path>>(cert_file: P, key_file: P) -> Acceptor {
        let mut builder = SslContext::builder(SslMethod::tls_server()).unwrap();
        builder
            .set_certificate_file(cert_file, SslFiletype::ASN1)
            .unwrap();
        builder
            .set_private_key_file(key_file, SslFiletype::ASN1)
            .unwrap();
        builder.build()
    }

    pub fn tls_acceptor() -> Acceptor {
        tls_acceptor_impl(
            "./examples/tls_config/local.cert",
            "./examples/tls_config/local.key",
        )
    }

    pub fn tls_acceptor2() -> Acceptor {
        tls_acceptor_impl(
            "./examples/tls_config/local2.cert",
            "./examples/tls_config/local2.key",
        )
    }
}

pub use config::*;
