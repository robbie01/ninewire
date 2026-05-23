use std::{io::{self, ErrorKind}, mem, pin::Pin, task::{Context, Poll, ready}};

use async_trait::async_trait;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures_core::Stream as _;
use futures_sink::Sink as _;
use futures_util::{SinkExt as _, StreamExt, stream::Peekable};
use snow::TransportState;
use tokio::{io::{AsyncRead, AsyncWrite}, net::TcpStream, sync::Mutex};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::{Side, NpTransport};

// Whatever man. Allocate everything. Allocate allocate allocate.
// Waste the whole damn heap bro. Allocate for every syscall.

// I'll try to minimize copies someday.

struct NoiseTransport {
    con: Framed<TcpStream, LengthDelimitedCodec>,
    crypto: TransportState,
    write_buf: BytesMut,
    cipher_buf: BytesMut,
    read_buf: BytesMut
}

const NOISE_THRESHOLD: usize = 65535 - 8 - 16;

impl NoiseTransport {
    async fn new(con: TcpStream, side: Side<'_>) -> io::Result<Self> {
        let mut con = Framed::new(con,
            LengthDelimitedCodec::builder()
                .big_endian()
                .length_field_type::<u16>()
                .new_codec());
        
        let crypto = snow::Builder::new("Noise_NK_25519_AESGCM_SHA256".parse().unwrap());
        let mut crypto = match side {
            Side::Initiator { remote_public_key } => crypto
                .remote_public_key(remote_public_key).map_err(io::Error::other)?
                .build_initiator().unwrap(),
            Side::Responder { local_private_key } => crypto
                .local_private_key(local_private_key).map_err(io::Error::other)?
                .build_responder().unwrap()
        };

        while !crypto.is_handshake_finished() {
            if crypto.is_my_turn() {
                let mut buf = BytesMut::zeroed(64);
                let n = crypto.write_message(&[], &mut buf).map_err(io::Error::other)?;
                buf.truncate(n);
                con.send(buf.freeze()).await?;
            } else {
                let buf = con.next().await.unwrap()?;
                crypto.read_message(&buf[..], &mut []).map_err(io::Error::other)?;
            }
        }

        Ok(Self {
            con,
            crypto: crypto.into_transport_mode().map_err(io::Error::other)?,
            write_buf: BytesMut::new(),
            cipher_buf: BytesMut::new(),
            read_buf: BytesMut::new()
        })
    }
}

impl AsyncWrite for NoiseTransport {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let mut this = self.get_mut();

        let n = loop {
            let n = buf.len().min(NOISE_THRESHOLD - this.write_buf.len());
            if n == 0 {
                ready!(Pin::new(&mut this).poll_flush(cx))?;
            } else {
                break n;
            }
        };

        this.write_buf.put_slice(&buf[..n]);
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let this = self.get_mut();
        let mut con = Pin::new(&mut this.con);

        if !this.cipher_buf.is_empty() {
            ready!(con.as_mut().poll_ready(cx))?;

            con.as_mut().start_send(mem::take(&mut this.cipher_buf).freeze())?;
        }

        if !this.write_buf.is_empty() {
            ready!(con.as_mut().poll_ready(cx))?;

            this.cipher_buf = BytesMut::zeroed(this.write_buf.len() + 8 + 16);
            let n = this.crypto.write_message(&this.write_buf, &mut this.cipher_buf).map_err(io::Error::other)?;
            assert_eq!(n, this.cipher_buf.len());

            this.write_buf.clear();
            
            con.as_mut().start_send(mem::take(&mut this.cipher_buf).freeze())?;
        }
        con.as_mut().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let mut this = self.get_mut();

        ready!(Pin::new(&mut this).poll_flush(cx))?;
        ready!(Pin::new(&mut this.con).poll_close(cx))?;
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for NoiseTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()))
        }

        let this = self.get_mut();

        while this.read_buf.is_empty() {
            match ready!(Pin::new(&mut this.con).poll_next(cx)) {
                Some(b) => {
                    let b = b?;
                    if b.len() > 8 + 16 {
                        this.read_buf.resize(b.len() - 8 - 16, 0);
                        let n = this.crypto.read_message(&b, &mut this.read_buf).map_err(io::Error::other)?;
                        assert_eq!(n, this.read_buf.len());
                    }
                },
                None => return Poll::Ready(Err(ErrorKind::UnexpectedEof.into()))
            }
        }

        let n = buf.remaining().min(this.read_buf.len());
        buf.put_slice(&this.read_buf[..n]);
        this.read_buf.advance(n);
        Poll::Ready(Ok(()))
    }
}

pub struct TcpTransport {
    inner: Mutex<Peekable<Framed<NoiseTransport, LengthDelimitedCodec>>>
}

impl TcpTransport {
    pub async fn new(inner: TcpStream, side: Side<'_>) -> io::Result<Self> {
        let codec = LengthDelimitedCodec::builder()
            .little_endian()
            .length_field_type::<u32>()
            .length_adjustment(-4)
            .new_codec();

        Ok(Self {
            inner: Mutex::new(Framed::new(
                NoiseTransport::new(inner, side).await?,
                codec
            ).peekable())
        })
    }
}

#[async_trait]
impl NpTransport for TcpTransport {
    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().await;

        if let Some(Ok(r)) = Pin::new(&mut *inner).peek().await && buf.len() < r.len() {
            return Err(io::Error::other("receive buffer too small"));
        }

        let r = inner.next().await.ok_or(ErrorKind::UnexpectedEof)??;
        let n = r.len().min(buf.len());
        buf[..n].copy_from_slice(&r[..n]);
        Ok(n)
    }

    async fn send(&self, buf: &[u8]) -> io::Result<()> {
        self.inner.lock().await.send(Bytes::copy_from_slice(buf)).await
    }

    async fn flush(&self) -> io::Result<()> {
        self.inner.lock().await.flush().await
    }
}