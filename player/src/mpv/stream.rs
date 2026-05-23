use std::{ffi::{CStr, c_void}, ptr, slice};

use mpv_sys::*;

#[derive(Debug, Clone, Copy)]
pub struct StreamError;

/// This is unsafe because it implies an impl of IntoStreamInfo.
/// Do not call any libmpv API functions.
pub unsafe trait Stream {
    const SEEKABLE: bool = false;
    const SIZEABLE: bool = false;
    const CANCELABLE: bool = false;

    fn read(&self, buf: &mut [u8]) -> Result<u64, StreamError>;

    fn seek(&self, offset: u64) -> Result<u64, StreamError> {
        let _ = offset;
        unimplemented!()
    }

    fn size(&self) -> Result<u64, StreamError> {
        unimplemented!()
    }

    fn cancel(&self) {
        unimplemented!()
    }
}

/// Do not call any libmpv API functions.
pub unsafe trait IntoStreamInfo {
    fn into_info(self: Box<Self>) -> mpv_stream_cb_info;
}

unsafe impl<S: Stream + Send + Sync + 'static> IntoStreamInfo for S {
    fn into_info(self: Box<Self>) -> mpv_stream_cb_info {
        unsafe extern "C" fn read_fn<S: Stream>(cookie: *mut c_void, buf: *mut i8, nbytes: u64) -> i64 {
            let s = unsafe { &mut *(cookie as *mut S) };
            let len = (nbytes as u128).min(usize::MAX as u128).min(i64::MAX as u128) as usize;
            let buf = unsafe { slice::from_raw_parts_mut(buf as *mut u8, len) };

            match s.read(buf) {
                Ok(n) => n as i64,
                Err(StreamError) => -1
            }
        }

        unsafe extern "C" fn seek_fn<S: Stream>(cookie: *mut c_void, offset: i64) -> i64 {
            let s = unsafe { &mut *(cookie as *mut S) };
            match s.seek(offset.try_into().unwrap()) {
                Ok(n) => n as i64,
                Err(StreamError) => MPV_ERROR_GENERIC.into()
            }
        }

        unsafe extern "C" fn size_fn<S: Stream>(cookie: *mut c_void) -> i64 {
            let s = unsafe { &mut *(cookie as *mut S) };
            match s.size() {
                Ok(n) if let Ok(n) = n.try_into() => n,
                Ok(_) | Err(StreamError) => MPV_ERROR_UNSUPPORTED.into()
            }
        }

        unsafe extern "C" fn cancel_fn<S: Stream>(cookie: *mut c_void) {
            let s = unsafe { &mut *(cookie as *mut S) };
            s.cancel()
        }

        unsafe extern "C" fn close_fn<S: Stream>(cookie: *mut c_void) {
            drop(unsafe { Box::from_raw(cookie as *mut S) })
        }

        let mut info = mpv_stream_cb_info {
            cookie: ptr::null_mut(),
            read_fn: Some(read_fn::<Self>),
            close_fn: Some(close_fn::<Self>),
            seek_fn: Self::SEEKABLE.then_some(seek_fn::<Self>),
            size_fn: Self::SIZEABLE.then_some(size_fn::<Self>),
            cancel_fn: Self::CANCELABLE.then_some(cancel_fn::<Self>)
        };
        info.cookie = Box::into_raw(self) as *mut c_void;
        info
    }
}

impl<'data> super::Handle<'data> {
    pub fn register_protocol_ro<
        F: Fn(&str) -> Result<Box<dyn IntoStreamInfo>, StreamError> + Sync
    >(
        &self,
        protocol: &str,
        open_fn: &'data F
    ) -> super::Result<()> {

        unsafe extern "C" fn trampoline<
            F: Fn(&str) -> Result<Box<dyn IntoStreamInfo>, StreamError>
        >(
            data: *mut c_void,
            uri: *mut i8,
            info: *mut mpv_stream_cb_info
        ) -> i32 {
            let f = unsafe { &mut *(data as *mut F) };
            let uri = unsafe { CStr::from_ptr(uri) }.to_str().unwrap();
            
            match f(uri) {
                Ok(i) => {
                    unsafe { *info = i.into_info(); }
                    0
                },
                Err(StreamError) => MPV_ERROR_LOADING_FAILED
            }
        }

        let protocol = super::to_cstr(protocol);
        super::res0(unsafe {
            mpv_stream_cb_add_ro(
                self.inner.as_ptr(),
                protocol.as_ptr(),
                open_fn as *const _ as *mut c_void,
                Some(trampoline::<F>)
            )
        })
    }
}