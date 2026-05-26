use std::{io, sync::Mutex, task::{Poll, ready}};

use async_trait::async_trait;
use bytes::Bytes;
use compio::{buf::{IoBufMut, Slice}, io::{AsyncRead, AsyncWrite, framed::{Framed, SymmetricFramed, codec::bytes::BytesCodec, frame::{Frame, Framer}}, util::Splittable}};
use futures_util::{SinkExt as _, StreamExt as _, future::poll_fn};

use crate::NpTransport;

struct NpFramer;

impl<B: IoBufMut> Framer<B> for NpFramer {
    fn enclose(&mut self, buf: &mut B) {
        let n = buf.buf_len();
        buf.reserve_exact(4).unwrap();
        buf.copy_within(0..n, 4);
        unsafe { buf.set_len(n + 4); }
        buf.as_mut_slice()[..4].copy_from_slice(&((n + 4) as u32).to_le_bytes());
    }

    fn extract(&mut self, buf: &Slice<B>) -> io::Result<Option<Frame>> {
        if buf.len() < 4 {
            return Ok(None);
        }

        let n = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;

        // enforce a 16MB maximum msize
        if n > 0x1000000 {
            return Err(io::Error::other("incoming message is too large"));
        }

        if buf.len() < n {
            return Ok(None);
        }

        Ok(Some(Frame::new(4, n.saturating_sub(4), 0)))
    }
}

pub struct PlainTransport<Io: Splittable + ?Sized> {
    inner: Mutex<SymmetricFramed<Io::ReadHalf, Io::WriteHalf, BytesCodec, NpFramer, Bytes>>
}

impl<Io: Splittable + ?Sized> PlainTransport<Io> {
    pub fn new(inner: Io) -> Self where Io: Sized {
        Self {
            inner: Mutex::new(Framed::new(BytesCodec::new(), NpFramer).with_duplex(inner))
        }
    }
}

#[async_trait(?Send)]
impl<Io: Splittable> NpTransport for PlainTransport<Io> where
    Io::ReadHalf: AsyncRead + Unpin + 'static,
    Io::WriteHalf: AsyncWrite + Unpin + 'static
{
    async fn recv(&self) -> io::Result<Bytes> {
        poll_fn(|cx| {
            Poll::Ready(match ready!(self.inner.lock().unwrap().poll_next_unpin(cx)) {
                Some(m) => m,
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