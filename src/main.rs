mod args;
mod cipher;
mod copy;
mod copy_bidirectional;
mod stream;

use std::{net::SocketAddr, sync::Arc};

use cfg_if::cfg_if;
use cipher::Encryption;
use clap::Parser;
use daemonize::Daemonize;
use opool::PoolAllocator;
use tokio::io::{AsyncRead, AsyncWrite};

const POOL_SIZE: usize = 1024 * 16;

cfg_if! {
    if #[cfg(all(feature = "alloc-jem", not(target_env = "msvc")))] {
        use jemallocator::Jemalloc;
        #[global_allocator]
        static GLOBAL: Jemalloc = Jemalloc;
    }
}

struct ObjectPoolAllocator(usize);

impl PoolAllocator<Vec<u8>> for ObjectPoolAllocator {
    #[inline]
    fn allocate(&self) -> Vec<u8> {
        vec![0; self.0]
    }

    #[inline]
    fn reset(&self, _obj: &mut Vec<u8>) {}
}

fn main() {
    let cli = args::Cli::parse();

    if let Some(ref log_path) = cli.log_path {
        let log_file = std::fs::File::create(log_path)
            .unwrap_or_else(|_| panic!("could not create log file {}", log_path.to_string_lossy()));

        tracing_subscriber::fmt()
            .with_max_level(cli.log_level)
            .with_writer(log_file)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(cli.log_level)
            .init();
    }

    if cli.daemonize {
        let daemonize = Daemonize::new().working_directory("/tmp");

        match daemonize.start() {
            Ok(_) => {}
            Err(e) => panic!("daemonize failed: {}", e),
        }
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(main_async(cli))
        .unwrap();
}

async fn main_async(cli: args::Cli) -> std::io::Result<()> {
    let pool = opool::Pool::new(POOL_SIZE, ObjectPoolAllocator(cli.mtu)).to_rc();
    let bind_sock = udpflow::UdpListener::new(cli.listen).unwrap();

    let local_bind_ip: SocketAddr = match cli.remote {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
    };
    let encryption = cli.encryption.clone().map(Arc::new);
    let cli = Arc::new(cli);
    loop {
        let (peer_sock, peer_addr) = bind_sock.accept(pool.get().as_mut()).await?;
        tracing::info!("new peer: {peer_addr}");
        tokio::spawn(handle_new_peer(
            peer_addr,
            local_bind_ip,
            cli.clone(),
            encryption.clone(),
            peer_sock,
            pool.clone(),
        ));
    }
}

async fn handle_new_peer(
    peer_addr: SocketAddr,
    local_bind_ip: SocketAddr,
    cli: Arc<args::Cli>,
    encryption: Option<Arc<Encryption>>,
    peer_sock: udpflow::UdpStreamLocal,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) {
    let remote_sock = match udpflow::UdpStreamRemote::new(local_bind_ip, cli.remote).await {
        Ok(sock) => sock,
        Err(err) => {
            tracing::error!("error creating port: {err}");
            return;
        }
    };

    match cli.command {
        args::Commands::Client => {
            handle_peer_connection(peer_addr, peer_sock, remote_sock, encryption, pool.clone())
                .await;
        }
        args::Commands::Server => {
            handle_peer_connection(peer_addr, remote_sock, peer_sock, encryption, pool.clone())
                .await;
        }
    }
}

async fn handle_peer_connection<T, U>(
    peer_addr: SocketAddr,
    peer_sock: T,
    remote_sock: U,
    encryption: Option<Arc<Encryption>>,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    if let Some(encryption) = &encryption {
        let peer_sock =
            stream::EncryptStream::new(peer_sock, encryption.clone(), stream::Mode::Encrypt);
        let remote_sock =
            stream::EncryptStream::new(remote_sock, encryption.clone(), stream::Mode::Decrypt);
        relay(peer_addr, peer_sock, remote_sock, pool).await;
        return;
    }
    relay(peer_addr, peer_sock, remote_sock, pool).await;
}
async fn relay<T, U>(
    peer_addr: SocketAddr,
    mut peer_sock: T,
    mut remote_sock: U,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    if let Err(err) = crate::copy_bidirectional::copy_bidirectional(
        &mut peer_sock,
        &mut remote_sock,
        pool.get().as_mut(),
        pool.get().as_mut(),
    )
    .await
    {
        tracing::error!("peer {peer_addr} connection failed: {err}");
    }
    tracing::info!("peer {peer_addr} disconnected");
}
