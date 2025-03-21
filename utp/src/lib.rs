use std::{collections::VecDeque, io, marker::PhantomPinned, net::SocketAddr, os::raw::c_void, pin::Pin, slice, sync::Arc, task::{ready, Context, Poll}};

use os_socketaddr::OsSocketAddr;
use parking_lot::{Condvar, Mutex, ReentrantMutex};
use tokio::{io::{AsyncRead, AsyncWrite, ReadBuf}, net::UdpSocket, runtime::Handle, sync::Notify, task::{self, JoinHandle}};
use utp_sys::*;

static API_LOCK: ReentrantMutex<()> = ReentrantMutex::new(());

#[derive(Debug)]
struct SendPtr<T: ?Sized>(*mut T);

unsafe impl<T: ?Sized> Send for SendPtr<T> {}

impl<T: ?Sized> Clone for SendPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for SendPtr<T> {}

struct UtpContext {
    _pin: PhantomPinned,
    handle: *mut utp_context,
    socket: Arc<UdpSocket>,
    backlog: Mutex<VecDeque<(OsSocketAddr, Pin<Box<UtpSocket>>)>>,
    backlog_max: usize,
    backlog_cvar: Notify
}

unsafe impl Send for UtpContext {}
unsafe impl Sync for UtpContext {}

extern "C" fn sendto(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?

    let ctx = unsafe { &*(utp_context_get_userdata((*args).context) as *const UtpContext) } ;

    let Some(addr) = unsafe { OsSocketAddr::copy_from_raw((*args).u0.address, (*args).u1.address_len) }.into_addr() else { return 0 };
    let buf = unsafe { slice::from_raw_parts((*args).buf, (*args).len) };

    let _ = Handle::current().block_on(ctx.socket.send_to(&buf, addr));
    
    0
}

extern "C" fn on_state_change(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?

    // let ctx = unsafe { &*(utp_context_get_userdata((*args).context) as *const UtpContext) } ;

    match unsafe { (*args).u0.state } {
        UTP_STATE_CONNECT => {
            let socket = unsafe { &*(utp_get_userdata((*args).socket) as *const UtpSocket) };

            socket.connected.notify_waiters();
        }
        UTP_STATE_WRITABLE => {
            let socket = unsafe { &*(utp_get_userdata((*args).socket) as *const UtpSocket) };
            
            let mut state = socket.writable.state.lock();
            *state = true;
            socket.writable.cvar.notify_all();
        },
        _ => ()
    }

    0
}

// drop connections if backlog is full
extern "C" fn on_firewall(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?
    
    let ctx = unsafe { &*(utp_context_get_userdata((*args).context) as *const UtpContext) } ;

    (ctx.backlog.lock().len() >= ctx.backlog_max) as u64
}

extern "C" fn on_accept(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?

    let (ctx, socket, addr) = unsafe {
        let ctx = utp_context_get_userdata((*args).context) as *const UtpContext;
        Arc::increment_strong_count(ctx);
        let socket = {
            let ctx = Pin::new_unchecked(Arc::from_raw(ctx));
            UtpSocket::from_raw_parts(ctx, (*args).socket)
        };

        (
            &*ctx,
            socket,
            OsSocketAddr::copy_from_raw((*args).u0.address, (*args).u1.address_len)
        )
    };

    let mut backlog = ctx.backlog.lock();
    backlog.push_back((addr, socket));
    ctx.backlog_cvar.notify_waiters();
    
    0
}

impl UtpContext {
    fn new(socket: Arc<UdpSocket>, backlog: usize) -> Pin<Arc<Self>> {
        let _guard = API_LOCK.lock();

        let me = Arc::new(UtpContext {
            _pin: PhantomPinned,
            handle: unsafe { utp_init(2) },
            socket,
            backlog: Mutex::new(VecDeque::with_capacity(backlog)),
            backlog_max: backlog,
            backlog_cvar: Notify::new()
        });

        let me = unsafe {
            utp_context_set_userdata(me.handle, Arc::as_ptr(&me) as *mut c_void);
            Pin::new_unchecked(me)
        };

        unsafe {
            utp_set_callback(me.handle, UTP_SENDTO, sendto);
            utp_set_callback(me.handle, UTP_ON_STATE_CHANGE, on_state_change);
            utp_set_callback(me.handle, UTP_ON_FIREWALL, on_firewall);
            utp_set_callback(me.handle, UTP_ON_ACCEPT, on_accept);
        }

        me
    }
}

impl Drop for UtpContext {
    fn drop(&mut self) {
        let _guard = API_LOCK.lock();

        unsafe { utp_destroy(self.handle); }
    }
}

struct WaitHandle {
    state: Mutex<bool>,
    cvar: Condvar
}

struct UtpSocket {
    _pin: PhantomPinned,
    ctx: Pin<Arc<UtpContext>>,
    handle: *mut utp_socket,
    writable: Arc<WaitHandle>,
    write: Option<JoinHandle<io::Result<()>>>,
    shutdown: Option<JoinHandle<()>>,
    connected: Notify
}

unsafe impl Send for UtpSocket {}
unsafe impl Sync for UtpSocket {}

impl UtpSocket {
    fn new(ctx: Pin<Arc<UtpContext>>) -> Pin<Box<Self>> {
        unsafe {
            let raw = utp_create_socket(ctx.handle);
            Self::from_raw_parts(ctx, raw)
        }
    }

