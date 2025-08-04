use std::{io, mem, net::SocketAddr, sync::Arc, time::Duration};

use tokio::task::spawn_blocking;

use crate::util::udt_getlasterror;

#[derive(Debug)]
pub struct DatagramConnection(Arc<super::DatagramConnection>);

impl super::Endpoint {
    pub async fn connect_datagram_async(self: &Arc<Self>, addr: SocketAddr, rendezvous: bool) -> io::Result<DatagramConnection> {
        let inner = self.clone();
        let con = spawn_blocking(move || inner.connect_datagram(addr, rendezvous)).await.unwrap()
            .map(|c| DatagramConnection(c.into()))?;

        let syn = false;
        let res = unsafe { udt_sys::setsockopt(
            con.0.u,
            0,
            udt_sys::SocketOption::SendSyn,
            (&syn as *const bool).cast(),
            mem::size_of::<bool>() as i32
        ) };
        if res < 0 {
            return Err(udt_getlasterror());
        }
        let res = unsafe { udt_sys::setsockopt(
            con.0.u,
            0,
            udt_sys::SocketOption::RecvSyn,
            (&syn as *const bool).cast(),
            mem::size_of::<bool>() as i32
        ) };
        if res < 0 {
            return Err(udt_getlasterror());
        }
        
        Ok(con)
    }
}

impl DatagramConnection {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    pub fn remote_addr(&self) -> io::Result<SocketAddr> {
        self.0.remote_addr()
    }

    pub async fn readable(&self) -> io::Result<()> {
        let inner = self.0.clone();
        spawn_blocking(move || {
            let res = unsafe { udt_sys::select_single(inner.u, false) };
            if res < 0 {
                return Err(udt_getlasterror());
            }
            Ok(())
        }).await.unwrap()
    }

    pub async fn writable(&self) -> io::Result<()> {
        let inner = self.0.clone();
        spawn_blocking(move || {
            let res = unsafe { udt_sys::select_single(inner.u, true) };
            if res < 0 {
                return Err(udt_getlasterror());
            }
            Ok(())
        }).await.unwrap()
    }

    pub fn try_recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf)
    }
    
    pub fn try_send_with(&self, buf: &[u8], ttl: Option<Duration>, inorder: bool) -> io::Result<usize> {
        self.0.send_with(buf, ttl, inorder)
    }

    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.try_recv(buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                v => break v
            }
            self.readable().await?;
        }
    }

    pub async fn send_with(&self, buf: &[u8], ttl: Option<Duration>, inorder: bool) -> io::Result<usize> {
        loop {
            match self.try_send_with(buf, ttl, inorder) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                v => break v
            }
            self.writable().await?;
        }
    }

    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.send_with(buf, None, true).await
    }

    pub async fn flush(&self) -> io::Result<()> {
        let inner = self.0.clone();
        spawn_blocking(move || inner.flush()).await.unwrap()
    }
}