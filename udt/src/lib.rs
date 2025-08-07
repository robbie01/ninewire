mod instance;
mod util;
use std::{future::poll_fn, i32, io, mem, net::SocketAddr, ptr, sync::Arc, task::Poll, time::Duration};

use instance::*;
use tokio::task::spawn_blocking;
use util::*;
use os_socketaddr::OsSocketAddr;
use udt_sys::{INVALID_SOCK};

#[derive(Debug)]
struct Socket {
    _inst: Instance,
    inner: udt_sys::Socket
}

impl Socket {
    fn local_addr_os(&self) -> io::Result<OsSocketAddr> {
        let mut addr = OsSocketAddr::new();
        let mut namelen = addr.len() as i32;
        let res = unsafe { udt_sys::getsockname(self.inner, addr.as_mut_ptr().cast(), &mut namelen) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(addr)
    }

    fn peer_addr_os(&self) -> io::Result<OsSocketAddr> {
        let mut addr = OsSocketAddr::new();
        let mut namelen = addr.len() as i32;
        let res = unsafe { udt_sys::getpeername(self.inner, addr.as_mut_ptr().cast(), &mut namelen) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(addr)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.local_addr_os().map(|addr| addr.into_addr().unwrap())
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.peer_addr_os().map(|addr| addr.into_addr().unwrap())
    }

    fn register(&self, interest: udt_sys::Event) -> impl Future<Output = io::Result<()>> {
        poll_fn(move |cx| {
            let rpoll = unsafe { udt_sys::getrpoll() };
            if rpoll.query(self.inner).intersects(interest) {
                return Poll::Ready(Ok(()));
            }
            rpoll.register(self.inner, interest, cx.waker());
            if rpoll.query(self.inner).intersects(interest) {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        })
    }

    fn readable(&self) -> impl Future<Output = io::Result<()>> {
        self.register(udt_sys::Event::IN | udt_sys::Event::ERR)
    }

    fn writable(&self) -> impl Future<Output = io::Result<()>> {
        self.register(udt_sys::Event::OUT | udt_sys::Event::ERR)
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe { udt_sys::close(self.inner) };
    }
}

#[derive(Debug)]
pub struct Endpoint {
    binding: Socket
}

#[derive(Debug)]
pub struct DatagramListener {
    u: Socket
}

#[derive(Debug)]
pub struct DatagramConnection {
    u: Socket
}

impl Endpoint {
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        let inst = Instance::default();
        let binding = unsafe { udt_sys::socket(
            match addr {
                SocketAddr::V4(_) => libc::AF_INET,
                SocketAddr::V6(_) => libc::AF_INET6
            }, libc::SOCK_DGRAM, 0
        ) };
        if binding == INVALID_SOCK {
            return Err(unsafe { udt_getlasterror() });
        }
        let addr = OsSocketAddr::from(addr);
        let res = unsafe { udt_sys::bind(
            binding,
            addr.as_ptr().cast(),
            addr.len() as i32
        ) };
        if res == -1 {
            unsafe { udt_sys::close(binding) };
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(Self {
            binding: Socket { _inst: inst, inner: binding }
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.binding.local_addr()
    }

    fn listen(&self, type_: i32, backlog: u32) -> io::Result<Socket> {
        let inst = Instance::default();
        let addr = self.binding.local_addr_os()?;
        let u = unsafe { udt_sys::socket(
            match addr.into_addr().unwrap() {
                SocketAddr::V4(_) => libc::AF_INET,
                SocketAddr::V6(_) => libc::AF_INET6
            }, type_, 0
        ) };
        if u == INVALID_SOCK {
            return Err(unsafe { udt_getlasterror() });
        }
        let res = unsafe { udt_sys::bind(
            u,
            addr.as_ptr().cast(),
            addr.len() as i32
        ) };
        if res == -1 {
            unsafe { udt_sys::close(u) };
            return Err(unsafe { udt_getlasterror() });
        }
        let res = unsafe { udt_sys::listen(u, backlog.try_into().unwrap_or(i32::MAX)) };
        if res == -1 {
            unsafe { udt_sys::close(u) };
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(Socket { _inst: inst, inner: u})
    }

    fn connect(&self, type_: i32, addr: SocketAddr, rendezvous: bool) -> io::Result<Socket> {
        let inst = Instance::default();
        let local_addr = self.binding.local_addr_os()?;
        let u = unsafe { udt_sys::socket(
            match local_addr.into_addr().unwrap() {
                SocketAddr::V4(_) => libc::AF_INET,
                SocketAddr::V6(_) => libc::AF_INET6
            }, type_, 0
        ) };
        if u == INVALID_SOCK {
            return Err(unsafe { udt_getlasterror() });
        }
        let res = unsafe { udt_sys::bind(
            u,
            local_addr.as_ptr().cast(),
            local_addr.len() as i32
        ) };
        if res == -1 {
            unsafe { udt_sys::close(u) };
            return Err(unsafe { udt_getlasterror() });
        }
        if rendezvous {
            let res = unsafe { udt_sys::setsockopt(
                u, 0,
                udt_sys::SocketOption::Rendezvous,
                (&rendezvous as *const bool).cast(),
                mem::size_of::<bool>() as i32
            ) };
            if res == -1 {
                unsafe { udt_sys::close(u) };
                return Err(unsafe { udt_getlasterror() });
            }
        }
        let addr = OsSocketAddr::from(addr);
        let res = unsafe { udt_sys::connect(u, addr.as_ptr().cast(), addr.len() as i32) };
        if res == -1 {
            unsafe { udt_sys::close(u) };
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(Socket { _inst: inst, inner: u })
    }

    pub fn listen_datagram(&self, backlog: u32) -> io::Result<DatagramListener> {
        let u = self.listen(libc::SOCK_DGRAM, backlog)?;
        Ok(DatagramListener { u })
    }

    pub fn connect_datagram(&self, addr: SocketAddr, rendezvous: bool) -> io::Result<DatagramConnection> {
        let u = self.connect(libc::SOCK_DGRAM, addr, rendezvous)?;
        Ok(DatagramConnection { u })
    }

    pub async fn connect_datagram_async(self: &Arc<Self>, addr: SocketAddr, rendezvous: bool) -> io::Result<DatagramConnection> {
        let inner = self.clone();
        let con = spawn_blocking(move || inner.connect_datagram(addr, rendezvous)).await.unwrap()?;

        let syn = false;
        let res = unsafe { udt_sys::setsockopt(
            con.u.inner,
            0,
            udt_sys::SocketOption::RecvSyn,
            (&syn as *const bool).cast(),
            mem::size_of::<bool>() as i32
        ) };
        if res < 0 {
            return Err(unsafe { udt_getlasterror() });
        }
        
        Ok(con)
    }
}

impl DatagramListener {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.u.local_addr()
    }

    pub fn try_accept(&self) -> io::Result<DatagramConnection> {
        let inst = Instance::default();
        let rpoll = unsafe { udt_sys::getrpoll() };
        rpoll.with_lock(self.u.inner, |s| {
            // This might deadlock.
            let u = unsafe { udt_sys::accept(self.u.inner, ptr::null_mut(), ptr::null_mut()) };
            if u == INVALID_SOCK {
                let e = unsafe { udt_getlasterror() };
                if e.kind() == io::ErrorKind::WouldBlock {
                    *s = s.difference(udt_sys::Event::IN);
                }
                return Err(e);
            }
            Ok(DatagramConnection { u: Socket { _inst: inst, inner: u } })
        })
    }

    pub async fn accept(&self) -> io::Result<DatagramConnection> {
        loop {
            match self.try_accept() {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                v => break v
            }
            self.u.readable().await?;
        }
    }
}

impl DatagramConnection {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.u.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.u.peer_addr()
    }

    fn recv_inner(&self, buf: &mut [u8]) -> io::Result<usize> {
        let res = unsafe { udt_sys::recvmsg(self.u.inner, buf.as_mut_ptr().cast(), buf.len().try_into().unwrap_or(i32::MAX)) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(res.try_into().unwrap())
    }

    fn send_with_inner(&self, buf: &[u8], ttl: Option<Duration>, inorder: bool) -> io::Result<usize> {
        let res = unsafe { udt_sys::sendmsg(
            self.u.inner,
            buf.as_ptr().cast(),
            buf.len().try_into().unwrap_or(i32::MAX),
            ttl.map_or(-1, |ttl| ttl.as_millis().try_into().unwrap()),
            inorder
        ) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(res.try_into().unwrap())
    }

    pub fn try_recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let rpoll = unsafe { udt_sys::getrpoll() };
        rpoll.with_lock(self.u.inner, |s| {
            // This might deadlock.
            let res = self.recv_inner(buf);
            if res.as_ref().is_err_and(|e| e.kind() == io::ErrorKind::WouldBlock) {
                *s = s.difference(udt_sys::Event::IN);
            }
            res
        })
    }
    
    pub fn try_send_with(&self, buf: &[u8], ttl: Option<Duration>, inorder: bool) -> io::Result<usize> {
        let rpoll = unsafe { udt_sys::getrpoll() };
        rpoll.with_lock(self.u.inner, |s| {
            // This might deadlock.
            let res = self.send_with_inner(buf, ttl, inorder);
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
            self.u.readable().await?;
        }
    }

    pub async fn send_with(&self, buf: &[u8], ttl: Option<Duration>, inorder: bool) -> io::Result<usize> {
        loop {
            match self.try_send_with(buf, ttl, inorder) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                v => break v
            }
            self.u.writable().await?;
        }
    }

    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.send_with(buf, None, true).await
    }
}
