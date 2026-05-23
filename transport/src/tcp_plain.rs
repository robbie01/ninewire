use std::{io::{self, ErrorKind}, pin::Pin};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{SinkExt as _, StreamExt, stream::Peekable};
use tokio::{net::{TcpStream, tcp::{OwnedReadHalf, OwnedWriteHalf}}, sync::Mutex};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::NpTransport;

pub struct PlainTcpTransport {
    read_half: Mutex<Peekable<FramedRead<OwnedReadHalf, LengthDelimitedCodec>>>,
    write_half: Mutex<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>>
}

impl PlainTcpTransport {
    pub fn new(inner: TcpStream) -> io::Result<Self> {
        let codec = LengthDelimitedCodec::builder()
            .little_endian()
            .length_field_type::<u32>()
            .length_adjustment(-4)
            .new_codec();

        let (r, w) = inner.into_split();

        Ok(Self {
            read_half: Mutex::new(FramedRead::new(
                r,
                codec.clone()
            ).peekable()),
            write_half: Mutex::new(FramedWrite::new(w, codec))
        })
    }
}

#[async_trait]
impl NpTransport for PlainTcpTransport {
    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut inner = self.read_half.lock().await;

        if let Some(Ok(r)) = Pin::new(&mut *inner).peek().await && buf.len() < r.len() {
            return Err(io::Error::other("receive buffer too small"));
        }

        let r = inner.next().await.ok_or(ErrorKind::UnexpectedEof)??;
        let n = r.len().min(buf.len());
        buf[..n].copy_from_slice(&r[..n]);
        Ok(n)
    }

    async fn send(&self, buf: &[u8]) -> io::Result<()> {
        self.write_half.lock().await.send(Bytes::copy_from_slice(buf)).await
    }

    async fn flush(&self) -> io::Result<()> {
        self.write_half.lock().await.flush().await
    }
}