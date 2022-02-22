# Changelog 

## 0.4.0 - 2022-02-22

### Added

- Support for [`native-tls`](https://github.com/sfackler/rust-native-tls).

### Changed

- Either one of the `rustls` or `native-tls` features must now be enabled.
- The `TlsListener` stream now returns a `tls_listener::Error` instead of `std::io::Error` type.
- Signatures of `TcpListener::new()` and `builder()` have changed to now take an argument `T: Into<TlsAcceptor>`. 
  When passing a `rustls::ServerConfig` it should therefore be wrapped in an `Arc` first.

### Fixed

- Crate will now compile when linked against a target that doesn't explicitly enable the `tokio/time` and `hyper/tcp`
  features.
