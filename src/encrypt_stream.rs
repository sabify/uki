use crate::cipher::Encryptor;
use futures::ready;
use std::io::Result;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub enum Mode {
    Encrypt,
    Decrypt,
}
pub struct EncryptStream<T, U>
where
    U: Encryptor + Unpin,
{
    io: T,
    encryption: Arc<U>,
    mode: Mode,
}

impl<T, U> EncryptStream<T, U>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Encryptor + Unpin,
{
    #[inline]
    pub fn new(io: T, encryption: Arc<U>, mode: Mode) -> Self {
        Self {
            io,
            encryption,
            mode,
        }
    }
}

impl<T, U> AsyncRead for EncryptStream<T, U>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Encryptor + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        let this = self.get_mut();

        let result = ready!(Pin::new(&mut this.io).poll_read(cx, buf));

        match result {
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
            Err(e) => return Poll::Ready(Err(e)),
        }

        match this.mode {
            Mode::Encrypt => this.encryption.encrypt(buf),
            Mode::Decrypt => this.encryption.decrypt(buf),
        }

        Poll::Ready(Ok(()))
    }
}

impl<T, U> AsyncWrite for EncryptStream<T, U>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Encryptor + Unpin,
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.io).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
