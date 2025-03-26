use std::{ffi::c_int, io, marker::PhantomPinned, net::SocketAddr, os::raw::c_void, pin::Pin, ptr::{self, NonNull}, slice, sync::{atomic::{AtomicBool, Ordering}, Arc}, task::{ready, Context, Poll}, time::Duration};

use bytes::BytesMut;
use crossbeam::{atomic::AtomicCell, queue::ArrayQueue};
use futures::task::AtomicWaker;
use os_socketaddr::OsSocketAddr;
use parking_lot::{Condvar, Mutex, ReentrantMutex};
use pin_weak::sync::PinWeak;
use ringbuf::{traits::{Consumer, Observer, Producer}, Cons, HeapRb, Prod};
use tokio::{io::{AsyncRead, AsyncWrite, ReadBuf}, net::UdpSocket, runtime::Handle, sync::Notify, task::{self, JoinHandle}, time::{interval, MissedTickBehavior}};
use tokio_util::net::Listener;
use utp_sys::*;

static API_LOCK: ReentrantMutex<()> = ReentrantMutex::new(());

// Bandwidth-delay product of 1000 Mbps * 200 ms
// Provides gigabit service (upper limit of virtually all residential links) up to an RTT of 200 ms
const READ_BUFFER_SIZE: usize = 25_000_000;

#[derive(Debug)]
struct SendPtr<T: ?Sized>(*mut T);

unsafe impl<T: ?Sized> Send for SendPtr<T> {}

impl<T: ?Sized> Clone for SendPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for SendPtr<T> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UtpError {
    ConnectionReset,
    TimedOut
}

impl From<UtpError> for io::Error {
    fn from(value: UtpError) -> Self {
        match value {
            UtpError::ConnectionReset => io::ErrorKind::ConnectionReset,
            UtpError::TimedOut => io::ErrorKind::TimedOut
        }.into()
    }
}

const _: () = assert!(AtomicCell::<Option<UtpError>>::is_lock_free());

struct UtpContext {
    _pin: PhantomPinned,
    handle: *mut utp_context,
    socket: Arc<UdpSocket>,
    backlog: Option<ArrayQueue<(OsSocketAddr, Pin<Arc<UtpSocket>>)>>,
    backlog_waker: AtomicWaker,
    backlog_cvar: Notify
}

unsafe impl Send for UtpContext {}
unsafe impl Sync for UtpContext {}

unsafe fn ctx_userdata(handle: *mut utp_context) -> Option<NonNull<UtpContext>> {
    NonNull::new(unsafe { utp_context_get_userdata(handle) }.cast())
}

unsafe fn socket_userdata(handle: *mut utp_socket) -> Option<NonNull<UtpSocket>> {
    NonNull::new(unsafe { utp_get_userdata(handle) }.cast())
}

// The single most problematic callback here. It ruins EVERYTHING and makes the
// entire API blocking.
// The alternative is to queue up writes, but there's no clean way to apply backpressure
// on that front.
extern "C" fn sendto(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?

    let ctx = unsafe { 
        let Some(ctx) = ctx_userdata((*args).context) else { return 0 };
        ctx.as_ref()
    };

    let Some(addr) = unsafe { OsSocketAddr::copy_from_raw((*args).u0.address, (*args).u1.address_len) }.into_addr() else { return 0 };
    let buf = unsafe { slice::from_raw_parts((*args).buf, (*args).len) };
    let _ = Handle::current().block_on(ctx.socket.send_to(&buf, addr));
    
    0
}

extern "C" fn on_read(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?
    let socket = unsafe { 
        let Some(socket) = socket_userdata((*args).socket) else { return 0 };
        socket.as_ref()
    };

    let mut prod = Prod::new(&socket.read_buffer);
    let incoming = unsafe { slice::from_raw_parts((*args).buf, (*args).len) };


    if prod.vacant_len() > incoming.len() {
        prod.push_slice(incoming);
        socket.readable.wake();
        unsafe { utp_read_drained(socket.handle); }
    }

    0
}

extern "C" fn on_state_change(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?
    let socket = unsafe { 
        let Some(socket) = socket_userdata((*args).socket) else { return 0 };
        socket.as_ref()
    };

    match unsafe { (*args).u0.state } {
        UTP_STATE_CONNECT => {
            socket.connected.notify_waiters();
        }
        UTP_STATE_WRITABLE => {
            let mut state = socket.writable.state.lock();
            *state = true;
            socket.writable.cvar.notify_all();
        },
        UTP_STATE_EOF => {
            socket.eof.store(true, Ordering::Relaxed);
            socket.readable.wake();
        },
        _ => ()
    }

    0
}

