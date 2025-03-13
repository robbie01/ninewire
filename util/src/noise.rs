use std::{io, pin::Pin, task::{Context, Poll}};

use bytes::BytesMut;
use futures::{SinkExt as _, TryStreamExt as _};
use length::SixteenBitDelimitedCodec;
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::{codec::Framed, io::{SinkWriter, StreamReader}};

mod codec;
mod length;
use codec::NoiseCodec;

const TAG_LEN: u16 = 16;
const MAX_BUF: usize = 65535;
const MAX_PAYLOAD: usize = MAX_BUF - TAG_LEN as usize;

#[derive(Debug, Clone, Copy)]
pub enum Side<'a> {
    Initiator { remote_public_key: &'a [u8] },
    Responder
}

#[pin_project]
pub struct NoiseStream<T> {
    #[pin]
    inner: SinkWriter<StreamReader<Framed<T, NoiseCodec>, BytesMut>>
}

impl<T> NoiseStream<T> {
    pub fn remote_public_key(&self) -> Option<&[u8]> {
        self.inner.get_ref().get_ref().codec().get_remote_static()
    }
}

impl<T: AsyncRead + AsyncWrite> NoiseStream<T> {
    pub async fn new(inner: T, privkey: &[u8], side: Side<'_>) -> io::Result<Self> where T: Unpin {
        let mut inner = Framed::new(inner, SixteenBitDelimitedCodec);

        let handshake = snow::Builder::new("Noise_IK_25519_AESGCM_SHA256".parse().unwrap())
            .local_private_key(privkey).map_err(io::Error::other)?;

        let mut handshake = match side {
            Side::Initiator { remote_public_key } => handshake
                .remote_public_key(remote_public_key).map_err(io::Error::other)?
                .build_initiator().map_err(io::Error::other)?,
            Side::Responder => handshake.build_responder().map_err(io::Error::other)?
        };

        let mut write_buf = vec![0; MAX_BUF];

        while !handshake.is_handshake_finished() {
            if handshake.is_my_turn() {
                let n = handshake.write_message(&[], &mut write_buf)
                    .map_err(io::Error::other)?;
                inner.send(&write_buf[..n]).await?;
            } else {
                let Some(buf) = inner.try_next().await?
                    else { return Err(io::ErrorKind::UnexpectedEof.into()) };
                handshake.read_message(&buf[..], &mut [])
                    .map_err(io::Error::other)?;
            }
        }

        let st =  handshake.into_transport_mode().map_err(io::Error::other)?;

        eprintln!("handshake complete");

        Ok(Self { inner: SinkWriter::new(
            StreamReader::new(
                inner.map_codec(|_| NoiseCodec::new(st)))) })
    }
}

impl<T: AsyncRead> AsyncRead for NoiseStream<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.project();
        me.inner.poll_read(cx, buf)
    }
}

impl<T: AsyncWrite> AsyncWrite for NoiseStream<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let me = self.project();
        me.inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let me = self.project();
        me.inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let me = self.project();
        me.inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        let me = self.project();
        me.inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}