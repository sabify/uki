use crate::cipher::Cipher;
use clap::{Parser, Subcommand};
use std::sync::Arc;
use std::{net::SocketAddr, path::PathBuf};
use tracing::Level;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(required = true, long, short)]
    /// Listen address. e.g. '0.0.0.0:8080' or '[::]:8080' for dual stack listen.
    pub listen: SocketAddr,
    #[arg(required = true, long, short)]
    /// Remote address. Both IPv4 and IPv6 is supported.
    pub remote: SocketAddr,
    #[arg(long, value_parser = ["udp", "tcp", "uot"])]
    /// Protocol of choice. (uot: udp over tcp)
    pub protocol: String,
    #[arg(long)]
    /// Enable deadline on open connections. An open connection will be forcibly closed after provided seconds.
    pub deadline: Option<u64>,
    #[arg(long, default_value_t = 20)]
    /// Connections that fail or are idle for `timeout` seconds will be closed.
    /// (udp related protocols only)
    pub timeout: u64,
    #[arg(long, value_parser = parse_encryption)]
    /// Enable encryption. Usage format: '<method>:<arg>', e.g. 'xor:mysecurekey'.
    /// This should be enabled on both server and client.
    /// Currently only XOR is supported.
    pub encryption: Option<Arc<Cipher>>,
    #[arg(long, value_parser = parse_handshake)]
    /// Enable sending custom handshake data. Format: '<request-file-path>,<response-file-path>'.
    /// When enabled, it should be enabled on both server and client with the same request and response
    /// file.
    pub custom_handshake: Option<Arc<CustomHandshake>>,
    #[arg(long)]
    /// Run the app as a daemon.
    pub daemonize: bool,
    #[arg(long, default_value_t = Level::ERROR)]
    /// Log level. Possible values from most to least priority: trace, debug, info, warn, error.
    pub log_level: Level,
    #[arg(long)]
    /// Path of the log file.
    pub log_path: Option<PathBuf>,
    #[arg(long, default_value_t = 4096)]
    /// Maximum datagram size.
    pub mtu: usize,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Client,
    Server,
}

#[derive(Debug)]
pub struct CustomHandshake {
    pub request: Box<[u8]>,
    pub response: Box<[u8]>,
}

fn parse_encryption(value: &str) -> Result<Arc<Cipher>, String> {
    Cipher::try_from(value).map(Arc::new)
}

fn parse_handshake(value: &str) -> Result<Arc<CustomHandshake>, String> {
    let values: Vec<&str> = value.split(',').collect();
    if values.len() < 2 {
        return Err("you should provide both request and response file paths".into());
    }
    let request = std::fs::read(values[0])
        .map_err(|err| format!("{err}"))?
        .into_boxed_slice();
    let response = std::fs::read(values[1])
        .map_err(|err| format!("{err}"))?
        .into_boxed_slice();

    Ok(Arc::new(CustomHandshake { request, response }))
}
