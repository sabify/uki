use crate::args::{Args, Commands};
use crate::stream::EncryptStream;
use opool::PoolAllocator;
use std::{net::SocketAddr, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};

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

    tracing::info!("listening on {}", args.listen);

    match args.protocol.as_str() {
        "uot" => uot_listen(pool, args).await,
        "tcp" => tcp_listen(pool, args).await,
        "udp" => udp_listen(pool, args).await,
        other => panic!("{} protocol is not supported.", other),
    }
}

async fn udp_listen(
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
    args: Args,
) -> std::io::Result<()> {
    let bind_sock = udpflow::UdpListener::new(args.listen)?;

    let local_bind_ip: SocketAddr = match args.remote {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
    };
    let args = Arc::new(args);
    loop {
        let (peer_sock, peer_addr) = bind_sock.accept(pool.get().as_mut()).await?;
        tokio::spawn(new_peer(
            peer_addr,
            local_bind_ip,
            args.clone(),
            peer_sock,
            pool.clone(),
        ));
    }
}

async fn tcp_listen(
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
    args: Args,
) -> std::io::Result<()> {
    let bind_sock = TcpListener::bind(args.listen).await?;

    let local_bind_ip: SocketAddr = match args.remote {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
    };
    let args = Arc::new(args);
    loop {
        let (peer_sock, peer_addr) = bind_sock.accept().await?;
        tokio::spawn(new_peer(
            peer_addr,
            local_bind_ip,
            args.clone(),
            peer_sock,
            pool.clone(),
        ));
    }
}

async fn uot_listen(
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
    args: Args,
) -> std::io::Result<()> {
    let local_bind_ip: SocketAddr = match args.remote {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
    };

    let args = Arc::new(args);

    match args.command {
        Commands::Client => {
            let bind_sock = udpflow::UdpListener::new(args.listen)?;
            loop {
                let (peer_sock, peer_addr) = bind_sock.accept(pool.get().as_mut()).await?;
                tokio::spawn(new_peer(
                    peer_addr,
                    local_bind_ip,
                    args.clone(),
                    peer_sock,
                    pool.clone(),
                ));
            }
        }
        Commands::Server => {
            let bind_sock = TcpListener::bind(args.listen).await?;
            loop {
                let (peer_sock, peer_addr) = bind_sock.accept().await?;
                let peer_sock = udpflow::UotStream::new(peer_sock);
                tokio::spawn(new_peer(
                    peer_addr,
                    local_bind_ip,
                    args.clone(),
                    peer_sock,
                    pool.clone(),
                ));
            }
        }
    };
}

async fn new_peer<T>(
    peer_addr: SocketAddr,
    local_bind_ip: SocketAddr,
    args: Arc<Args>,
    peer_sock: T,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
{
    tracing::info!("new peer: {peer_addr}");
    let result = match args.protocol.as_str() {
        "uot" => new_peer_uot(peer_addr, local_bind_ip, args, peer_sock, pool).await,
        "tcp" => new_peer_tcp(peer_addr, args, peer_sock, pool).await,
        "udp" => new_peer_udp(peer_addr, local_bind_ip, args, peer_sock, pool).await,
        _ => unreachable!(),
    };
    if let Err(err) = result {
        tracing::error!("error creating port: {err}");
    };
}

async fn new_peer_udp<T>(
    peer_addr: SocketAddr,
    local_bind_ip: SocketAddr,
    args: Arc<Args>,
    peer_sock: T,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let remote_sock = udpflow::UdpStreamRemote::new(local_bind_ip, args.remote).await?;
    handle_client_server(peer_addr, args, peer_sock, remote_sock, pool).await;
    Ok(())
}

async fn new_peer_tcp<T>(
    peer_addr: SocketAddr,
    args: Arc<Args>,
    peer_sock: T,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let remote_sock = TcpStream::connect(args.remote).await?;
    handle_client_server(peer_addr, args, peer_sock, remote_sock, pool).await;
    Ok(())
}

async fn new_peer_uot<T>(
    peer_addr: SocketAddr,
    local_bind_ip: SocketAddr,
    args: Arc<Args>,
    peer_sock: T,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    match args.command {
        Commands::Client => {
            let remote_sock = TcpStream::connect(args.remote).await?;
            let remote_sock = udpflow::UotStream::new(remote_sock);
            handle_client_server(peer_addr, args, peer_sock, remote_sock, pool).await;
        }
        Commands::Server => {
            let remote_sock = udpflow::UdpStreamRemote::new(local_bind_ip, args.remote).await?;
            handle_client_server(peer_addr, args, peer_sock, remote_sock, pool).await;
        }
    }
    Ok(())
}

async fn handle_client_server<T, U>(
    peer_addr: SocketAddr,
    args: Arc<Args>,
    peer_sock: T,
    remote_sock: U,
    pool: Arc<opool::Pool<ObjectPoolAllocator, Vec<u8>>>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    match args.command {
        Commands::Client => {
            encrypt(peer_addr, peer_sock, remote_sock, args, pool.clone()).await;
        }
        Commands::Server => {
            encrypt(peer_addr, remote_sock, peer_sock, args, pool.clone()).await;
        }
    }
}

async fn encrypt<T, U>(
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