// drop connections if backlog is full
extern "C" fn on_firewall(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?
    
    let ctx = unsafe { 
        let Some(ctx) = ctx_userdata((*args).context) else { return 0 };
        ctx.as_ref()
    };

    ctx.backlog.as_ref().is_none_or(ArrayQueue::is_full) as u64
}

extern "C" fn on_accept(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?

    let (ctx, socket, addr) = unsafe {
        let Some(ctx) = ctx_userdata((*args).context) else { return 0 };
        let ctx = ctx.as_ptr();
        Arc::increment_strong_count(ctx);
        let socket = {
            let ctx = Pin::new_unchecked(Arc::from_raw(ctx));
            UtpSocket::from_raw_parts(ctx, (*args).socket, false)
        };

        (
            &*ctx,
            socket,
            OsSocketAddr::copy_from_raw((*args).u0.address, (*args).u1.address_len)
        )
    };

    if let Some(ref backlog) = ctx.backlog {
        let _ = backlog.push((addr, socket));
        ctx.backlog_waker.wake();
        ctx.backlog_cvar.notify_waiters();
    }
    
    0
}

extern "C" fn on_error(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?

    let socket = unsafe { 
        let Some(socket) = socket_userdata((*args).socket) else { return 0 };
        socket.as_ref()
    };

    match unsafe { (*args).u0.error_code } {
        UTP_ECONNREFUSED => {
            socket.connection_refused.store(true, Ordering::Relaxed);
            socket.connected.notify_waiters();
        },
        e@(UTP_ECONNRESET | UTP_ETIMEDOUT) => {
            let err = if e == UTP_ECONNRESET { UtpError::ConnectionReset } else { UtpError::TimedOut };

            let mut writable = socket.writable.state.lock();

            socket.io_error.store(Some(err));

            socket.readable.wake();
            *writable = true;
            socket.writable.cvar.notify_all();
        },
        _ => ()
    }

    0
}

extern "C" fn get_read_buffer_size(args: *mut utp_callback_arguments) -> u64 {
    let _guard = API_LOCK.lock(); // is this really necessary?

    let socket = unsafe { 
        let Some(socket) = socket_userdata((*args).socket) else { return 0 };
        socket.as_ref()
    };

    c_int::try_from(socket.read_buffer.occupied_len()).unwrap_or(c_int::MAX) as u64
}

impl UtpContext {
    fn new(socket: Arc<UdpSocket>, backlog: usize) -> Pin<Arc<Self>> {
        let _guard = API_LOCK.lock();

        let me = Arc::new(UtpContext {
            _pin: PhantomPinned,
            handle: utp_init(2),
            socket,
            backlog: (backlog > 0).then(|| ArrayQueue::new(backlog)),
            backlog_waker: AtomicWaker::new(),
            backlog_cvar: Notify::new()
        });

        let me = unsafe {
            utp_context_set_userdata(me.handle, Arc::as_ptr(&me) as *mut c_void);
            Pin::new_unchecked(me)
        };

        unsafe {
            utp_set_callback(me.handle, UTP_SENDTO, sendto);
            utp_set_callback(me.handle, UTP_ON_READ, on_read);
            utp_set_callback(me.handle, UTP_ON_STATE_CHANGE, on_state_change);
            utp_set_callback(me.handle, UTP_ON_FIREWALL, on_firewall);
            utp_set_callback(me.handle, UTP_ON_ACCEPT, on_accept);
            utp_set_callback(me.handle, UTP_ON_ERROR, on_error);
            utp_set_callback(me.handle, UTP_GET_READ_BUFFER_SIZE, get_read_buffer_size);
        }

        me
    }
}

impl Drop for UtpContext {
    fn drop(&mut self) {
        let _guard = API_LOCK.lock();

        unsafe {
            utp_context_set_userdata(self.handle, ptr::null_mut());
            utp_destroy(self.handle);
        }
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
    readable: AtomicWaker,
    eof: AtomicBool,
    read_buffer: HeapRb<u8>,
    writable: Arc<WaitHandle>,
    connection_refused: AtomicBool,
    connected: Notify,
    io_error: Arc<AtomicCell<Option<UtpError>>>
}

unsafe impl Send for UtpSocket {}
unsafe impl Sync for UtpSocket {}

impl UtpSocket {
    fn new(ctx: Pin<Arc<UtpContext>>, writable: bool) -> Pin<Arc<Self>> {
        let _guard = API_LOCK.lock();

        unsafe {
            let raw = utp_create_socket(ctx.handle);
            Self::from_raw_parts(ctx, raw, writable)
        }
    }

