# tls-listener

[![Apache 2 License](https://img.shields.io/badge/License-Apache--2.0-brightgreen)](https://www.apache.org/licenses/LICENSE-2.0)
[![Crate version](https://img.shields.io/crates/v/tls-listener)](https://crates.io/crates/tls-listener)
[![Docs](https://docs.rs/tls-listener/badge.svg)](https://docs.rs/tls-listener)
[![Build status](https://github.com/tmccombs/tls-listener/workflows/CI/badge.svg)](https://github.com/tmccombs/tls-listener/actions?query=workflow%3ACI)

This library is intended to automatically initiate a TLS connection
as for each new connection in a source of new streams (such as a listening
TCP or unix domain socket).

It can be used to easily create a `Stream` of TLS connections from a listening socket.

See examples for examples of usage.

You must enable either one of the `rustls[-xyz]` (more details below), `native-tls`, or `openssl`
features depending on which implementation you would like to use.

When enabling the `rustls` feature, the `rustls` crate will be added as a dependency through
the `tokio-rustls` crate without any
[cryptography providers](https://docs.rs/rustls/latest/rustls/#cryptography-providers)
included by default. To include one, do either of the following:

1. Enable at least one of the additional `rustls-aws-lc`, `rustls-fips`, or `rustls-ring` features.
   By doing this, you can also remove the `rustls` feature flag since it will be enabled
   automatically by any of the `rustls-xyz` features.

   ```toml
   # Replace `rustls-xyz` with one of the features mentioned above.
   tls-listener = { version = "x", features = ["rustls-xyz"] }
   ```

   These features will enable their relevant [`rustls` features](https://docs.rs/rustls/latest/rustls/#crate-features).

1. Keep the `rustls` feature flag, but directly add the [`rustls`](https://crates.io/crates/rustls)
   and/or [`tokio-rustls`](https://crates.io/crates/tokio-rustls) crates to your project's 
   dependencies and enable your preferred flags on them instead of adding additional flags on
   this crate (`tls-listener`).

   ```toml
   # Replace `xyz` with one of the features mentioned in the crate's documentation.
   # for example: `aws-lc-rc`, `fips` or `ring`
   rustls = { version = "x", default-features = false, features = ["xyz"]}
   # And/or
   tokio-rustls = { version = "x", default-features = false, features = ["xyz"]}
   ```

   You can also enable the default features by removing `default-features = false`, which will
   enable the [AWS-LC crypto provider](https://github.com/aws/aws-lc-rs). However, their
   default features are not enable by `tls-listener` because doing so will make disabling
   them very hard for dependent crates.
