use crate::args::{Args, Commands};
use crate::stream::EncryptStream;
use opool::PoolAllocator;
use std::{net::SocketAddr, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};

const POOL_SIZE: usize = 1024 * 16;

struct ObjectPoolAllocator(usize);

impl PoolAllocator<Vec<u8>> for ObjectPoolAllocator {
    #[inline]
    fn allocate(&self) -> Vec<u8> {
        vec![0; self.0]
    }

    #[inline]
    fn reset(&self, _obj: &mut Vec<u8>) {}
}

pub async fn handle(args: Args) -> std::io::Result<()> {
    udpflow::set_timeout(std::time::Duration::from_secs(args.timeout));
    let pool = opool::Pool::new(POOL_SIZE, ObjectPoolAllocator(args.mtu)).to_rc();
    let bind_sock = udpflow::UdpListener::new(args.listen).unwrap();

    let local_bind_ip: SocketAddr = match args.remote {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
    };
    let args = Arc::new(args);
    loop {
        let (peer_sock, peer_addr) = bind_sock.accept(pool.get().as_mut()).await?;
        tracing::info!("new peer: {peer_addr}");
        tokio::spawn(handle_new_peer(
            peer_addr,
            local_bind_ip,
            args.clone(),
            peer_sock,
            pool.clone(),
        ));
    }
}

async fn handle_new_peer(
    peer_addr: SocketAddr,
    local_bind_ip: SocketAddr,
    args: Arc<Args>,
    peer_sock: udpflow::UdpStreamLocal,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) {
    let remote_sock = match udpflow::UdpStreamRemote::new(local_bind_ip, args.remote).await {
        Ok(sock) => sock,
        Err(err) => {
            tracing::error!("error creating port: {err}");
            return;
        }
    };

    match args.command {
        Commands::Client => {
            handle_peer_connection(peer_addr, peer_sock, remote_sock, args, pool.clone()).await;
        }
        Commands::Server => {
            handle_peer_connection(peer_addr, remote_sock, peer_sock, args, pool.clone()).await;
        }
    }
}

async fn handle_peer_connection<T, U>(
    peer_addr: SocketAddr,
    peer_sock: T,
    remote_sock: U,
    args: Arc<Args>,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    if let Some(encryption) = &args.encryption {
        let peer_sock =
            EncryptStream::new(peer_sock, encryption.clone(), crate::stream::Mode::Encrypt);
        let remote_sock = EncryptStream::new(
            remote_sock,
            encryption.clone(),
            crate::stream::Mode::Decrypt,
        );
        relay(peer_addr, peer_sock, remote_sock, args, pool).await;
        return;
    }
    relay(peer_addr, peer_sock, remote_sock, args, pool).await;
}
async fn relay<T, U>(
    peer_addr: SocketAddr,
    mut peer_sock: T,
    mut remote_sock: U,
    args: Arc<Args>,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    let duration = match args.deadline {
        Some(deadline) => std::time::Duration::from_secs(deadline),
        None => std::time::Duration::from_secs(84600 * 365),
    };

    if let Err(err) = tokio::time::timeout(
        duration,
        crate::copy_bidirectional::copy_bidirectional(
            &mut peer_sock,
            &mut remote_sock,
            pool.get().as_mut(),
            pool.get().as_mut(),
        ),
    )
    .await
    {
        tracing::error!("peer {peer_addr} connection failed: {err}");
    }
    tracing::info!("peer {peer_addr} disconnected");
}
