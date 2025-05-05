use crate::{
    args::{Args, Commands},
    cipher::Encryptor,
};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

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

    let args = Arc::new(args);
    loop {
        let mut buf = pool.clone().get_rc();
        let (n, peer_sock, peer_addr) = bind_sock.accept(buf.as_mut()).await?;
        tracing::debug!("accepting new peer: {peer_addr}");
        tokio::spawn(new_peer(
            peer_addr,
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

    let args = Arc::new(args);
    loop {
        let (peer_sock, peer_addr) = bind_sock.accept().await?;
        tracing::debug!("accepting new peer: {peer_addr}");
        peer_sock.set_nodelay(true)?;
        tokio::spawn(new_peer(
            peer_addr,
            args.clone(),
            peer_sock,
            pool.clone(),
            None,
        ));
    }
}

async fn uot_listen(pool: Arc<PoolAllocator>, args: Args) -> std::io::Result<()> {
    let args = Arc::new(args);

    match args.command {
        Commands::Client => {
            let bind_sock = udpflow::UdpListener::new(args.listen)?;
            loop {
                let mut buf = pool.clone().get_rc();
                let (n, peer_sock, peer_addr) = bind_sock.accept(buf.as_mut()).await?;
                tracing::debug!("accepting new peer: {peer_addr}");
                tokio::spawn(new_peer(
                    peer_addr,
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
                tracing::debug!("accepting new peer: {peer_addr}");
                tokio::spawn(new_peer(
                    peer_addr,
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
    args: Arc<Args>,
    peer_sock: T,
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
{
    tracing::info!("new peer: {peer_addr}");

    let result = match args.protocol.as_str() {
        "uot" => new_peer_uot(peer_addr, args, peer_sock, pool, buf).await,
        "tcp" => new_peer_tcp(peer_addr, args, peer_sock, pool, buf).await,
        "udp" => new_peer_udp(peer_addr, args, peer_sock, pool, buf).await,
        _ => unreachable!(),
    };
    if let Err(err) = result {
        tracing::error!("error creating remote socket: {err}");
    };
}

async fn new_peer_udp<T>(
    peer_addr: SocketAddr,
    args: Arc<Args>,
    mut peer_sock: T,
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let local_bind_ip: SocketAddr = match args.remote {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
    };

    let mut remote_sock = udpflow::UdpStreamRemote::new(local_bind_ip, args.remote).await?;
    handshake(&mut peer_sock, &mut remote_sock, &args).await?;
    relay(peer_addr, peer_sock, remote_sock, args, pool, buf).await;
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
    relay(peer_addr, peer_sock, remote_sock, args, pool, buf).await;
    Ok(())
}

async fn new_peer_uot<T>(
    peer_addr: SocketAddr,
    args: Arc<Args>,
    mut peer_sock: T,
    pool: Arc<PoolAllocator>,
    buf: Option<(PoolAllocatorObject, usize)>,
) -> std::io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let local_bind_ip: SocketAddr = match args.remote {
        SocketAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        SocketAddr::V6(_) => "[::]:0".parse().unwrap(),
    };

    match args.command {
        Commands::Client => {
            let mut remote_sock = TcpStream::connect(args.remote).await?;
            remote_sock.set_nodelay(true)?;
            handshake(&mut peer_sock, &mut remote_sock, &args).await?;
            let remote_sock = udpflow::UotStream::new(remote_sock);
            relay(peer_addr, peer_sock, remote_sock, args, pool, buf).await;
        }
        Commands::Server => {
            let mut remote_sock = udpflow::UdpStreamRemote::new(local_bind_ip, args.remote).await?;
            handshake(&mut peer_sock, &mut remote_sock, &args).await?;
            let peer_sock = udpflow::UotStream::new(peer_sock);
            relay(peer_addr, peer_sock, remote_sock, args, pool, buf).await;
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
        let mut encryption_buf = pool.get();
        encryption_buf[..buf.1].copy_from_slice(&buf.0.as_ref()[..buf.1]);
        match args.command {
            Commands::Client => {
                args.encryption.encrypt(&mut encryption_buf[..buf.1]);
            }
            Commands::Server => {
                args.encryption.decrypt(&mut encryption_buf[..buf.1]);
            }
        }
        let result = remote_sock.write_all(&encryption_buf[..buf.1]).await;
        if let Err(err) = result {
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

                match args.command {
                    Commands::Client => args.encryption.encrypt(&mut peer_buf[..n]),
                    Commands::Server => args.encryption.decrypt(&mut peer_buf[..n]),
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

                match args.command {
                    Commands::Client => args.encryption.decrypt(&mut peer_buf[..n]),
                    Commands::Server => args.encryption.encrypt(&mut peer_buf[..n]),
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