    unsafe fn from_raw_parts(ctx: Pin<Arc<UtpContext>>, handle: *mut utp_socket, writable: bool) -> Pin<Arc<Self>> {
        let _guard = API_LOCK.lock();

        let me = Arc::new(Self {
            _pin: PhantomPinned,
            handle,
            ctx,
            readable: AtomicWaker::new(),
            eof: AtomicBool::new(false),
            read_buffer: HeapRb::new(READ_BUFFER_SIZE),
            writable: Arc::new(WaitHandle { state: Mutex::new(writable), cvar: Condvar::new() }),
            connected: Notify::new(),
            connection_refused: AtomicBool::new(false),
            io_error: Arc::new(AtomicCell::new(None))
        });
        unsafe {
            utp_setsockopt(me.handle, UTP_RCVBUF, READ_BUFFER_SIZE.try_into().unwrap());
            utp_set_userdata(me.handle, Arc::as_ptr(&me) as *mut c_void);
            Pin::new_unchecked(me)
        }
    }
}

impl Drop for UtpSocket {
    fn drop(&mut self) {
        unsafe {
            let _guard = API_LOCK.lock();
            utp_set_userdata(self.handle, ptr::null_mut());
        }

        let handle = SendPtr(self.handle);
        let ctx = self.ctx.clone();

        task::spawn_blocking(move || {
            let _guard = API_LOCK.lock();
            let _ctx = ctx;
            let handle = handle;
            unsafe { utp_close(handle.0); }
        });
    }
}

pub struct Connection {
    socket: Pin<Arc<UtpSocket>>,
    write: Option<JoinHandle<io::Result<()>>>,
    shutdown: Option<JoinHandle<()>>
}

impl Drop for Connection {
    fn drop(&mut self) {
        let socket = self.socket.clone();
        let write = self.write.take();
        let shutdown = self.shutdown.take();
        tokio::spawn(async move {
            let _socket = socket;

            if let Some(write) = write {
                let _ = write.await;
            }

            if let Some(shutdown) = shutdown {
                let _ = shutdown.await;
            }
        });
    }
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
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        if let Some(err) = self.socket.io_error.load() {
            return Poll::Ready(Err(err.into()));
        }

        self.socket.readable.register(cx.waker());

        // check again after registration
        if let Some(err) = self.socket.io_error.load() {
            return Poll::Ready(Err(err.into()));
        }

        let mut cons = Cons::new(&self.socket.read_buffer);
        if cons.is_empty() && !self.socket.eof.load(Ordering::Relaxed) {
            return Poll::Pending;
        }

        let n = buf.remaining();

        let (l, r) = cons.as_slices();
        let nl = l.len().min(n);
        let nr = r.len().min(n - nl);
        buf.put_slice(&l[..nl]);
        buf.put_slice(&r[..nr]);
        cons.skip(nl + nr);
        Poll::Ready(Ok(()))
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
        let io_error = self.socket.io_error.clone();
        
