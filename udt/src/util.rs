use std::io;

pub unsafe fn udt_strerror() -> String {
    unsafe { udt_sys::getlasterror_desc() }.to_string_lossy().into_owned()
}

pub unsafe fn udt_getlasterror() -> io::Error {
    io::Error::new(
        match unsafe { udt_sys::getlasterror_code() } {
            udt_sys::EASYNCSND | udt_sys::EASYNCRCV => io::ErrorKind::WouldBlock,
            udt_sys::ENOSERVER => io::ErrorKind::TimedOut,
            _ => io::ErrorKind::Other
        },
        unsafe { udt_strerror() }
    )
}