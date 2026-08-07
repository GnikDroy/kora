---
title: std/net
description: TCP and UDP sockets.
---

TCP and UDP sockets. Import with `import "std/net";`. Native backend only.

Calls block by default. Failure is reported as `none`, `false`, or a negative
byte count, never as an exception. Sockets are plain OS handles: call `close`
when you are done with one.

The module defines three types: `Socket` (a TCP or UDP socket), `Addr` (a
resolved network address), and `Datagram` (bytes received over UDP together
with their sender).

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

Create an unbound, unconnected socket. `family` is `4` for IPv4 or `6` for
IPv6. These are the low-level building blocks. Most programs use `connect`,
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
bool set_option(self, option: int, value: int)  # generic form; 0 reuse, 1 keepalive,
                                                # 2 broadcast, 3 nodelay
bool shutdown(self, how: int)            # stop one direction: 0 read, 1 write, 2 both
void close(self)                         # release the OS handle
```

All the setters return `true` on success.

## Methods on `Addr`

```ruby
string host(self)     # numeric IP, e.g. "93.184.216.34"
int    port(self)
int    family(self)   # 4 for IPv4, 6 for IPv6
```

## Example: TCP client

An HTTP GET, start to finish:

```ruby
import "std/net";
import "std/io";

int main() {
    let opened = net.connect("example.com", 80);
    if (opened == none) {
        io.print("could not connect");
        return 1;
    }
    let sock = opened!;

    let request = "GET / HTTP/1.0\r\nHost: example.com\r\n\r\n";
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
