use std::{io, mem, net::SocketAddr, sync::Arc, time::Duration};

use tokio::task::spawn_blocking;

use crate::util::udt_getlasterror;


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

// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// enum Watcher {
//     Readable,
//     Writable
// }

// #[derive(Debug)]
// struct Poller {
//     _inst: Instance,
//     eid: i32,
//     pollers: Mutex<WeakValueHashMap<(udt_sys::Socket, Watcher), Weak<Notify>>>
// }

// impl Poller {
//     fn new() -> io::Result<Self> {
//         let inst = Instance::default();
//         let eid = unsafe { udt_sys::epoll_create() };
//         if eid < 0 {
//             return Err(udt_getlasterror());
//         }
//         Ok(Self {
//             _inst: inst,
//             eid,
//             pollers: Default::default()
//         })
//     }

//     fn wait(&self) -> io::Result<()> {
//         let mut readfds = Vec::new();
//         let mut writefds = Vec::new();
//         let res = unsafe { udt_sys::epoll_wait(
//             self.eid,
//             &mut readfds,
//             &mut writefds,
//             -1,
//             ptr::null_mut(),
//             ptr::null_mut()
//         ) };
//         if res < 0 {
//             return Err(udt_getlasterror());
//         }
//         let guard = self.pollers.lock();
//         for socket in readfds {
//             if let Some(n) = guard.get(&(socket, Watcher::Readable)) {
//                 n.notify_waiters();
//             }
//         }
//         for socket in writefds {
//             if let Some(n) = guard.get(&(socket, Watcher::Writable)) {
//                 n.notify_waiters();
//             }
//         }
//         Ok(())
//     }

//     fn introduce(&self, u: Socket) -> io::Result<()> {
//         let res = unsafe { udt_sys::epoll_add_usock(
//             self.eid,
//             u,
//             ptr::null_mut()
//         ) };
//         if res < 0 {
//             return Err(udt_getlasterror());
//         }
//         Ok(())
//     }

//     fn obituary(&self, u: Socket) -> io::Result<()> {
//         let res = unsafe { udt_sys::epoll_remove_usock(
//             self.eid,
//             u
//         ) };
//         if res < 0 {
//             return Err(udt_getlasterror());
//         }
//         Ok(())
//     }

//     async fn readable(&self, u: Socket) -> io::Result<()> {
//         let mut guard = self.pollers.lock();
//         let n = guard.entry((u, Watcher::Readable)).or_insert_with(Default::default);
//         let nt = n.notified_owned();
//         drop(guard);
//         nt.await;
//         Ok(())
//     }

//     async fn writable(&self, u: Socket) -> io::Result<()> {
//         let mut guard = self.pollers.lock();
//         let n = guard.entry((u, Watcher::Writable)).or_insert_with(Default::default);
//         let nt = n.notified_owned();
//         drop(guard);
//         nt.await;
//         Ok(())
//     }
// }

// impl Drop for Poller {
//     fn drop(&mut self) {
//         unsafe { udt_sys::epoll_release(self.eid); }
//     }
// }

#[derive(Debug)]
pub struct DatagramConnection(Arc<super::DatagramConnection>);

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