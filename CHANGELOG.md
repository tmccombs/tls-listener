# Changelog

All notable changes to this project will be documented in this file.

## [0.9.1] - 2023-12-23

### Miscellaneous Tasks

- Update tokio-rustls


## [0.9.0] - 2023-12-05

### Features

- [**breaking**] Remove until & remove option from accept
  * BREAKING CHANGE: remove `until` from AsyncAccept trait. Use
  `StreamExt.take_until` on the TlsListener instead.
  * BREAKING CHANGE: `accept` fn on AsyncAccept trait no longer returns an
  Option
  * BREAKING CHANGE: `accept` fn on TlsListener no longer returns an Option


### Upgrade

- [**breaking**] Update to hyper 1.0
  * BREAKING CHANGE: Removed hyper-h1 and hyper-h2 features


## [0.8.0] - 2023-10-19

This is a backwards incompatible release. The main change is that accepting a new connection now returns a tuple of the new connection, and the peer
address. The `AsyncAccept` trait was also changed similarly. The `Error` enum was also changed to provide more details about the error. And if
the handshake times out, it now returns an error instead of silently waiting for the next connection.

### Features

- [**breaking**] Add a new error type for handshake timeouts
    * BREAKING CHANGE: Adds a new variant to the Error Enum
    * BREAKING CHANGE: The Error enum is now non_exhaustive
    * BREAKING CHANGE: Now returns an error if a handshake times out

- [**breaking**] Yield remote address upon accepting a connection, and include it in errors.
    * BREAKING CHANGE: The enum variant `Error::ListenerError` is now struct-like instead of tuple-like, and is `non_exhaustive` like the enum itself.
    * BREAKING CHANGE: `Error` now has three type parameters, not two.
    * BREAKING CHANGE: `TlsListener::accept` and `<TlsListener as Stream>::next` yields a tuple of (connection, remote address), not just the connection.
    * BREAKING CHANGE: `AsyncAccept` now has an associated type `Address`, which `poll_accept` must now return along with the accepted connection.

- [**breaking**] More changes for including peer address in response
    * BREAKING CHANGE: AsyncAccept::Error must implement std::error::Error
    * BREAKING CHANGE: TlsAcceptError is now a struct form variant.

## 0.7.0 - 2023-03-31

### Changed
- Increase tokio-rustls version to 0.24.0

## 0.6.0 - 2022-12-30

### Added
- Added additional tests and examples
- Re-export tls engine crates as public modules.

### Changed
- Increased default handshake timeout to 10 seconds (technically a breaking change)

## 0.5.1 - 2022-03-21

### Added

- Support for [`openssl`](https://github.com/sfackler/rust-openssl)

### Fixed

- Fixed compilation on non-unix environments, where tokio-net doesn't include unix sockets
- `SpawningHandshakes` will abort the tasks for pending connections when the linked futures are dropped. This should allow timeouts to cause the connectionto be closed.

## 0.5.0 - 2022-03-20

### Added

- Added [`AsyncAccept::until`] method, that creates a new `AsyncAccept` that will stop accepting connections after another future finishes.
- Added `hyper` submodule to add additional support for hyper. Specifically, a newtype for the hyper `Accept` trait for `AsyncAccept`.
- Added `SpawningHandshakes` struct behind the `rt` feature flag. This allows you to perform multiple handshakes in parallel with a multi-threaded runtime.

### Changed
- **Backwards incompatible**: `AsyncAccept::poll_accept` now returns, `Poll<Option<Result<...>>>` instead of `Poll<Result<...>>`. This allows the incoming stream of connections to stop, for example, if a graceful shutdown has been initiated. `impl`s provided by this crate have been updated, but custom implementations of `AsyncAccept`, or direct usage of the trait may break.
- Removed unnecessary type bounds (see #14). Potentially a breaking change, although I'd be suprised if any real code was affected.


## 0.4.3 - 2022-03-20

- Added `TlsListener::replace_accept_pin()` function to allow replacing the listener certificate at runtime, when the listener is pinned.

## 0.4.2 - 2022-03-09

### Added

- Added `TlsListener::replace_acceptor()` function to allow replacing the listener certificate at runtime.

## 0.4.1 - 2022-03-09

### Changed

- The implementation of `AsyncTls` for `tokio_native_tls::TlsAcceptor` now requires the connection type to implement `Send`. This in turn allows `TlsListener` to be `Send` when using the `native-tls` feature. Technically, this is a breaking change. However, in practice it is unlikely to break existing code and makes using `TlsListener` much easier to use when `native-tls` is enabled.

## 0.4.0 - 2022-02-22

NOTE: This release contains several breaking changes.

### Added

- Support for [`native-tls`](https://github.com/sfackler/rust-native-tls).

### Changed

- The TLS backend is now configurable. Both rustls and native-tls are supported. Other backends can also be used by implementing the `AsyncTls` trait.
  - You must now supply either the `rustls` or `native-tls` features to get support for a tls backend.
  - Unfortunately, the machinery for this required adding an additional type parameter to `TlsListener`.
- The `TlsListener` stream now returns a `tls_listener::Error` instead of `std::io::Error` type.
- Signatures of `TcpListener::new()` and `builder()` have changed to now take an argument of the TLS type rather than a `rustls::ServerConfig`,
  to update existing calls, replace `builder(config)` with `builder(Arc::new(config).into())`.

### Fixed

- Crate will now compile when linked against a target that doesn't explicitly enable the `tokio/time` and `hyper/tcp`
  features.
