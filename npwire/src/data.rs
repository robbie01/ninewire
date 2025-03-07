use std::fmt::{Debug, Display};

use bytestring::ByteString;
use int_enum::IntEnum;
use bytes::Bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntEnum)]
#[repr(u8)]
pub enum TypeId {
    Tversion = 100,
    Rversion = 101,
    Tauth = 102,
    Rauth = 103,
    Tattach = 104,
    Rattach = 105,
    // Terror = 106, /* illegal */
    Rerror = 107,
    Tflush = 108,
    Rflush = 109,
    Twalk = 110,
    Rwalk = 111,
    Topen = 112,
    Ropen = 113,
    Tcreate = 114,
    Rcreate = 115,
    Tread = 116,
    Rread = 117,
    Twrite = 118,
    Rwrite = 119,
    Tclunk = 120,
    Rclunk = 121,
    Tremove = 122,
    Rremove = 123,
    Tstat = 124,
    Rstat = 125,
    Twstat = 126,
    Rwstat = 127
}

/*
spec:

	
size[4] Tversion tag[2] msize[4] version[s]
size[4] Rversion tag[2] msize[4] version[s]
size[4] Tauth tag[2] afid[4] uname[s] aname[s]
size[4] Rauth tag[2] aqid[13]
size[4] Rerror tag[2] ename[s]
size[4] Tflush tag[2] oldtag[2]
size[4] Rflush tag[2]
size[4] Tattach tag[2] fid[4] afid[4] uname[s] aname[s]
size[4] Rattach tag[2] qid[13]
size[4] Twalk tag[2] fid[4] newfid[4] nwname[2] nwname*(wname[s])
size[4] Rwalk tag[2] nwqid[2] nwqid*(wqid[13])
size[4] Topen tag[2] fid[4] mode[1]
size[4] Ropen tag[2] qid[13] iounit[4]
size[4] Tcreate tag[2] fid[4] name[s] perm[4] mode[1]
size[4] Rcreate tag[2] qid[13] iounit[4]
size[4] Tread tag[2] fid[4] offset[8] count[4]
size[4] Rread tag[2] count[4] data[count]
size[4] Twrite tag[2] fid[4] offset[8] count[4] data[count]
size[4] Rwrite tag[2] count[4]
size[4] Tclunk tag[2] fid[4]
size[4] Rclunk tag[2]
size[4] Tremove tag[2] fid[4]
size[4] Rremove tag[2]
size[4] Tstat tag[2] fid[4]
size[4] Rstat tag[2] stat[n]
size[4] Twstat tag[2] fid[4] stat[n]
size[4] Rwstat tag[2] 
 */

pub const RREAD_OVERHEAD: usize = 7;
pub const TWRITE_OVERHEAD: usize = 19;

pub const QTDIR: u8 = 0x80; /* type bit for directories */
pub const QTAPPEND: u8 = 0x40; /* type bit for append only files */
pub const QTEXCL: u8 = 0x20; /* type bit for exclusive use files */
pub const QTAUTH: u8 = 0x08; /* type bit for authentication file */
pub const QTTMP: u8 = 0x04; /* type bit for non-backed-up file */
pub const QTFILE: u8 = 0x00; /* plain file */

pub const DMDIR: u32 = 0x80000000;
pub const DMAPPEND: u32 = 0x40000000;
pub const DMEXCL: u32 = 0x20000000;
pub const DMAUTH: u32 = 0x08000000;
pub const DMTMP: u32 = 0x04000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Qid {
    pub type_: u8,
    pub version: u32,
    pub path: u64
}

#[derive(Debug, Clone)]
pub struct Stat {
    pub type_: u16,
    pub dev: u32,
    pub qid: Qid,
    pub mode: u32,
    pub atime: u32,
    pub mtime: u32,
    pub length: u64,
    pub name: ByteString,
    pub uid: ByteString,
    pub gid: ByteString,
    pub muid: ByteString
}

#[derive(Debug, Clone)]
pub struct Tversion {
    pub msize: u32,
    pub version: ByteString,
}

#[derive(Debug, Clone)]
pub struct Rversion {
    pub msize: u32,
    pub version: ByteString,
}

#[derive(Debug, Clone)]
pub struct Tauth {
    pub afid: u32,
    pub uname: ByteString,
    pub aname: ByteString,
}

