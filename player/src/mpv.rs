use std::{borrow::Cow, ffi::{CStr, CString, c_void}, fmt::Display, future::poll_fn, iter::once, marker::PhantomData, mem, ptr::{self, NonNull}, sync::{Arc, Mutex}, task::{Poll, Waker}};

use mpv_sys::*;
use stable_vec::StableVec;

mod node;
mod stream;
pub use {node::*, stream::*};

fn to_cstr(s: &str) -> Cow<'_, CStr> {
    if let Ok(s) = CStr::from_bytes_until_nul(s.as_bytes()) {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(CString::new(s).unwrap())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Error {
    AsyncInterrupted,
    Library { code: i32 }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Library { code } => {
                let name = unsafe {
                    CStr::from_ptr(mpv_error_string(code))
                };
                f.write_str(name.to_str().unwrap())
            },
            Self::AsyncInterrupted => {
                f.write_str("async operation interrupted")
            }
        }
    }
}

fn res0(v: i32) -> Result<()> {
    if v < 0 {
        Err(Error::Library { code: v })
    } else {
        Ok(())
    }
}

#[derive(Debug)]
enum AsyncState {
    Pending(Waker),
    Complete(Result<()>)
}

#[derive(Debug, Clone, Copy)]
pub enum Event<'a> {
    Shutdown,
    LogMessage {
        prefix: &'a CStr,
        level: &'a CStr,
        text: &'a CStr
    },
    StartFile,
    EndFile,
    FileLoaded,
    ClientMessage,
    VideoReconfig,
    AudioReconfig,
    Seek,
    PlaybackRestart,
    QueueOverflow
}

#[derive(Debug)]
pub struct EventToken(Arc<()>);

#[derive(Debug)]
pub struct Handle<'data> {
    inner: NonNull<mpv_handle>,
    in_flight: Arc<Mutex<StableVec<AsyncState>>>,
    token: Arc<()>,
    _phantom: PhantomData<&'data ()>
}

unsafe impl<'data> Send for Handle<'data> {}
unsafe impl<'data> Sync for Handle<'data> {}

impl<'data> Handle<'data> {
    pub fn new() -> Result<(Self, EventToken)> {
        let inner = NonNull::new(unsafe { mpv_create() })
            .expect("mpv_create returned null");

        let token = Arc::new(());

        let this = Self {
            inner,
            in_flight: Arc::new(Mutex::new(StableVec::new())),
            token: token.clone(),
            _phantom: PhantomData
        };

        // TODO: opportunity to set initialization-time properties

        res0(unsafe { mpv_initialize(this.inner.as_ptr()) })?;

        Ok((this, EventToken(token)))
    }

    /// Ensure that the callback doesn't call any libmpv API functions.
    /// 
    /// There's other contractual stipulations. See client.h. Here be dragons.
    pub unsafe fn set_wakeup_callback<C: Fn() + Sync>(&self, cb: &'data C) {
        unsafe extern "C" fn trampoline<C: Fn()>(data: *mut c_void) {
            (unsafe { &*(data as *const C) })()
        }

        unsafe {
            mpv_set_wakeup_callback(
                self.inner.as_ptr(),
                Some(trampoline::<C>),
                cb as *const _ as *mut c_void
            );
        }
    }

    pub fn request_log_messages(&self, level: &str) -> Result<()> {
        let level = to_cstr(level);

        res0(unsafe {
            mpv_request_log_messages(
                self.inner.as_ptr(),
                level.as_ptr()
            )
        })
    }

