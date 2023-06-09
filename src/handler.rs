use crate::args::{Args, Commands};
use crate::encrypt_stream::EncryptStream;
use std::{net::SocketAddr, sync::Arc};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const POOL_SIZE: usize = 1024 * 16;

type PoolAllocator = opool::Pool<ObjectPoolAllocator, Vec<u8>>;
type PoolAllocatorObject = opool::RcGuard<ObjectPoolAllocator, Vec<u8>>;

struct ObjectPoolAllocator(usize);

impl opool::PoolAllocator<Vec<u8>> for ObjectPoolAllocator {
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
        _ => unreachable!(),
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
        let mut buf = pool.clone().get_rc();
        let (n, peer_sock, peer_addr) = bind_sock.accept(buf.as_mut()).await?;
        tokio::spawn(new_peer(
            peer_addr,
            local_bind_ip,
            args.clone(),
            peer_sock,
            pool.clone(),
            Some((buf, n)),
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
        peer_sock.set_nodelay(true)?;
        tokio::spawn(new_peer(
            peer_addr,
            local_bind_ip,
            args.clone(),
            peer_sock,
            pool.clone(),
            None,
        ));
    }
}

async fn uot_listen(pool: Arc<PoolAllocator>, args: Args) -> std::io::Result<()> {
    let local_bind_ip: SocketAddr = match args.remote {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
    };

    let args = Arc::new(args);

    match args.command {
        Commands::Client => {
            let bind_sock = udpflow::UdpListener::new(args.listen)?;
            loop {
                let mut buf = pool.clone().get_rc();
                let (n, peer_sock, peer_addr) = bind_sock.accept(buf.as_mut()).await?;
                tokio::spawn(new_peer(
                    peer_addr,
                    local_bind_ip,
                    args.clone(),
                    peer_sock,
                    pool.clone(),
                    Some((buf, n)),
                ));
            }
        }
        Commands::Server => {
            let bind_sock = TcpListener::bind(args.listen).await?;
            loop {
                let (peer_sock, peer_addr) = bind_sock.accept().await?;
                if let Err(err) = peer_sock.set_nodelay(true) {
                    tracing::error!("{peer_addr} tcp nodelay set failed: {err}");
                    continue;
                }
                tokio::spawn(new_peer(
                    peer_addr,
                    local_bind_ip,
                    args.clone(),
                    peer_sock,
                    pool.clone(),
                    None,
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
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
{
    tracing::info!("new peer: {peer_addr}");
    let result = match args.protocol.as_str() {
        "uot" => new_peer_uot(peer_addr, local_bind_ip, args, peer_sock, pool, buf).await,
        "tcp" => new_peer_tcp(peer_addr, args, peer_sock, pool, buf).await,
        "udp" => new_peer_udp(peer_addr, local_bind_ip, args, peer_sock, pool, buf).await,
        _ => unreachable!(),
    };
    if let Err(err) = result {
        tracing::error!("error creating remote socket: {err}");
    };
}

async fn new_peer_udp<T>(
    peer_addr: SocketAddr,
    local_bind_ip: SocketAddr,
    args: Arc<Args>,
    mut peer_sock: T,
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let mut remote_sock = udpflow::UdpStreamRemote::new(local_bind_ip, args.remote).await?;
    handshake(&mut peer_sock, &mut remote_sock, &args).await?;
    encrypt(peer_addr, args, peer_sock, remote_sock, pool, buf).await;
    Ok(())
}

async fn new_peer_tcp<T>(
    peer_addr: SocketAddr,
    args: Arc<Args>,
    mut peer_sock: T,
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let mut remote_sock = TcpStream::connect(args.remote).await?;
    remote_sock.set_nodelay(true)?;
    handshake(&mut peer_sock, &mut remote_sock, &args).await?;
    encrypt(peer_addr, args, peer_sock, remote_sock, pool, buf).await;
    Ok(())
}

async fn new_peer_uot<T>(
    peer_addr: SocketAddr,
    local_bind_ip: SocketAddr,
    args: Arc<Args>,
    mut peer_sock: T,
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    match args.command {
        Commands::Client => {
            let mut remote_sock = TcpStream::connect(args.remote).await?;
            remote_sock.set_nodelay(true)?;
            handshake(&mut peer_sock, &mut remote_sock, &args).await?;
            let remote_sock = udpflow::UotStream::new(remote_sock);
            encrypt(peer_addr, args, peer_sock, remote_sock, pool, buf).await;
        }
        Commands::Server => {
            let mut remote_sock = udpflow::UdpStreamRemote::new(local_bind_ip, args.remote).await?;
            handshake(&mut peer_sock, &mut remote_sock, &args).await?;
            let peer_sock = udpflow::UotStream::new(peer_sock);
            encrypt(peer_addr, args, peer_sock, remote_sock, pool, buf).await;
        }
    }
    Ok(())
}

async fn handshake<T, U>(peer_sock: &mut T, remote_sock: &mut U, args: &Args) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    if let Some(custom_handshake) = args.custom_handshake.clone() {
        match args.command {
            Commands::Client => {
                remote_sock.write_all(&custom_handshake.request).await?;
                remote_sock
                    .read_exact(vec![0u8; custom_handshake.response.len()].as_mut())
                    .await?;
            }
            Commands::Server => {
                peer_sock
                    .read_exact(vec![0u8; custom_handshake.request.len()].as_mut())
                    .await?;
                peer_sock.write_all(&custom_handshake.response).await?;
            }
        }
    };

    Ok(())
}

async fn encrypt<T, U>(
    peer_addr: SocketAddr,
    args: Arc<Args>,
    peer_sock: T,
    remote_sock: U,
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    if let Some(encryption) = &args.encryption {
        match args.command {
            Commands::Client => {
                let peer_sock = EncryptStream::new(
                    peer_sock,
                    encryption.clone(),
                    crate::encrypt_stream::Mode::Encrypt,
                );
                let remote_sock = EncryptStream::new(
                    remote_sock,
                    encryption.clone(),
                    crate::encrypt_stream::Mode::Decrypt,
                );
                relay(peer_addr, peer_sock, remote_sock, args, pool, buf).await;
            }
            Commands::Server => {
                let peer_sock = EncryptStream::new(
                    peer_sock,
                    encryption.clone(),
                    crate::encrypt_stream::Mode::Decrypt,
                );
                let remote_sock = EncryptStream::new(
                    remote_sock,
                    encryption.clone(),
                    crate::encrypt_stream::Mode::Encrypt,
                );
                relay(peer_addr, peer_sock, remote_sock, args, pool, buf).await;
            }
        }
        return;
    }
    relay(peer_addr, peer_sock, remote_sock, args, pool, buf).await;
}

async fn relay<T, U>(
    peer_addr: SocketAddr,
    mut peer_sock: T,
    mut remote_sock: U,
    args: Arc<Args>,
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    if let Some(buf) = buf {
        if let Err(err) = remote_sock.write_all(&buf.0.as_ref()[..buf.1]).await {
            tracing::error!("peer {peer_addr} failed sending first packet to remote: {err}");
            return;
        }
    }
    let duration = match args.deadline {
        Some(deadline) => std::time::Duration::from_secs(deadline),
        None => std::time::Duration::from_secs(84600 * 365),
    };

    let deadline = tokio::time::sleep(duration);
    tokio::pin!(deadline);

    let mut peer_buf = pool.get();
    let mut remote_buf = pool.get();

    loop {
        tokio::select! {
            result = peer_sock.read(&mut peer_buf) => {
                let n = match result {
                    Ok(n) => n,
                    Err(err) => {
                        tracing::error!("peer {peer_addr} read failed: {err}");
                        return;
                    }
                };

                if n == 0 {
                    // EOF
                    tracing::error!("peer {peer_addr} read received EOF");
                    return;
                }

                if let Err(err) = remote_sock.write_all(&peer_buf[..n]).await {
                    tracing::error!("peer {peer_addr} write remote error: {err}");
                    return;
                }
            }
            result = remote_sock.read(&mut remote_buf) => {
                let n = match result {
                    Ok(n) => n,
                    Err(err) => {
                        tracing::error!("peer {peer_addr} remote read failed: {err}");
                        return;
                    }
                };

                if n == 0 {
                    // EOF
                    tracing::error!("peer {peer_addr} read remote received EOF");
                    return;
                }

                if let Err(err) = peer_sock.write_all(&remote_buf[..n]).await {
                    tracing::error!("peer {peer_addr} write remote error: {err}");
                    return;
                }
            }
            _ = &mut deadline => {
                tracing::error!("peer {peer_addr} reached deadline");
                return;
            }
        }
    }
}
