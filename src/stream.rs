use crate::cipher::Encryption;
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
pub struct EncryptStream<T> {
    socket: T,
    encryption: Arc<Encryption>,
    mode: Mode,
}

impl<T: AsyncRead + AsyncWrite + Unpin> EncryptStream<T> {
    #[inline]
    pub fn new(socket: T, encryption: Arc<Encryption>, mode: Mode) -> Self {
        Self {
            socket,
            encryption,
            mode,
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncRead for EncryptStream<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        let this = self.get_mut();

        let result = ready!(Pin::new(&mut this.socket).poll_read(cx, buf));

        match result {
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
            Err(e) => return Poll::Ready(Err(e)),
        }

        match this.mode {
            Mode::Encrypt => this.encryption.encrypt(buf.filled_mut()),
            Mode::Decrypt => this.encryption.decrypt(buf.filled_mut()),
        }

        Poll::Ready(Ok(()))
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncWrite for EncryptStream<T> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.socket).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