#[derive(Debug, Clone, Copy)]
pub struct Rauth {
    pub aqid: Qid,
}

#[derive(Debug, Clone)]
pub struct Rerror {
    pub ename: ByteString
}

impl<E: Display> From<E> for Rerror {
    fn from(value: E) -> Self {
        Self { ename: value.to_string().into() }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tflush {
    pub oldtag: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct Rflush;

#[derive(Debug, Clone)]
pub struct Tattach {
    pub fid: u32,
    pub afid: u32,
    pub uname: ByteString,
    pub aname: ByteString,
}

#[derive(Debug, Clone, Copy)]
pub struct Rattach {
    pub qid: Qid,
}

#[derive(Debug, Clone)]
pub struct Twalk {
    pub fid: u32,
    pub newfid: u32,
    pub wname: Vec<ByteString>,
}

#[derive(Debug, Clone)]
pub struct Rwalk {
    pub wqid: Vec<Qid>,
}

#[derive(Debug, Clone, Copy)]
pub struct Topen {
    pub fid: u32,
    pub mode: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct Ropen {
    pub qid: Qid,
    pub iounit: u32,
}

#[derive(Debug, Clone)]
pub struct Tcreate {
    pub fid: u32,
    pub name: ByteString,
    pub perm: u32,
    pub mode: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct Rcreate {
    pub qid: Qid,
    pub iounit: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Tread {
    pub fid: u32,
    pub offset: u64,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct Rread {
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct Twrite {
    pub fid: u32,
    pub offset: u64,
    pub data: Bytes,
}

#[derive(Debug, Clone, Copy)]
pub struct Rwrite {
    pub count: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Tclunk {
    pub fid: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Rclunk;

#[derive(Debug, Clone, Copy)]
pub struct Tremove {
    pub fid: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Rremove;

#[derive(Debug, Clone, Copy)]
pub struct Tstat {
    pub fid: u32
}

#[derive(Debug, Clone)]
pub struct Rstat {
    pub stat: Stat
}

#[derive(Debug, Clone)]
pub struct Twstat {
    pub fid: u32,
    pub stat: Stat,
}

#[derive(Debug, Clone, Copy)]
pub struct Rwstat;

#[derive(Clone)]
pub enum TMessage {
    Tversion(Tversion),
    Tauth(Tauth),
    Tflush(Tflush),
    Tattach(Tattach),
    Twalk(Twalk),
    Topen(Topen),
    Tcreate(Tcreate),
    Tread(Tread),
    Twrite(Twrite),
    Tclunk(Tclunk),
    Tremove(Tremove),
    Tstat(Tstat),
    Twstat(Twstat),
}

impl Debug for TMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tversion(inner) => Debug::fmt(inner, f),
            Self::Tauth(inner) => Debug::fmt(inner, f),
            Self::Tflush(inner) => Debug::fmt(inner, f),
            Self::Tattach(inner) => Debug::fmt(inner, f),
            Self::Twalk(inner) => Debug::fmt(inner, f),
            Self::Topen(inner) => Debug::fmt(inner, f),
            Self::Tcreate(inner) => Debug::fmt(inner, f),
            Self::Tread(inner) => Debug::fmt(inner, f),
            Self::Twrite(inner) => Debug::fmt(inner, f),
            Self::Tclunk(inner) => Debug::fmt(inner, f),
            Self::Tremove(inner) => Debug::fmt(inner, f),
            Self::Tstat(inner) => Debug::fmt(inner, f),
            Self::Twstat(inner) => Debug::fmt(inner, f),
        }
    }
}

impl From<Tversion> for TMessage {
    fn from(value: Tversion) -> Self {
        Self::Tversion(value)
    }
}

impl From<Tauth> for TMessage {
    fn from(value: Tauth) -> Self {
        Self::Tauth(value)
    }
}

impl From<Tflush> for TMessage {
    fn from(value: Tflush) -> Self {
        Self::Tflush(value)
    }
}

impl From<Tattach> for TMessage {
    fn from(value: Tattach) -> Self {
        Self::Tattach(value)
    }
}

impl From<Twalk> for TMessage {
    fn from(value: Twalk) -> Self {
        Self::Twalk(value)
    }
}

impl From<Topen> for TMessage {
    fn from(value: Topen) -> Self {
        Self::Topen(value)
    }
}

impl From<Tcreate> for TMessage {
    fn from(value: Tcreate) -> Self {
        Self::Tcreate(value)
    }
}

impl From<Tread> for TMessage {
    fn from(value: Tread) -> Self {
        Self::Tread(value)
    }
}

impl From<Twrite> for TMessage {
    fn from(value: Twrite) -> Self {
        Self::Twrite(value)
    }
}

impl From<Tclunk> for TMessage {
    fn from(value: Tclunk) -> Self {
        Self::Tclunk(value)
    }
}

impl From<Tremove> for TMessage {
    fn from(value: Tremove) -> Self {
        Self::Tremove(value)
    }
}

impl From<Tstat> for TMessage {
    fn from(value: Tstat) -> Self {
        Self::Tstat(value)
    }
}

impl From<Twstat> for TMessage {
    fn from(value: Twstat) -> Self {
        Self::Twstat(value)
    }
}

#[derive(Clone)]
pub enum RMessage {
    Rversion(Rversion),
    Rauth(Rauth),
    Rerror(Rerror),
    Rflush(Rflush),
    Rattach(Rattach),
    Rwalk(Rwalk),
    Ropen(Ropen),
    Rcreate(Rcreate),
    Rread(Rread),
    Rwrite(Rwrite),
    Rclunk(Rclunk),
    Rremove(Rremove),
    Rstat(Rstat),
    Rwstat(Rwstat),
}

impl Debug for RMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rversion(inner) => Debug::fmt(inner, f),
            Self::Rauth(inner) => Debug::fmt(inner, f),
            Self::Rerror(inner) => Debug::fmt(inner, f),
            Self::Rflush(inner) => Debug::fmt(inner, f),
            Self::Rattach(inner) => Debug::fmt(inner, f),
            Self::Rwalk(inner) => Debug::fmt(inner, f),
            Self::Ropen(inner) => Debug::fmt(inner, f),
            Self::Rcreate(inner) => Debug::fmt(inner, f),
            Self::Rread(inner) => Debug::fmt(inner, f),
            Self::Rwrite(inner) => Debug::fmt(inner, f),
            Self::Rclunk(inner) => Debug::fmt(inner, f),
            Self::Rremove(inner) => Debug::fmt(inner, f),
            Self::Rstat(inner) => Debug::fmt(inner, f),
            Self::Rwstat(inner) => Debug::fmt(inner, f),
        }
    }
}

impl From<Rversion> for RMessage {
    fn from(value: Rversion) -> Self {
        Self::Rversion(value)
    }
}

impl From<Rauth> for RMessage {
    fn from(value: Rauth) -> Self {
        Self::Rauth(value)
    }
}

impl From<Rerror> for RMessage {
    fn from(value: Rerror) -> Self {
        Self::Rerror(value)
    }
}

impl From<Rflush> for RMessage {
    fn from(value: Rflush) -> Self {
        Self::Rflush(value)
    }
}

impl From<Rattach> for RMessage {
    fn from(value: Rattach) -> Self {
        Self::Rattach(value)
    }
}

impl From<Rwalk> for RMessage {
    fn from(value: Rwalk) -> Self {
        Self::Rwalk(value)
    }
}

impl From<Ropen> for RMessage {
    fn from(value: Ropen) -> Self {
        Self::Ropen(value)
    }
}

impl From<Rcreate> for RMessage {
    fn from(value: Rcreate) -> Self {
        Self::Rcreate(value)
    }
}

impl From<Rread> for RMessage {
    fn from(value: Rread) -> Self {
        Self::Rread(value)
    }
}

impl From<Rwrite> for RMessage {
    fn from(value: Rwrite) -> Self {
        Self::Rwrite(value)
    }
}

impl From<Rclunk> for RMessage {
    fn from(value: Rclunk) -> Self {
        Self::Rclunk(value)
    }
}

impl From<Rremove> for RMessage {
    fn from(value: Rremove) -> Self {
        Self::Rremove(value)
    }
}

impl From<Rstat> for RMessage {
    fn from(value: Rstat) -> Self {
        Self::Rstat(value)
    }
}

impl From<Rwstat> for RMessage {
    fn from(value: Rwstat) -> Self {
        Self::Rwstat(value)
    }
}