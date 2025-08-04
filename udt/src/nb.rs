use std::{future::poll_fn, io, mem, net::SocketAddr, sync::Arc, task::Poll, time::Duration};

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

        let maxbw = i64::MAX;
        let res = unsafe { udt_sys::setsockopt(
            con.0.u,
            0,
            udt_sys::SocketOption::MaxBandwidth,
            (&maxbw as *const i64).cast(),
            mem::size_of::<i64>() as i32
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

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.0.peer_addr()
    }

    fn register(&self, interest: udt_sys::Event) -> impl Future<Output = io::Result<()>> {
        poll_fn(move |cx| {
            let rpoll = unsafe { udt_sys::getrpoll() };
            if rpoll.query(self.0.u).intersects(interest) {
                return Poll::Ready(Ok(()));
            }
            rpoll.register(self.0.u, interest, cx.waker());
            if rpoll.query(self.0.u).intersects(interest) {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        })
    }

    pub fn readable(&self) -> impl Future<Output = io::Result<()>> {
        self.register(udt_sys::Event::IN | udt_sys::Event::ERR)
    }

    pub fn writable(&self) -> impl Future<Output = io::Result<()>> {
        self.register(udt_sys::Event::OUT | udt_sys::Event::ERR)
    }

    pub fn try_recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let rpoll = unsafe { udt_sys::getrpoll() };
        rpoll.with_lock(self.0.u, |s| {
            // This might deadlock.
            let res = self.0.recv(buf);
            if res.as_ref().is_err_and(|e| e.kind() == io::ErrorKind::WouldBlock) {
                *s = s.difference(udt_sys::Event::IN);
            }
            res
        })
    }
    
    pub fn try_send_with(&self, buf: &[u8], ttl: Option<Duration>, inorder: bool) -> io::Result<usize> {
        let rpoll = unsafe { udt_sys::getrpoll() };
        rpoll.with_lock(self.0.u, |s| {
            // This might deadlock.
            let res = self.0.send_with(buf, ttl, inorder);
            if res.as_ref().is_err_and(|e| e.kind() == io::ErrorKind::WouldBlock) {
                *s = s.difference(udt_sys::Event::OUT);
            }
            res
        })
    }

    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.try_recv(buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                v => break v
            }
            // println!("waiting for readable");
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