    pub fn poll_event<R>(&self, token: &mut EventToken, cb: impl FnOnce(Event<'_>) -> R) -> Option<R>  {
        assert!(Arc::ptr_eq(&self.token, &token.0));

        loop {
            let ev = unsafe { *mpv_wait_event(self.inner.as_ptr(), 0.) };
            if ev.event_id == MPV_EVENT_NONE {
                break None
            }

            break Some(match ev.event_id {
                // todo: add command data
                MPV_EVENT_COMMAND_REPLY | MPV_EVENT_SET_PROPERTY_REPLY => {
                    let mut in_flight = self.in_flight.lock().unwrap();
                    let id = ev.reply_userdata as usize;
                    let res = res0(ev.error);
                    let AsyncState::Pending(w) = mem::replace(&mut in_flight[id], AsyncState::Complete(res))
                        else { panic!("unexpected async state (not pending)") };
                    w.wake();
                    continue;
                },
                MPV_EVENT_GET_PROPERTY_REPLY => todo!(),

                MPV_EVENT_SHUTDOWN => {
                    let mut in_flight = self.in_flight.lock().unwrap();

                    for item in in_flight.values_mut() {
                        if !matches!(item, AsyncState::Pending(_)) {
                            continue;
                        }
                        let AsyncState::Pending(w) = mem::replace(item, AsyncState::Complete(Err(Error::AsyncInterrupted)))
                            else { unreachable!() };
                        w.wake();
                    }

                    // no reentrancy risk; self is borrowed exclusively
                    cb(Event::Shutdown)
                },
                MPV_EVENT_LOG_MESSAGE => cb(unsafe { // maybe add numeric log level?
                    let lm = *(ev.data as *const mpv_event_log_message);
                    Event::LogMessage {
                        prefix: CStr::from_ptr(lm.prefix),
                        level: CStr::from_ptr(lm.level),
                        text: CStr::from_ptr(lm.text)
                    }
                }),
                MPV_EVENT_START_FILE => cb(Event::StartFile), // needs data
                MPV_EVENT_END_FILE => cb(Event::EndFile), // needs data
                MPV_EVENT_FILE_LOADED => cb(Event::FileLoaded),
                MPV_EVENT_CLIENT_MESSAGE => cb(Event::ClientMessage), // needs data
                MPV_EVENT_VIDEO_RECONFIG => cb(Event::VideoReconfig),
                MPV_EVENT_AUDIO_RECONFIG => cb(Event::AudioReconfig),
                MPV_EVENT_SEEK => cb(Event::Seek),
                MPV_EVENT_PLAYBACK_RESTART => cb(Event::PlaybackRestart),
                MPV_EVENT_QUEUE_OVERFLOW => cb(Event::QueueOverflow),

                _ => break None
            })
        }
    }

    pub fn set_property<'args>(&self, name: &'args str, data: &'args Node<'args>) -> impl Future<Output = Result<()>> + 'args {
        let inner = self.inner;
        let in_flight = self.in_flight.clone();
        let mut id = None;
        let mut yielded = false;

        poll_fn(move |ctx| {
            assert!(!yielded);

            if let Some(id) = id {
                let mut in_flight = in_flight.lock().unwrap();

                if let AsyncState::Pending(ref mut w) = in_flight[id] {
                    w.clone_from(ctx.waker());
                    return Poll::Pending
                }

                yielded = true;
                let AsyncState::Complete(r) = in_flight.remove(id).unwrap() else { unreachable!() };
                Poll::Ready(r)
            } else {
                let name = to_cstr(name);

                let i = in_flight.lock().unwrap().push(AsyncState::Pending(ctx.waker().clone()));
                id = Some(i);

                res0(unsafe {
                    mpv_set_property_async(
                        inner.as_ptr(),
                        i as u64,
                        name.as_ptr(),
                        MPV_FORMAT_NODE,
                        data as *const _ as *mut c_void
                    )
                })?;

                Poll::Pending
            }
        })
    }

    // pub fn get_property<'args, R: 'args>(&self, name: &'args str, cb: impl FnOnce(Node<'_>) -> R + 'args) -> impl Future<Output = Result<R>> + 'args {
    //     poll_fn(todo!())
    // }

    pub fn command<'args>(&self, args: &'args [&str]) -> impl Future<Output = Result<()>> + 'args {
        let inner = self.inner;
        let in_flight = self.in_flight.clone();
        let mut id = None;
        let mut yielded = false;

        poll_fn(move |ctx| {
            assert!(!yielded);

            if let Some(id) = id {
                let mut in_flight = in_flight.lock().unwrap();

                if let AsyncState::Pending(ref mut w) = in_flight[id] {
                    w.clone_from(ctx.waker());
                    return Poll::Pending
                }

                yielded = true;
                let AsyncState::Complete(r) = in_flight.remove(id).unwrap() else { unreachable!() };
                Poll::Ready(r)
            } else {
                let args = args.iter()
                    .map(|&s| to_cstr(s))
                    .collect::<Vec<_>>();
                let mut arg_ptrs = args.iter()
                    .map(|arg| arg.as_ptr())
                    .chain(once(ptr::null()))
                    .collect::<Vec<_>>();

                let i = in_flight.lock().unwrap().push(AsyncState::Pending(ctx.waker().clone()));
                id = Some(i);

                res0(unsafe {
                    mpv_command_async(
                        inner.as_ptr(),
                        i as u64,
                        arg_ptrs.as_mut_ptr()
                    )
                })?;

                Poll::Pending
            }
        })
    }
}

impl<'data> Drop for Handle<'data> {
    fn drop(&mut self) {
        println!("destroying mpv handle");
        unsafe { mpv_destroy(self.inner.as_ptr()) };
        println!("destroyed mpv handle");
    }
}