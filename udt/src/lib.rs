pub mod nb;
mod instance;
mod util;
use std::{i32, io, mem, net::SocketAddr, ptr, time::Duration};

use instance::*;
use util::*;
use os_socketaddr::OsSocketAddr;
use udt_sys::{Socket, INVALID_SOCK};

#[derive(Debug)]
pub struct Endpoint {
    _inst: Instance,
    binding: Socket
}

#[derive(Debug)]
pub struct StreamListener {
    _inst: Instance,
    u: Socket
}

#[derive(Debug)]
pub struct StreamConnection {
    _inst: Instance,
    u: Socket
}

#[derive(Debug)]
pub struct DatagramListener {
    _inst: Instance,
    u: Socket
}

#[derive(Debug)]
pub struct DatagramConnection {
    _inst: Instance,
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
            _inst: inst,
            binding
        })
    }

    fn local_addr_os(&self) -> io::Result<OsSocketAddr> {
        let mut addr = OsSocketAddr::new();
        let mut namelen = addr.len() as i32;
        let res = unsafe { udt_sys::getsockname(self.binding, addr.as_mut_ptr().cast(), &mut namelen) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.local_addr_os().map(|addr| addr.into_addr().unwrap())
    }

    fn listen(&self, type_: i32, backlog: u32) -> io::Result<(Instance, Socket)> {
        let inst = Instance::default();
        let addr = self.local_addr_os()?;
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
        Ok((inst, u))
    }

    fn connect(&self, type_: i32, addr: SocketAddr, rendezvous: bool) -> io::Result<(Instance, Socket)> {
        let inst = Instance::default();
        let local_addr = self.local_addr_os()?;
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
        Ok((inst, u))
    }

    pub fn listen_datagram(&self, backlog: u32) -> io::Result<DatagramListener> {
        let (inst, u) = self.listen(libc::SOCK_DGRAM, backlog)?;
        Ok(DatagramListener { _inst: inst, u })
    }

    pub fn connect_datagram(&self, addr: SocketAddr, rendezvous: bool) -> io::Result<DatagramConnection> {
        let (inst, u) = self.connect(libc::SOCK_DGRAM, addr, rendezvous)?;
        Ok(DatagramConnection { _inst: inst, u })
    }
}

impl DatagramListener {
    fn local_addr_os(&self) -> io::Result<OsSocketAddr> {
        let mut addr = OsSocketAddr::new();
        let mut namelen = addr.len() as i32;
        let res = unsafe { udt_sys::getsockname(self.u, addr.as_mut_ptr().cast(), &mut namelen) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.local_addr_os().map(|addr| addr.into_addr().unwrap())
    }

    pub fn accept(&self) -> io::Result<DatagramConnection> {
        let inst = Instance::default();
        let u = unsafe { udt_sys::accept(self.u, ptr::null_mut(), ptr::null_mut()) };
        if u == INVALID_SOCK {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(DatagramConnection { _inst: inst, u })
    }
}

impl DatagramConnection {
    fn local_addr_os(&self) -> io::Result<OsSocketAddr> {
        let mut addr = OsSocketAddr::new();
        let mut namelen = addr.len() as i32;
        let res = unsafe { udt_sys::getsockname(self.u, addr.as_mut_ptr().cast(), &mut namelen) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(addr)
    }

    fn peer_addr_os(&self) -> io::Result<OsSocketAddr> {
        let mut addr = OsSocketAddr::new();
        let mut namelen = addr.len() as i32;
        let res = unsafe { udt_sys::getpeername(self.u, addr.as_mut_ptr().cast(), &mut namelen) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.local_addr_os().map(|addr| addr.into_addr().unwrap())
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.peer_addr_os().map(|addr| addr.into_addr().unwrap())
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let res = unsafe { udt_sys::recvmsg(self.u, buf.as_mut_ptr().cast(), buf.len().try_into().unwrap_or(i32::MAX)) };
        if res == -1 {
            return Err(unsafe { udt_getlasterror() });
        }
        Ok(res.try_into().unwrap())
    }

    pub fn send_with(&self, buf: &[u8], ttl: Option<Duration>, inorder: bool) -> io::Result<usize> {
        let res = unsafe { udt_sys::sendmsg(
            self.u,
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

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.send_with(buf, None, true)
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe { udt_sys::close(self.binding) };
    }
}

impl Drop for StreamListener {
    fn drop(&mut self) {
        unsafe { udt_sys::close(self.u) };
    }
}

impl Drop for StreamConnection {
    fn drop(&mut self) {
        unsafe {
            udt_sys::close(self.u)
        };
    }
}

impl Drop for DatagramListener {
    fn drop(&mut self) {
        unsafe { udt_sys::close(self.u) };
    }
}

impl Drop for DatagramConnection {
    fn drop(&mut self) {
        unsafe {
            udt_sys::close(self.u)
        };
    }
}