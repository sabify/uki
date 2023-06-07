# Uki

Uki (pronounced `ookee`) is a fast, simple and cross-platform packet forwarder and encryptor. It allows you to forward UDP and TCP packets between two or more hosts, and encrypts the packets to protect your data from prying eyes (you read "firewalls"!). Uki is easy to use, and can be configured with a simple command line interface.

Here are some of the features of Uki:

- Fast: Uki is designed to be fast.
- Simple to use: Uki is easy to use, even for users with no experience with packet forwarding.
- Secure: Uki can encrypt all packets to protect your data from prying eyes.
- Cross-Platform: Uki can be run anywhere that Rust compiler supports.

# Usage

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

# Supported Protocols

- UDP
- TCP
- UDP over TCP

# Supported Encryptions

- Xor

# Installation

Install the Uki by running `cargo install uki` or use the latest prebuild binaries from [Releases](https://github.com/sabify/uki/releases/latest).

# Usage

Please consult `uki --help`

# TODO

- More encryption methods
- More transports/protocols

Contributions are so welcome.

# License

Licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