    unsafe fn from_raw_parts(ctx: Pin<Arc<UtpContext>>, handle: *mut utp_socket) -> Pin<Box<Self>> {
        let _guard = API_LOCK.lock();

        let me = Box::new(Self {
            _pin: PhantomPinned,
            handle,
            ctx,
            writable: Arc::new(WaitHandle { state: Mutex::new(true), cvar: Condvar::new() }),
            write: None,
            shutdown: None,
            connected: Notify::new()
        });
        unsafe { utp_set_userdata(me.handle, &raw const *me as *mut c_void) }; // cf. Box::as_ptr
        Box::into_pin(me)
    }

    fn write(self: Pin<&mut Self>) -> &mut Option<JoinHandle<io::Result<()>>> {
        &mut unsafe { self.get_unchecked_mut() }.write
    }

    fn shutdown(self: Pin<&mut Self>) -> &mut Option<JoinHandle<()>> {
        &mut unsafe { self.get_unchecked_mut() }.shutdown
    }
}

impl Drop for UtpSocket {
    fn drop(&mut self) {
        let handle = SendPtr(self.handle);
        let write = self.write.take();
        let shutdown = self.shutdown.take();
        let ctx = self.ctx.clone();

        task::spawn(async move {
            let _ctx = ctx;

            if let Some(write) = write {
                let _ = write.await;
            }

            if let Some(shutdown) = shutdown {
                let _ = shutdown.await;
            }

            task::spawn_blocking(move || {
                let handle = handle;
                let _guard = API_LOCK.lock();
                unsafe { utp_close(handle.0); }
            }).await.unwrap()
        });
    }
}

pub struct Connection {
    socket: Pin<Box<UtpSocket>>,
}

impl Connection {
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        let _guard = API_LOCK.lock();

        let mut addr = OsSocketAddr::new();
        let res = unsafe {
            let mut addrlen = addr.len();
            utp_getpeername(self.socket.handle, addr.as_mut_ptr(), &mut addrlen)
        };
        if res == -1 {
            return None;
        }
        addr.into_addr()
    }
}

impl AsyncRead for Connection {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        todo!()
    }
}

// I'm sure there's a much more efficient way to implement this using a ring buffer,
// but just ingesting all of the bytes at once works well enough.
impl AsyncWrite for Connection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.as_mut().poll_flush(cx))?;
        
        let buflen = buf.len();
        let buf = buf.to_vec();
        let handle = SendPtr(self.socket.handle);
        let writable = self.socket.writable.clone();
        
        *self.socket.as_mut().write() = Some(task::spawn_blocking(move || {
            let _guard = API_LOCK.lock();
            let handle = handle;
            let mut slice = &buf[..];

            unsafe {
                while !slice.is_empty() {
                    let mut state = writable.state.lock();
                    let res = utp_write(handle.0, slice.as_ptr() as *mut c_void, slice.len());
                    let Ok(n) = usize::try_from(res) else { return Err(io::Error::other("utp_write returned -1")) };
                    if n == 0 {
                        *state = false;
                        writable.cvar.wait_while(&mut state, |&mut able| !able);
                    } else {
                        slice = &slice[n..];
                    }
                }
            }

            Ok(())
        }));

        Poll::Ready(Ok(buflen))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.socket.as_mut().write() {
            Some(write) => {
                let res = ready!(Pin::new(write).poll(cx));
                *self.socket.as_mut().write() = None;
                Poll::Ready(res.unwrap())
            },
            None => Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let handle = SendPtr(self.socket.handle);

        let shutdown = self.socket.as_mut().shutdown().get_or_insert_with(move || task::spawn_blocking(move || {
            let handle = handle;
            let _guard = API_LOCK.lock();
            
            unsafe { utp_shutdown(handle.0, SHUT_WR); }
        }));

        let res = ready!(Pin::new(shutdown).poll(cx));
        *self.socket.as_mut().shutdown() = None;
        Poll::Ready(Ok(res.unwrap()))
    }
}
 
pub struct Endpoint {
    ctx: Pin<Arc<UtpContext>>
}

impl Endpoint {
    pub fn new(socket: Arc<UdpSocket>, backlog: usize) -> Self {
        Self { ctx: UtpContext::new(socket, backlog) }
    }

    pub async fn accept(&self) -> io::Result<(SocketAddr, Connection)> {
        loop {
            let mut backlog = self.ctx.backlog.lock();
            if let Some((addr, socket)) = backlog.pop_front() {
                break Ok((addr.into_addr().unwrap(), Connection { socket }));
            }
            let notified = self.ctx.backlog_cvar.notified();
            drop(backlog);
            notified.await;
        }
    }

    pub async fn connect(&self, peer: SocketAddr) -> io::Result<Connection> {
        let socket = UtpSocket::new(self.ctx.clone());
        let addr = OsSocketAddr::from(peer);
        let handle = SendPtr(socket.handle);

        let connected = socket.connected.notified();

        let res = task::spawn_blocking(move || unsafe {
            let handle = handle;
            utp_connect(handle.0, addr.as_ptr(), addr.len())
        }).await.unwrap();

        if res == -1 {
            return Err(io::Error::other("utp_connect returned -1"));
        }

        connected.await;

        Ok(Connection { socket })
    }
}