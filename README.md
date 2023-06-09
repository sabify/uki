# Uki

Uki (pronounced `ookee`) is a fast, simple and cross-platform packet forwarder and encryptor. It allows you to forward UDP and TCP packets between two or more hosts, and encrypts the packets to protect your data from prying eyes (you read "firewalls"!). Uki is easy to use, and can be configured with a simple command line interface.

Here are some of the features of Uki:

- Fast: Uki is designed to be fast.
- Simple to use: Uki is easy to use, even for users with no experience with packet forwarding.
- Secure: Uki can encrypt all packets to protect your data from prying eyes.
- Cross-Platform: Uki can be run anywhere that Rust compiler supports.

## Usage

IPs and ports are just for illustration. Both IPv4 and IPv6 are supported.
For globally listen on IPv4, use `0.0.0.0`, and for IPv6, use `[::]`.

Here's an example configuration:

```text
Client Traffic <==> Uki Client <==> <Uki Traffic> <==> Uki Server <==> Remote Traffic
                      |                                     |
                      |                                     |
            listen: 127.0.0.1:1111                listen: 127.0.0.1:2222
            remote: 127.0.0.1:2222                remote: 127.0.0.1:3333
```

For Uki Client you run:

```sh
uki --listen 127.0.0.1:1111 --remote 127.0.0.1:2222 --protocol udp client
```

And for Uki Server you run:

```sh
uki --listen 127.0.0.1:2222 --remote 127.0.0.1:3333 --protocol udp server
```

Please consult `uki --help` for more options.

### Supported Protocols

- UDP
- TCP
- UDP over TCP

### Supported Encryptions

- Xor

## Installation

Install the Uki by running `cargo install uki` or use the latest prebuild binaries from [Releases](https://github.com/sabify/uki/releases/latest).

## TODO

- More encryption methods
- More transports/protocols

Contributions are so welcome.

## Command Line

```text
Usage: uki [OPTIONS] --listen <LISTEN> --remote <REMOTE> --protocol <PROTOCOL> <COMMAND>

Commands:
  client
  server
  help    Print this message or the help of the given subcommand(s)

Options:
  -l, --listen <LISTEN>
          Listen address. e.g. '0.0.0.0:8080' or '[::]:8080' for dual stack listen
  -r, --remote <REMOTE>
          Remote address. Both IPv4 and IPv6 is supported
      --protocol <PROTOCOL>
          Protocol of choice. (uot: udp over tcp) [possible values: udp, tcp, uot]
      --deadline <DEADLINE>
          Enable deadline on open connections. An open connection will be forcibly closed after provided seconds
      --timeout <TIMEOUT>
          Connections that fail or are idle for `timeout` seconds will be closed. (udp related protocols only) [default: 20]
      --encryption <ENCRYPTION>
          Enable encryption. Usage format: '<method>:<arg>', e.g. 'xor:mysecurekey'. This should be enabled on both server and client. Currently only XOR is supported
      --custom-handshake <CUSTOM_HANDSHAKE>
          Enable sending custom handshake data. Format: '<request-file-path>,<response-file-path>'. When enabled, it should be enabled on both server and client with the same request and response file
      --daemonize
          Run the app as a daemon
      --log-level <LOG_LEVEL>
          Log level. Possible values from most to least priority: trace, debug, info, warn, error [default: ERROR]
      --log-path <LOG_PATH>
          Path of the log file
      --mtu <MTU>
          Maximum datagram size [default: 4096]
  -h, --help
          Print help
  -V, --version
          Print version
```

## License

Licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
