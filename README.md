# tls-listener

[![Apache 2 License](https://img.shields.io/badge/License-Apache--2.0-brightgreen)](https://www.apache.org/licenses/LICENSE-2.0)
[![Crate version](https://img.shields.io/crates/v/tls-listener)](https://crates.io/crates/tls-listener)
[![Docs](https://docs.rs/tls-listener/badge.svg)](https://docs.rs/tls-listener)
[![Build status](https://github.com/tmccombs/tls-listener/workflows/CI/badge.svg)](https://github.com/tmccombs/tls-listener/actions?query=workflow%3ACI)

This library is intended to automatically initiate a TLS connection
as for each new connection in a source of new streams (such as a listening
TCP or unix domain socket).

In particular, the `TlsListener` can be used as the `incoming` argument to `hyper::server::Server::builder`.

See examples for examples of usage.
