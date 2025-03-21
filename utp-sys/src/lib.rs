#![allow(non_camel_case_types)]

cfg_if::cfg_if! {
    if #[cfg(windows)] {
        use windows_sys::Win32::Networking::WinSock::{SOCKADDR as sockaddr, socklen_t};
        pub use windows_sys::Win32::Networking::WinSock::SD_SEND as SHUT_WR;
    } else {
        use libc::{sockaddr, socklen_t};
        pub use libc::SHUT_WR;
    }
}

pub const UTP_IOV_MAX: libc::c_int = 1024;
pub const UTP_UDP_DONTFRAG: libc::c_int = 2;
pub const UTP_STATE_CONNECT: libc::c_int = 1;
pub const UTP_STATE_WRITABLE: libc::c_int = 2;
pub const UTP_STATE_EOF: libc::c_int = 3;
pub const UTP_STATE_DESTROYING: libc::c_int = 4;
pub const UTP_ECONNREFUSED: libc::c_int = 0;
pub const UTP_ECONNRESET: libc::c_int = 1;
pub const UTP_ETIMEDOUT: libc::c_int = 2;
pub const UTP_ON_FIREWALL: libc::c_int = 0;
pub const UTP_ON_ACCEPT: libc::c_int = 1;
pub const UTP_ON_CONNECT: libc::c_int = 2;
pub const UTP_ON_ERROR: libc::c_int = 3;
pub const UTP_ON_READ: libc::c_int = 4;
pub const UTP_ON_OVERHEAD_STATISTICS: libc::c_int = 5;
pub const UTP_ON_STATE_CHANGE: libc::c_int = 6;
pub const UTP_GET_READ_BUFFER_SIZE: libc::c_int = 7;
pub const UTP_ON_DELAY_SAMPLE: libc::c_int = 8;
pub const UTP_GET_UDP_MTU: libc::c_int = 9;
pub const UTP_GET_UDP_OVERHEAD: libc::c_int = 10;
pub const UTP_GET_MILLISECONDS: libc::c_int = 11;
pub const UTP_GET_MICROSECONDS: libc::c_int = 12;
pub const UTP_GET_RANDOM: libc::c_int = 13;
pub const UTP_LOG: libc::c_int = 14;
pub const UTP_SENDTO: libc::c_int = 15;
pub const UTP_LOG_NORMAL: libc::c_int = 16;
pub const UTP_LOG_MTU: libc::c_int = 17;
pub const UTP_LOG_DEBUG: libc::c_int = 18;
pub const UTP_SNDBUF: libc::c_int = 19;
pub const UTP_RCVBUF: libc::c_int = 20;
pub const UTP_TARGET_DELAY: libc::c_int = 21;
pub const UTP_ARRAY_SIZE: libc::c_int = 22;

#[repr(C)]
pub struct utp_socket {
    _unused: [u8; 0],
}

#[repr(C)]
pub struct utp_context {
    _unused: [u8; 0],
}

