---
title: std/net
description: TCP and UDP sockets, TLS clients and servers.
---

TCP and UDP sockets, and TLS on both sides of the connection. Import with
`import "std/net";`. Native backend only.

Calls block by default. Failure is reported as `none`, `false`, or a negative
byte count, never as an exception. Sockets are plain OS handles: call `close`
when you are done with one.

The module defines five types: `Socket` (a TCP or UDP socket), `TlsSocket` (a
TLS connection over TCP), `TlsConfig` (a reusable TLS setup: role,
certificates, ALPN, verification policy), `Addr` (a resolved network address),
and `Datagram` (bytes received over UDP together with their sender).

## Connecting and listening

### `connect`

```ruby
Socket? connect(host: string, port: int)
```

Resolves `host`, creates a TCP socket, and connects it to `host:port`. Returns
the connected socket, or `none` if any step fails.

```ruby
let sock = net.connect("example.com", 80);
if (sock == none) { return 1; }
```

### `listen`

```ruby
Socket? listen(host: string, port: int, backlog: int)
```

Creates a TCP server socket bound to `host:port` (with address reuse enabled)
and starts listening. `backlog` is the pending-connection queue length.
Returns `none` on failure. Use `"0.0.0.0"` as the host to accept from any
interface.

### `bind_udp`

```ruby
Socket? bind_udp(host: string, port: int)
```

Creates a UDP socket bound to `host:port`, ready for `recv_from` and
`send_to`. Returns `none` on failure.

### `resolve`

```ruby
Addr? resolve(host: string, port: int)
```

Resolves `host:port` into an address suitable as a connect or send target.
`host` can be a name or a numeric IP. Returns `none` if resolution fails.

### `resolve_bind`

```ruby
Addr? resolve_bind(host: string, port: int)
```

Like `resolve`, but produces a wildcard-capable address suitable for binding a
server socket.

### `tcp_socket`, `udp_socket`

```ruby
Socket? tcp_socket(family: int)
Socket? udp_socket(family: int)
```

Create an unbound, unconnected socket. `family` is `net.IPV4` or `net.IPV6`.
These are the low-level building blocks. Most programs use `connect`,
`listen`, or `bind_udp` instead.

## Methods on `Socket`

### `accept`

```ruby
Socket? accept(self)
```

Waits for the next incoming connection on a listening socket and returns it,
or `none` on error. Close the returned socket separately.

### `send`

```ruby
int send(self, data: string)
```

Sends `data` on a connected socket. Returns the number of bytes actually sent,
which may be fewer than `data.len()`, or `-1` on error.

### `send_all`

```ruby
bool send_all(self, data: string)
```

Sends every byte of `data`, retrying partial sends. Returns `false` on error.
Prefer this over `send` unless you handle partial sends yourself.

### `recv`

```ruby
string? recv(self, max: int)
```

Receives up to `max` bytes. Returns `""` when the peer has closed the
connection, or `none` on error.

```ruby
let chunk = sock.recv(4096);
while (chunk != none && chunk!.len() > 0) {
    io.write(chunk!);
    chunk = sock.recv(4096);
}
```

### `send_to`

```ruby
int send_to(self, data: string, addr: Addr)
```

Sends `data` as a UDP datagram to `addr`. Returns bytes sent, or `-1` on
error.

### `recv_from`

```ruby
Datagram? recv_from(self, max: int)
```

Receives one UDP datagram of up to `max` bytes. Returns the data and the
sender's address, or `none` on error. A `Datagram` is a struct
`{ data: string, from: Addr }`.

### `local`, `peer`

```ruby
Addr? local(self)   # this socket's own address
Addr? peer(self)    # the connected peer's address
```

Return `none` on error, for example `peer` on an unconnected socket.

### Options and lifetime

