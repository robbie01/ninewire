use std::{fmt::Display, future::Future, hash::Hash, sync::Arc};

use bytes::Bytes;

use npwire::{Qid, Stat};

pub trait Fid: Copy + Send + Sync + Eq + Hash + 'static {
    fn is_nofid(self) -> bool;
}

impl Fid for u32 {
    fn is_nofid(self) -> bool {
        self == !0
    }
}

pub trait Serve: Send + Sync + 'static {
    type Fid: Fid;
    type Error: Display;

    fn auth(&self, afid: Self::Fid, uname: &str, aname: &str) -> impl Future<Output = Result<Qid, Self::Error>> + Send;
    fn attach(&self, fid: Self::Fid, afid: Self::Fid, uname: &str, aname: &str) -> impl Future<Output = Result<Qid, Self::Error>> + Send;
    fn walk(&self, fid: Self::Fid, newfid: Self::Fid, wname: &[&str]) -> impl Future<Output = Result<impl IntoIterator<Item = Qid>, Self::Error>> + Send;
    fn open(&self, fid: Self::Fid, mode: u8) -> impl Future<Output = Result<(Qid, u32), Self::Error>> + Send;
    fn create(&self, fid: Self::Fid, name: &str, perm: u32, mode: u8) -> impl Future<Output = Result<(Qid, u32), Self::Error>> + Send;
    fn read(&self, fid: Self::Fid, offset: u64, count: u32) -> impl Future<Output = Result<Bytes, Self::Error>> + Send;
    fn write(&self, fid: Self::Fid, offset: u64, data: &[u8]) -> impl Future<Output = Result<u32, Self::Error>> + Send;
    fn clunk(&self, fid: Self::Fid) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn remove(&self, fid: Self::Fid) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn stat(&self, fid: Self::Fid) -> impl Future<Output = Result<Stat, Self::Error>> + Send;
    fn wstat(&self, fid: Self::Fid, stat: Stat) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn clunk_where(&self, matcher: impl FnMut(Self::Fid) -> bool + Send) -> impl Future<Output = ()> + Send;
}

impl<S: Serve> Serve for Arc<S> {
    type Fid = S::Fid;
    type Error = S::Error;

    fn auth(&self, afid: Self::Fid, uname: &str, aname: &str) -> impl Future<Output = Result<Qid, Self::Error>> + Send {
        S::auth(self, afid, uname, aname)
    }

    fn attach(&self, fid: Self::Fid, afid: Self::Fid, uname: &str, aname: &str) -> impl Future<Output = Result<Qid, Self::Error>> + Send {
        S::attach(self, fid, afid, uname, aname)
    }

    fn walk(&self, fid: Self::Fid, newfid: Self::Fid, wname: &[&str]) -> impl Future<Output = Result<impl IntoIterator<Item = Qid>, Self::Error>> + Send {
        S::walk(self, fid, newfid, wname)
    }

    fn open(&self, fid: Self::Fid, mode: u8) -> impl Future<Output = Result<(Qid, u32), Self::Error>> + Send {
        S::open(self, fid, mode)
    }

    fn create(&self, fid: Self::Fid, name: &str, perm: u32, mode: u8) -> impl Future<Output = Result<(Qid, u32), Self::Error>> + Send {
        S::create(self, fid, name, perm, mode)
    }

    fn read(&self, fid: Self::Fid, offset: u64, count: u32) -> impl Future<Output = Result<Bytes, Self::Error>> + Send {
        S::read(self, fid, offset, count)
    }

    fn write(&self, fid: Self::Fid, offset: u64, data: &[u8]) -> impl Future<Output = Result<u32, Self::Error>> + Send {
        S::write(self, fid, offset, data)
    }

    fn clunk(&self, fid: Self::Fid) -> impl Future<Output = Result<(), Self::Error>> + Send {
        S::clunk(self, fid)
    }

    fn remove(&self, fid: Self::Fid) -> impl Future<Output = Result<(), Self::Error>> + Send {
        S::remove(self, fid)
    }

    fn stat(&self, fid: Self::Fid) -> impl Future<Output = Result<Stat, Self::Error>> + Send {
        S::stat(self, fid)
    }

    fn wstat(&self, fid: Self::Fid, stat: Stat) -> impl Future<Output = Result<(), Self::Error>> + Send {
        S::wstat(self, fid, stat)
    }

    fn clunk_where(&self, matcher: impl FnMut(Self::Fid) -> bool + Send) -> impl Future<Output = ()> + Send {
        S::clunk_where(self, matcher)
    }
}