        self.write = Some(task::spawn_blocking(move || {
            let mut _guard = API_LOCK.lock();
            let handle = handle;
            let mut slice = &buf[..];

            unsafe {
                while !slice.is_empty() {
                    let mut state = writable.state.lock();

                    if let Some(err) = io_error.load() {
                        return Err(err.into());
                    }

                    let res = utp_write(handle.0, slice.as_ptr() as *mut c_void, slice.len());
                    let Ok(n) = usize::try_from(res) else { return Err(io::Error::other("utp_write returned -1")) };
                    if n == 0 {
                        *state = false;
                        drop(_guard);
                        writable.cvar.wait_while(&mut state, |&mut able| !able);
                        _guard = API_LOCK.lock();
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
        match self.write {
            Some(ref mut write) => {
                let res = ready!(Pin::new(write).poll(cx));
                self.write = None;
                Poll::Ready(res.unwrap())
            },
            None => Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        
        let handle = SendPtr(self.socket.handle);

        let shutdown = self.shutdown.get_or_insert_with(move || task::spawn_blocking(move || {
            let handle = handle;
            let _guard = API_LOCK.lock();
            
            unsafe { utp_shutdown(handle.0, SHUT_WR); }
        }));

        let res = ready!(Pin::new(shutdown).poll(cx));
        self.shutdown = None;
        Poll::Ready(Ok(res.unwrap()))
    }
}
 
pub struct Endpoint {
    ctx: Pin<Arc<UtpContext>>
}

fn con_from_backlog((addr, socket): (OsSocketAddr, Pin<Arc<UtpSocket>>)) -> (Connection, SocketAddr) {
    (
        Connection {
            socket,
            write: None,
            shutdown: None
        },
        addr.into_addr().unwrap()
    )
}

impl Endpoint {
    pub fn new(socket: Arc<UdpSocket>, backlog: usize) -> Self {
        let ctx = UtpContext::new(socket, backlog);

        let _read_task = {
            let socket = ctx.socket.clone();
            let ctx = PinWeak::downgrade(ctx.clone());
            task::spawn(async move {
                let read_buf = Arc::new(Mutex::new(BytesMut::with_capacity(65535)));

                loop {
                    socket.readable().await?;
                    let Some(ctx) = ctx.upgrade() else { break };

                    let socket = socket.clone();
                    let read_buf = read_buf.clone();
                    task::spawn_blocking(move || {
                        let mut read_buf = read_buf.lock();
                        let _guard = API_LOCK.lock();
                        loop {
                            read_buf.truncate(0);
                            let (_, addr) = match socket.try_recv_buf_from(&mut *read_buf) {
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                                v => v?
                            };
                            let addr = OsSocketAddr::from(addr);

                            unsafe { utp_process_udp(ctx.handle, read_buf.as_ptr(), read_buf.len(), addr.as_ptr(), addr.len()); }
                        }
                        unsafe { utp_issue_deferred_acks(ctx.handle); }

                        Ok::<_, io::Error>(())
                    }).await.unwrap()?;
                }

                Ok::<_, io::Error>(())
            })
        };

        let _timeout_task = {
            let ctx = PinWeak::downgrade(ctx.clone());
            task::spawn(async move {
                let mut int = interval(Duration::from_millis(500));
                int.set_missed_tick_behavior(MissedTickBehavior::Delay);
                loop {
                    int.tick().await;
                    let Some(ctx) = ctx.upgrade() else { break };

                    let handle = SendPtr(ctx.handle);
                    task::spawn_blocking(move || {
                        let _guard = API_LOCK.lock();
                        let handle = handle;
                        unsafe { utp_check_timeouts(handle.0); }
                    }).await.unwrap();
                }
            })
        };

        Self { ctx }
    }

    pub fn poll_accept(&self, cx: &mut Context<'_>) -> Poll<io::Result<(Connection, SocketAddr)>> {
        let Some(ref backlog) = self.ctx.backlog else { return Poll::Ready(Err(io::ErrorKind::InvalidInput.into())) };

        // hot path
        if let Some(con) = backlog.pop() {
            return Poll::Ready(Ok(con_from_backlog(con)));
        }

        self.ctx.backlog_waker.register(cx.waker());

        match backlog.pop() {
            None => Poll::Pending,
            Some(con) => Poll::Ready(Ok(con_from_backlog(con)))
        }
    }

    pub async fn accept(&self) -> io::Result<(Connection, SocketAddr)> {
        let Some(ref backlog) = self.ctx.backlog else { return Err(io::ErrorKind::InvalidInput.into()) };

        loop {
            if let Some(con) = backlog.pop() {
                break Ok(con_from_backlog(con));
            }
            let notified = self.ctx.backlog_cvar.notified();
            if let Some(con) = backlog.pop() {
                break Ok(con_from_backlog(con));
            }
            notified.await;
        }
    }

    pub async fn connect(&self, peer: impl Into<SocketAddr>) -> io::Result<Connection> {
        let socket = UtpSocket::new(self.ctx.clone(), true);
        let addr = OsSocketAddr::from(peer.into());
        let handle = SendPtr(socket.handle);

        let connected = socket.connected.notified();

        let res = task::spawn_blocking(move || unsafe {
            let handle = handle;
            let _guard = API_LOCK.lock();
            utp_connect(handle.0, addr.as_ptr(), addr.len())
        }).await.unwrap();

        if res == -1 {
            return Err(io::Error::other("utp_connect returned -1"));
        }

        connected.await;

        if socket.connection_refused.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::ConnectionRefused.into());
        }

        Ok(Connection {
            socket,
            write: None,
            shutdown: None
        })
    }
}

impl Listener for Endpoint {
    type Io = Connection;
    type Addr = SocketAddr;

    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<(Self::Io, Self::Addr)>> {
        Endpoint::poll_accept(&*self, cx)
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.ctx.socket.local_addr()
    }
}