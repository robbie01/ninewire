use std::{ffi::CStr, io};

pub fn udt_strerror() -> String {
    unsafe { CStr::from_ptr(udt_sys::getlasterror_desc()) }.to_string_lossy().into_owned()
}

pub fn udt_getlasterror() -> io::Error {
    io::Error::new(io::ErrorKind::Other, udt_strerror())
}