```ruby
bool set_blocking(self, blocking: bool)  # non-blocking calls fail fast instead of waiting
bool set_timeout(self, ms: int)          # send/receive timeout in ms; 0 disables
bool set_reuse(self, on: bool)           # SO_REUSEADDR
bool set_keepalive(self, on: bool)       # SO_KEEPALIVE
bool set_broadcast(self, on: bool)       # SO_BROADCAST, needed to send UDP broadcasts
bool set_nodelay(self, on: bool)         # TCP_NODELAY, disable Nagle batching
bool set_option(self, option: int, value: int)  # generic form, see the constants below
bool shutdown(self, how: int)            # stop one direction, see the constants below
void close(self)                         # release the OS handle
```

All the setters return `true` on success. The module provides constants for
the numeric codes:

```ruby
let IPV4 = 4;           # address families, for tcp_socket and udp_socket
let IPV6 = 6;

let OPT_REUSE = 0;      # option codes, for set_option
let OPT_KEEPALIVE = 1;
let OPT_BROADCAST = 2;
let OPT_NODELAY = 3;

let SHUT_READ = 0;      # directions, for shutdown
let SHUT_WRITE = 1;
let SHUT_BOTH = 2;
```

```ruby
let sock = net.tcp_socket(net.IPV6)!;
sock.shutdown(net.SHUT_WRITE);
```

## TLS

The native runtime bundles TLS clients and servers (Mbed TLS 3.6). By default,
certificates are verified against the system trust store. This includes the Windows
certificate store, or the standard CA bundle locations on Linux and macOS.
Point the `KORA_CA_BUNDLE` environment variable at a PEM file to supply your
own roots. Set `KORA_TLS_DEBUG=1` to print handshake failures to stderr.

For the common client case, use `tls_connect` or `tls_client` below and skip
the rest. Everything else (servers, mutual TLS, ALPN, custom roots) goes
through a `TlsConfig`.

### `tls_connect`

```ruby
TlsSocket? tls_connect(host: string, port: int)
```

Resolves `host`, connects over TCP, and performs a TLS handshake, verifying
the server's certificate and hostname. Returns `none` if any step fails,
including verification.

```ruby
let sock = net.tls_connect("example.com", 443);
```

### `tls_client`

```ruby
TlsSocket? tls_client(sock: Socket, host: string)
```

Upgrades an already-connected TCP socket to TLS, verifying the certificate
against `host`. Use this when you need socket options (like timeouts) set
before the handshake. The returned `TlsSocket` owns the connection. Do not
read or write `sock` directly afterwards.

### `tls_client_insecure`

```ruby
TlsSocket? tls_client_insecure(sock: Socket, host: string)
```

Like `tls_client`, but skips certificate verification. The connection is
encrypted yet you have no proof of who is on the other end. Only for local
endpoints and self-signed setups.

### `tls_config`

```ruby
TlsConfig? tls_config()
```

A client configuration: verification on, system roots loaded. Adjust it with
the methods below, then upgrade connected sockets with `handshake`. One config
serves any number of connections and is safe to share across threads.

### `tls_server_config`

```ruby
TlsConfig? tls_server_config(cert_pem: string, key_pem: string)
```

A server configuration presenting the given certificate chain and private key,
both PEM text. Returns `none` if either fails to parse. By default it does not
request client certificates. See `verify` for mutual TLS.

### `tls_accept`

```ruby
TlsSocket? tls_accept(listener: Socket, config: TlsConfig)
```

Waits for the next TCP connection on a listening socket and performs the
server side of the handshake. Returns `none` on error, with the raw connection
closed.

### Methods on `TlsConfig`

```ruby
bool own_cert(self, cert_pem: string, key_pem: string)  # certificate to present;
                                                        # on a client config this is mutual TLS
bool ca(self, ca_pem: string)             # trust these roots instead of the system's
bool alpn(self, protos: [string])         # offer application protocols, e.g. ["h2", "http/1.1"]
bool verify(self, on: bool)               # require and check the peer's certificate
TlsSocket? handshake(self, sock: Socket, host: string)  # upgrade sock; "" as host on servers
```

