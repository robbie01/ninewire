use std::{io, sync::Mutex, task::{Poll, ready}};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt as _, StreamExt as _, future::poll_fn};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::SyncNpTransport;

pub struct PlainTransport<Io> {
    inner: Mutex<Framed<Io, LengthDelimitedCodec>>
}

impl<Io: AsyncRead + AsyncWrite + Send> PlainTransport<Io> {
    pub fn new(inner: Io) -> Self {
        Self {
            inner: Mutex::new(LengthDelimitedCodec::builder()
                .little_endian()
                .length_field_type::<u32>()
                .length_adjustment(-4)
                .new_framed(inner))
        }
    }
}

#[async_trait]
impl<Io: AsyncRead + AsyncWrite + Unpin + Send> SyncNpTransport for PlainTransport<Io> {
    async fn recv(&self) -> io::Result<Bytes> {
        poll_fn(|cx| {
            Poll::Ready(match ready!(self.inner.lock().unwrap().poll_next_unpin(cx)) {
                Some(m) => m.map(BytesMut::freeze),
                None => Err(io::ErrorKind::UnexpectedEof.into())
            })
        }).await
    }

    async fn send(&self, buf: Bytes) -> io::Result<()> {
        let mut buf = Some(buf);

        poll_fn(|cx| {
            let mut inner = self.inner.lock().unwrap();

            if buf.is_some() {
                ready!(inner.poll_ready_unpin(cx))?;
                inner.start_send_unpin(buf.take().unwrap())?;
            }
            inner.poll_flush_unpin(cx)
        }).await
    }

    async fn flush(&self) -> io::Result<()> {
        poll_fn(|cx| {
            self.inner.lock().unwrap().poll_flush_unpin(cx)
        }).await
    }
}