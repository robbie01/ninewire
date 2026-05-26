use std::{rc::Rc, sync::Arc};

#[async_trait]
pub trait NpTransport {
    fn max_msize(&self) -> u32 { u32::MAX }

    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize>;
    async fn send(&self, buf: &[u8]) -> io::Result<()>;

    async fn flush(&self) -> io::Result<()>;
}

impl<L: NpTransport, R: NpTransport> NpTransport for Either<L, R> {
    fn max_msize(&self) -> u32 {
        match self {
            Either::Left(l) => l.max_msize(),
            Either::Right(r) => r.max_msize()
        }
    }

    fn send<'a, 'b, 'c>(&'a self, buf: &'b [u8]) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'c>> where 'a: 'c, 'b: 'c, Self: 'c {
        match self {
            Either::Left(t) => t.send(buf),
            Either::Right(t) => t.send(buf)
        }
    }

    fn recv<'a, 'b, 'c>(&'a self, buf: &'b mut [u8]) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'c>> where 'a: 'c, 'b: 'c, Self: 'c {
        match self {
            Either::Left(t) => t.recv(buf),
            Either::Right(t) => t.recv(buf)
        }
    }

    fn flush<'a, 'c>(&'a self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'c>> where 'a: 'c, Self: 'c {
        match self {
            Either::Left(t) => t.flush(),
            Either::Right(t) => t.flush()
        }
    }
}

impl<T: NpTransport + ?Sized> NpTransport for Box<T> {
    fn max_msize(&self) -> u32 {
        (**self).max_msize()
    }

    fn send<'a, 'b, 'c>(&'a self, buf: &'b [u8]) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'c>> where 'a: 'c, 'b: 'c, Self: 'c {
        (**self).send(buf)
    }

    fn recv<'a, 'b, 'c>(&'a self, buf: &'b mut [u8]) -> Pin<Box<dyn Future<Output = io::Result<usize> > + Send + 'c>> where 'a: 'c, 'b: 'c, Self: 'c {
        (**self).recv(buf)
    }

    fn flush<'a, 'c>(&'a self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'c>> where 'a: 'c, Self: 'c {
        (**self).flush()
    }
}

impl<T: NpTransport + ?Sized> NpTransport for Arc<T> {
    fn max_msize(&self) -> u32 {
        (**self).max_msize()
    }

    fn send<'a, 'b, 'c>(&'a self, buf: &'b [u8]) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'c>> where 'a: 'c, 'b: 'c, Self: 'c {
        (**self).send(buf)
    }

    fn recv<'a, 'b, 'c>(&'a self, buf: &'b mut [u8]) -> Pin<Box<dyn Future<Output = io::Result<usize> > + Send + 'c>> where 'a: 'c, 'b: 'c, Self: 'c {
        (**self).recv(buf)
    }

    fn flush<'a, 'c>(&'a self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'c>> where 'a: 'c, Self: 'c {
        (**self).flush()
    }
}

impl<T: NpTransport + ?Sized> NpTransport for Rc<T> {
    fn max_msize(&self) -> u32 {
        (**self).max_msize()
    }

    fn send<'a, 'b, 'c>(&'a self, buf: &'b [u8]) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'c>> where 'a: 'c, 'b: 'c, Self: 'c {
        (**self).send(buf)
    }

    fn recv<'a, 'b, 'c>(&'a self, buf: &'b mut [u8]) -> Pin<Box<dyn Future<Output = io::Result<usize> > + Send + 'c>> where 'a: 'c, 'b: 'c, Self: 'c {
        (**self).recv(buf)
    }

    fn flush<'a, 'c>(&'a self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'c>> where 'a: 'c, Self: 'c {
        (**self).flush()
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "secure")] {
        #[derive(Debug, Clone, Copy)]
        pub enum Side<'a> {
            Initiator { remote_public_key: &'a [u8] },
            Responder { local_private_key: &'a [u8] }
        }
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "secure-transport")] {
        mod udt;
        pub use udt::*;

        use std::{io, pin::Pin};

        use async_trait::async_trait;
        use either::Either;

        #[async_trait]
        impl NpTransport for SecureTransport {
            fn max_msize(&self) -> u32 {
                1280 - 64 - 8 - 16
            }

            async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
                self.recv(buf).await
            }

            async fn send(&self, buf: &[u8]) -> io::Result<()> {
                self.send(buf).await
            }

            async fn flush(&self) -> io::Result<()> {
                self.flush().await
            }
        }
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "tcp")] {
        mod tcp;
        pub use tcp::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "tcp-plain")] {
        mod tcp_plain;
        pub use tcp_plain::*;
    }
}