use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use tokio::task::spawn_blocking;

mod private {
    use std::sync::Arc;

    pub trait Sealed {}
    impl Sealed for Arc<super::super::Endpoint> {}
}

pub trait EndpointExt: private::Sealed {
    fn connect_datagram_async(&self, addr: SocketAddr, rendezvous: bool) -> impl Future<Output = io::Result<DatagramConnection>> + Send;
}

impl EndpointExt for Arc<super::Endpoint> {
    async fn connect_datagram_async(&self, addr: SocketAddr, rendezvous: bool) -> io::Result<DatagramConnection> {
        // This API is terrible. The Arc shenanigans are required to guarantee the binding won't be dropped.
        // Again, this should be using the UDT epoll API.

        let inner = self.clone();
        spawn_blocking(move || inner.connect_datagram(addr, rendezvous)).await.unwrap()
            .map(|c| DatagramConnection(c.into()))
    }
}

#[derive(Debug)]
pub struct DatagramConnection(Arc<super::DatagramConnection>);

impl DatagramConnection {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    pub fn remote_addr(&self) -> io::Result<SocketAddr> {
        self.0.remote_addr()
    }

    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        // This *sucks*. Not going to fix it because we should be using the UDT epoll API anyway

        let inner = self.0.clone();
        let mut tmp = vec![0; buf.len()];
        let tmp = spawn_blocking(move || {
            let n = inner.recv(&mut tmp)?;
            tmp.truncate(n);
            Ok::<_, io::Error>(tmp)
        }).await.unwrap()?;
        buf[..tmp.len()].copy_from_slice(&tmp);
        Ok(tmp.len())
    }

    pub async fn send(&self, buf: &[u8], ttl: Option<Duration>, inorder: bool) -> io::Result<usize> {
        // This also *sucks*.

        let inner = self.0.clone();
        let tmp = buf.to_vec();
        spawn_blocking(move || inner.send_with(&tmp, ttl, inorder)).await.unwrap()
    }

    pub async fn flush(&self) -> io::Result<()> {
        let inner = self.0.clone();
        spawn_blocking(move || inner.flush()).await.unwrap()
    }
}