unsafe extern "C" {
    pub static utp_state_names: [*const libc::c_char; 4];
    pub static utp_error_code_names: [*const libc::c_char; 3];
    pub static utp_callback_names: [*const libc::c_char; 16];
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct utp_callback_arguments {
    pub context: *mut utp_context,
    pub socket: *mut utp_socket,
    pub len: usize,
    pub flags: u32,
    pub callback_type: libc::c_int,
    pub buf: *const u8,
    pub u0: utp_callback_arguments_u0,
    pub u1: utp_callback_arguments_u1,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub union utp_callback_arguments_u0 {
    pub address: *const sockaddr,
    pub send: libc::c_int,
    pub sample_ms: libc::c_int,
    pub error_code: libc::c_int,
    pub state: libc::c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union utp_callback_arguments_u1 {
    pub address_len: socklen_t,
    pub type_: libc::c_int,
}

pub type utp_callback_t = unsafe extern "C" fn(args: *mut utp_callback_arguments) -> u64;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct utp_context_stats {
    pub _nraw_recv: [u32; 5],
    pub _nraw_send: [u32; 5],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct utp_socket_stats {
    pub nbytes_recv: u64,
    pub nbytes_xmit: u64,
    pub rexmit: u32,
    pub fastrexmit: u32,
    pub nxmit: u32,
    pub nrecv: u32,
    pub nduprecv: u32,
    pub mtu_guess: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct utp_iovec {
    pub iov_base: *mut libc::c_void,
    pub iov_len: usize,
}

unsafe extern "C" {
    pub fn utp_init(version: libc::c_int) -> *mut utp_context;
    pub fn utp_destroy(ctx: *mut utp_context);
    pub fn utp_set_callback(
        ctx: *mut utp_context,
        callback_name: libc::c_int,
        proc_: utp_callback_t,
    );
    pub fn utp_context_set_userdata(
        ctx: *mut utp_context,
        userdata: *mut libc::c_void,
    ) -> *mut libc::c_void;
    pub fn utp_context_get_userdata(ctx: *mut utp_context) -> *mut libc::c_void;
    pub fn utp_context_set_option(
        ctx: *mut utp_context,
        opt: libc::c_int,
        val: libc::c_int,
    ) -> libc::c_int;
    pub fn utp_context_get_option(
        ctx: *mut utp_context,
        opt: libc::c_int,
    ) -> libc::c_int;
    pub fn utp_process_udp(
        ctx: *mut utp_context,
        buf: *const u8,
        len: usize,
        to: *const sockaddr,
        tolen: socklen_t,
    ) -> libc::c_int;
    pub fn utp_process_icmp_error(
        ctx: *mut utp_context,
        buffer: *const u8,
        len: usize,
        to: *const sockaddr,
        tolen: socklen_t,
    ) -> libc::c_int;
    pub fn utp_process_icmp_fragmentation(
        ctx: *mut utp_context,
        buffer: *const u8,
        len: usize,
        to: *const sockaddr,
        tolen: socklen_t,
        next_hop_mtu: u16,
    ) -> libc::c_int;
    pub fn utp_check_timeouts(ctx: *mut utp_context);
    pub fn utp_issue_deferred_acks(ctx: *mut utp_context);
    pub fn utp_get_context_stats(ctx: *mut utp_context) -> *mut utp_context_stats;
    pub fn utp_create_socket(ctx: *mut utp_context) -> *mut utp_socket;
    pub fn utp_set_userdata(
        s: *mut utp_socket,
        userdata: *mut libc::c_void,
    ) -> *mut libc::c_void;
    pub fn utp_get_userdata(s: *mut utp_socket) -> *mut libc::c_void;
    pub fn utp_setsockopt(
        s: *mut utp_socket,
        opt: libc::c_int,
        val: libc::c_int,
    ) -> libc::c_int;
    pub fn utp_getsockopt(s: *mut utp_socket, opt: libc::c_int) -> libc::c_int;
    pub fn utp_connect(
        s: *mut utp_socket,
        to: *const sockaddr,
        tolen: socklen_t,
    ) -> libc::c_int;
    pub fn utp_write(s: *mut utp_socket, buf: *mut libc::c_void, count: usize) -> isize;
    pub fn utp_writev(s: *mut utp_socket, iovec: *mut utp_iovec, num_iovecs: usize) -> isize;
    pub fn utp_getpeername(
        s: *mut utp_socket,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t,
    ) -> libc::c_int;
    pub fn utp_read_drained(s: *mut utp_socket);
    pub fn utp_get_delays(
        s: *mut utp_socket,
        ours: *mut u32,
        theirs: *mut u32,
        age: *mut u32,
    ) -> libc::c_int;
    pub fn utp_get_stats(s: *mut utp_socket) -> *mut utp_socket_stats;
    pub fn utp_get_context(s: *mut utp_socket) -> *mut utp_context;
    pub fn utp_shutdown(s: *mut utp_socket, how: libc::c_int);
    pub fn utp_close(s: *mut utp_socket);
}
