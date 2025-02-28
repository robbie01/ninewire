use std::{io, pin::Pin, task::{ready, Context, Poll}};

use bytes::{Buf, BufMut, BytesMut};
use futures::{Sink, SinkExt, Stream, TryStreamExt};
use pin_project::pin_project;
use snow::TransportState;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[pin_project]
pub struct NoiseStream<T> {
    #[pin]
    inner: Framed<T, LengthDelimitedCodec>,
    st: Option<TransportState>,
    read_buf: BytesMut,
}

const TAG_LEN: usize = 16;
const MAX_BUF: usize = 65535;
const MAX_PAYLOAD: usize = MAX_BUF - TAG_LEN;

#[derive(Debug, Clone, Copy)]
pub enum Side<'a> {
    Initiator { remote_public_key: &'a [u8] },
    Responder
}

impl<T> NoiseStream<T> {
    pub fn remote_public_key(&self) -> Option<&[u8]> {
        self.st.as_ref()?.get_remote_static()
    }
}

impl<T: AsyncRead + AsyncWrite> NoiseStream<T> {
    pub async fn new_init(inner: T, privkey: &[u8], side: Side<'_>) -> io::Result<Self> where T: Unpin {
        let mut this = Self::new(inner);
        Pin::new(&mut this).initialize(privkey, side).await?;
        Ok(this)
    }

    fn new(inner: T) -> Self {
        let inner = LengthDelimitedCodec::builder()
            .big_endian()
            .length_field_type::<u16>()
            .new_framed(inner);

        Self {
            inner,
            st: None,
            read_buf: BytesMut::with_capacity(MAX_PAYLOAD)
        }
    }

    async fn initialize(self: Pin<&mut Self>, privkey: &[u8], side: Side<'_>) -> io::Result<()> {
        let mut this = self.project();

        if this.st.is_some() {
            return Err(io::Error::other("already initialized"));
        }

        let handshake = snow::Builder::new("Noise_IK_25519_AESGCM_BLAKE2s".parse().unwrap())
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
                this.inner.send(buf.freeze()).await?;
            } else {
                let Some(buf) = this.inner.try_next().await?
                    else { return Err(io::ErrorKind::UnexpectedEof.into()) };
                handshake.read_message(&buf[..], &mut [])
                    .map_err(io::Error::other)?;
            }
        }

        *this.st = Some(handshake.into_transport_mode().map_err(io::Error::other)?);

        Ok(())
    }
}

impl<T: AsyncRead> AsyncRead for NoiseStream<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>
    ) -> Poll<io::Result<()>> {
        let mut this = self.project();
        let st = this.st.as_mut().ok_or_else(|| io::Error::other("uninitialized"))?;

        while this.read_buf.is_empty() {
            let buf = ready!(this.inner.as_mut().poll_next(cx)).transpose()?;
            match buf {
                None => return Poll::Ready(Ok(())),
                Some(buf) => {
                    this.read_buf.put_bytes(0, buf.len().saturating_sub(TAG_LEN));
                    match st.read_message(&buf, &mut this.read_buf[..]) {
                        Ok(n) => assert_eq!(this.read_buf.len(), n),
                        Err(e) => {
                            this.read_buf.truncate(0);
                            return Poll::Ready(Err(io::Error::other(e)));
                        }
                    }
                }
            }
        }

        let n = buf.remaining().min(this.read_buf.len());
        buf.put_slice(&this.read_buf[..n]);
        this.read_buf.advance(n);
        Poll::Ready(Ok(()))
    }
}

// TODO: this could be buffered for performance
impl<T: AsyncWrite> AsyncWrite for NoiseStream<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut this = self.project();
        let st = this.st.as_mut().ok_or_else(|| io::Error::other("uninitialized"))?;

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        ready!(this.inner.as_mut().poll_ready(cx))?;

        if buf.len() > MAX_PAYLOAD {
            buf = &buf[..MAX_PAYLOAD];
        }

        let mut msg = BytesMut::zeroed(buf.len() + TAG_LEN);
        st.write_message(buf, &mut msg[..]).map_err(io::Error::other)?;
        this.inner.as_mut().start_send(msg.freeze())?;

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let this = self.project();
        
        if this.st.is_none() {
            return Poll::Ready(Err(io::Error::other("uninitialized")));
        }

        this.inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let this = self.project();

        if this.st.is_none() {
            return Poll::Ready(Err(io::Error::other("uninitialized")));
        }

        this.inner.poll_close(cx)
    }
}