Configure first, then hand out sessions: `alpn` can be set once, and a config
should not be modified while connections made from it are live. On a server
config, `verify(true)` plus `ca(...)` demands and checks client certificates,
which is mutual TLS.

### Methods on `TlsSocket`

```ruby
int     send(self, data: string)       # bytes written, or -1
bool    send_all(self, data: string)   # write every byte; false on error
string? recv(self, max: int)           # up to max bytes; "" at clean close, none on error
void    close(self)                    # send close-notify and close the socket

bool    verified(self)                 # the peer's certificate chain checked out
string  version(self)                  # negotiated protocol, e.g. "TLSv1.3"
string? alpn(self)                     # negotiated ALPN protocol, or none
string? peer_cert(self)                # the peer's certificate as DER bytes, or none
```

The I/O methods have the same shapes as their `Socket` counterparts, carried
over TLS.

## Methods on `Addr`

```ruby
string host(self)     # numeric IP, e.g. "93.184.216.34"
int    port(self)
int    family(self)   # 4 for IPv4, 6 for IPv6
```

## Example: HTTPS client

An HTTPS GET, start to finish. For plain HTTP, swap `net.tls_connect(host, 443)`
for `net.connect(host, 80)`. Everything else is identical.

```ruby
import "std/net";
import "std/io";

int main() {
    let opened = net.tls_connect("example.com", 443);
    if (opened == none) {
        io.print("could not connect");
        return 1;
    }
    let sock = opened!;

    let request = "GET / HTTP/1.0\r\nHost: example.com\r\nConnection: close\r\n\r\n";
    if (!sock.send_all(request)) {
        sock.close();
        return 1;
    }

    let chunk = sock.recv(4096);
    while (chunk != none && chunk!.len() > 0) {
        io.write(chunk!);
        chunk = sock.recv(4096);
    }
    sock.close();
    return 0;
}
```

## Example: TCP echo server

```ruby
import "std/net";
import "std/io";

int main() {
    let server = net.listen("0.0.0.0", 7777, 16);
    if (server == none) {
        io.print("could not listen on 7777");
        return 1;
    }
    while (true) {
        let conn = server!.accept();
        if (conn == none) { continue; }
        let c = conn!;
        let data = c.recv(1024);
        if (data != none) {
            c.send_all(data!);
        }
        c.close();
    }
    return 0;
}
```

## Example: TLS echo server

The same server behind TLS. The certificate and key are PEM files, for local
testing generated with
`openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout server.key -out server.crt -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost"`.

```ruby
import "std/net";
import "std/io";
import "std/fs";

int main() {
    let cert = fs.open("server.crt", "r")!.read_all();
    let key = fs.open("server.key", "r")!.read_all();
    let made = net.tls_server_config(cert, key);
    if (made == none) {
        io.print("bad certificate or key");
        return 1;
    }
    let config = made!;

    let server = net.listen("0.0.0.0", 8443, 16);
    if (server == none) {
        io.print("could not listen on 8443");
        return 1;
    }
    while (true) {
        let conn = net.tls_accept(server!, config);
        if (conn == none) { continue; }
        let c = conn!;
        let data = c.recv(1024);
        if (data != none) {
            c.send_all(data!);
        }
        c.close();
    }
    return 0;
}
```

A client connects with its own roots pointed at the server's certificate:

```ruby
let config = net.tls_config()!;
config.ca(cert_pem);                       # trust this server's cert
let sock = config.handshake(net.connect("127.0.0.1", 8443)!, "localhost");
```

## Example: UDP

```ruby
import "std/net";

int main() {
    # receiver
    let sock = net.bind_udp("127.0.0.1", 9999)!;
    let dgram = sock.recv_from(1024);
    if (dgram != none) {
        # reply to whoever sent it
        sock.send_to("pong", dgram!.from);
    }
    sock.close();
    return 0;
}
```
