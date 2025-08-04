use std::{ffi::CStr, io};

pub fn udt_strerror() -> String {
    unsafe { CStr::from_ptr(udt_sys::getlasterror_desc()) }.to_string_lossy().into_owned()
}

pub fn udt_getlasterror() -> io::Error {
    io::Error::new(
        match unsafe { udt_sys::getlasterror_code() } {
            udt_sys::EASYNCSND | udt_sys::EASYNCRCV => io::ErrorKind::WouldBlock,
            udt_sys::ENOSERVER | udt_sys::ETIMEOUT => io::ErrorKind::TimedOut,
            _ => io::ErrorKind::Other
        },
        udt_strerror()
    )
}