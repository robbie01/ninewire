#![allow(clippy::type_complexity)]

use std::{io, pin::Pin};

use async_trait::async_trait;
use bytes::Bytes;

#[cfg(feature = "compio")]
pub mod compio;

#[cfg(feature = "tokio")]
pub mod tokio;

#[async_trait(?Send)]
pub trait NpTransport {
    fn max_msize(&self) -> u32 { u32::MAX }

    async fn recv(&self) -> io::Result<Bytes>;
    async fn send(&self, buf: Bytes) -> io::Result<()>;

    async fn flush(&self) -> io::Result<()>;
}

#[async_trait]
pub trait SyncNpTransport: Send + Sync {
    fn max_msize(&self) -> u32 { u32::MAX }

    async fn recv(&self) -> io::Result<Bytes>;
    async fn send(&self, buf: Bytes) -> io::Result<()>;

    async fn flush(&self) -> io::Result<()>;
}

impl<T: SyncNpTransport + ?Sized> NpTransport for T {
    fn max_msize(&self) -> u32 {
        <T as SyncNpTransport>::max_msize(self)
    }

    // Woe! Woe that we must double-box futures to strip the Send constraint!

    fn send<'a, 'c>(&'a self, buf: Bytes) -> Pin<Box<dyn Future<Output = io::Result<()>> + 'c>> where 'a: 'c, Self: 'c {
        Box::pin(<T as SyncNpTransport>::send(self, buf))
    }

    fn recv<'a, 'c>(&'a self) -> Pin<Box<dyn Future<Output = io::Result<Bytes> > + 'c>> where 'a: 'c, Self: 'c {
        Box::pin(<T as SyncNpTransport>::recv(self))
    }

    fn flush<'a, 'c>(&'a self) -> Pin<Box<dyn Future<Output = io::Result<()>> + 'c>> where 'a: 'c, Self: 'c {
        Box::pin(<T as SyncNpTransport>::flush(self))
    }
}