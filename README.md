# tcp-listener


This library is intended to automatically initiate a TLS connection
as for each new connection in a source of new streams (such as a listening
TCP or unix domain socket).

In particular, the `TlsListener` can be used as the `incoming` argument to `hyper::server::Server::builder`.
