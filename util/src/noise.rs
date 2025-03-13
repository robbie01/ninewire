use std::{io, pin::Pin, task::{Context, Poll}};

use bytes::{Buf, BufMut, BytesMut};
use futures::{SinkExt as _, TryStreamExt as _};
use pin_project::pin_project;
use snow::TransportState;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::{codec::{Decoder, Encoder, Framed, LengthDelimitedCodec}, io::{SinkWriter, StreamReader}};

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
        self.inner.get_ref().get_ref().codec().st.get_remote_static()
    }
}

impl<T: AsyncRead + AsyncWrite> NoiseStream<T> {
    pub async fn new(inner: T, privkey: &[u8], side: Side<'_>) -> io::Result<Self> where T: Unpin {
        let mut inner = LengthDelimitedCodec::builder()
            .big_endian()
            .length_field_type::<u16>()
            .new_framed(inner);

        let handshake = snow::Builder::new("Noise_IK_25519_AESGCM_SHA256".parse().unwrap())
            .local_private_key(privkey).map_err(io::Error::other)?;

        let mut handshake = match side {
            Side::Initiator { remote_public_key } => handshake
                .remote_public_key(remote_public_key).map_err(io::Error::other)?
                .build_initiator().map_err(io::Error::other)?,
            Side::Responder => handshake.build_responder().map_err(io::Error::other)?
        };

        while !handshake.is_handshake_finished() {
            if handshake.is_my_turn() {
                let mut buf = BytesMut::zeroed(MAX_BUF);
                let n = handshake.write_message(&[], &mut buf[..])
                    .map_err(io::Error::other)?;
                buf.truncate(n);
                inner.send(buf.freeze()).await?;
            } else {
                let Some(buf) = inner.try_next().await?
                    else { return Err(io::ErrorKind::UnexpectedEof.into()) };
                handshake.read_message(&buf[..], &mut [])
                    .map_err(io::Error::other)?;
            }
        }

        let st =  handshake.into_transport_mode().map_err(io::Error::other)?;

        Ok(Self { inner: SinkWriter::new(
            StreamReader::new(
                inner.map_codec(|_| NoiseCodec { st }))) })
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

struct NoiseCodec {
    st: TransportState
}

impl<'a> Encoder<&'a [u8]> for NoiseCodec {
    type Error = io::Error;

    fn encode(&mut self, mut item: &'a [u8], dst: &mut BytesMut) -> io::Result<()> {
        while !item.is_empty() {
            let buf = &item[..item.len().min(MAX_PAYLOAD)];
            item = &item[buf.len()..];

            let n = item.len() + usize::from(TAG_LEN);
            dst.put_u16(n as u16);
            let pos = dst.len();
            dst.put_bytes(0, n.into());
            let n2 = self.st.write_message(item, &mut dst[pos..]).map_err(io::Error::other)?;
            assert_eq!(usize::from(n), n2);
        }
        Ok(())
    }
}

impl Decoder for NoiseCodec {
    type Error = io::Error;
    type Item = BytesMut;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        if src.len() < 2 { return Ok(None); }
        let n = u16::from_be_bytes(src[..2].try_into().unwrap());
        if src.len() < (usize::from(n) + 2) { return Ok(None); }
        src.advance(2);
        let Some(n2) = n.checked_sub(TAG_LEN) else { return Err(io::Error::other("too short")) };
        let ct = src.split_to(usize::from(n));
        let mut pt = BytesMut::zeroed(n2.into());
        let k = self.st.read_message(&ct, &mut pt).map_err(io::Error::other)?;
        assert_eq!(usize::from(n2), k);
        Ok(Some(pt))
    }
}