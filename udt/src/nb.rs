use std::{collections::HashSet, io::{self, PipeReader, PipeWriter, Read, Write}, mem, net::SocketAddr, os::fd::{AsFd, AsRawFd}, ptr, sync::{Arc, LazyLock, Weak}, time::Duration};

use parking_lot::Mutex;
use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
use tokio::{sync::Notify, task::spawn_blocking};
use udt_sys::{Socket, UDT_EPOLL_ERR, UDT_EPOLL_IN, UDT_EPOLL_OUT};
use weak_table::WeakValueHashMap;

use crate::{instance::Instance, util::udt_getlasterror};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Watcher {
    Readable,
    Writable
}

#[derive(Debug)]
struct Poller {
    _inst: Instance,
    eid: i32,
    introduced: Mutex<HashSet<udt_sys::Socket>>,
    pollers: Mutex<WeakValueHashMap<(udt_sys::Socket, Watcher), Weak<Notify>>>,
    pr: Mutex<PipeReader>,
    pw: Mutex<PipeWriter>
}

fn set_nonblocking<Fd: AsFd>(fd: Fd) -> io::Result<()> {
    let oflags = fcntl_getfl(&fd)?;
    fcntl_setfl(&fd, oflags | OFlags::NONBLOCK)?;
    Ok(())
}

impl Poller {
    fn new() -> io::Result<Self> {
        let inst = Instance::default();
        let eid = unsafe { udt_sys::epoll_create() };
        if eid < 0 {
            return Err(udt_getlasterror());
        }
        let (pr, pw) = io::pipe()?;
        set_nonblocking(&pr)?;
        set_nonblocking(&pw)?;
        let events = UDT_EPOLL_IN;
        let res = unsafe { udt_sys::epoll_add_ssock(eid, udt_sys::SysSocket(pr.as_raw_fd()), &events) };
        if res < 0 {
            unsafe { udt_sys::epoll_release(eid) };
            return Err(udt_getlasterror());
        }
        Ok(Self {
            _inst: inst,
            eid,
            introduced: Default::default(),
            pollers: Default::default(),
            pr: Mutex::new(pr),
            pw: Mutex::new(pw)
        })
    }

    fn wait(&self) -> io::Result<()> {
        let mut readfds = Vec::new();
        let mut writefds = Vec::new();
        let mut lrfds = Vec::new();
        let res = unsafe { udt_sys::epoll_wait(
            self.eid,
            &mut readfds,
            &mut writefds,
            -1,
            &mut lrfds,
            ptr::null_mut()
        ) };
        if res < 0 {
            return Err(udt_getlasterror());
        }
        if !lrfds.is_empty() {
            println!("woken up");
            let mut buf = [0; 1];
            while let Ok(_) = self.pr.lock().read(&mut buf) {}
        }
        let guard = self.pollers.lock();
        for socket in readfds {
            if let Some(n) = guard.get(&(socket, Watcher::Readable)) {
                n.notify_waiters();
            }
        }
        for socket in writefds {
            if let Some(n) = guard.get(&(socket, Watcher::Writable)) {
                n.notify_waiters();
            }
        }
        Ok(())
    }

    fn worker(&self) -> io::Result<()> {
        loop {
            {
                let introduced = self.introduced.lock();
                let watchers = self.pollers.lock();

                for &socket in introduced.iter() {
                    let mut flags = 0;
                    if watchers.contains_key(&(socket, Watcher::Readable)) {
                        flags |= UDT_EPOLL_ERR | UDT_EPOLL_IN;
                    }
                    if watchers.contains_key(&(socket, Watcher::Writable)) {
                        flags |= UDT_EPOLL_ERR | UDT_EPOLL_OUT;
                    }
                    let res = unsafe { udt_sys::epoll_update_usock(self.eid, socket, &flags) };
                    if res < 0 {
                        return Err(udt_getlasterror());
                    }
                }
            }
            self.wait()?;
        }
    }

    fn introduce(&self, u: Socket) -> io::Result<()> {
        let mut guard = self.introduced.lock();
        let events = 0;
        let res = unsafe { udt_sys::epoll_add_usock(
            self.eid,
            u,
            &events
        ) };
        if res < 0 {
            return Err(udt_getlasterror());
        }
        guard.insert(u);
        Ok(())
    }

    fn obituary(&self, u: Socket) -> io::Result<()> {
        let mut guard = self.introduced.lock();
        let res = unsafe { udt_sys::epoll_remove_usock(
            self.eid,
            u
        ) };
        if res < 0 {
            return Err(udt_getlasterror());
        }
        guard.remove(&u);
        Ok(())
    }

    async fn readable(&self, u: Socket) -> io::Result<()> {
        let nt = {
            let mut guard = self.pollers.lock();
            let n = guard.entry((u, Watcher::Readable)).or_insert_with(Default::default);
            println!("awakening");
            let _ = self.pw.lock().write_all(&[0]);
            n.notified_owned()
        };
        nt.await;
        Ok(())
    }

    async fn writable(&self, u: Socket) -> io::Result<()> {
        let nt = {
            let mut guard = self.pollers.lock();
            let n = guard.entry((u, Watcher::Writable)).or_insert_with(Default::default);
            println!("awakening");
            let _ = self.pw.lock().write_all(&[0]);
            n.notified_owned()
        };
        nt.await;
        Ok(())
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        unsafe { udt_sys::epoll_release(self.eid); }
    }
}

static POLLER: LazyLock<Poller> = LazyLock::new(|| {
    spawn_blocking(|| POLLER.worker().unwrap());
    Poller::new().unwrap()
});

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

        if false {
            POLLER.introduce(con.0.u)?;
        }
        
        Ok(con)
    }
}

impl Drop for DatagramConnection {
    fn drop(&mut self) {
        if false {
            let _ = POLLER.obituary(self.0.u);
        }
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
        if false {
            return POLLER.readable(self.0.u).await;
        }
        
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
        if false {
            return POLLER.writable(self.0.u).await;
        }

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