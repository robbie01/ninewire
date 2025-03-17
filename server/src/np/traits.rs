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

pub trait Resource: Send {
    type Error: Display;

    fn qid(&self) -> Qid;
    fn remove(self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn stat(&self) -> impl Future<Output = Result<Stat, Self::Error>> + Send;
    fn wstat(&self, stat: Stat) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub trait PathResource: Resource + Sized + Send {
    type OpenResource: OpenResource;

    fn walk(&self, wname: &[&str]) -> impl Future<Output = Result<(Vec<Qid>, Self), Self::Error>> + Send;
    fn open(&mut self, mode: u8) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + Send;
    fn create(&self, name: &str, perm: u32, mode: u8) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + Send;
}

pub trait OpenResource: Resource + Send {
    fn read(&self, offset: u64, count: u32) -> impl Future<Output = Result<Bytes, Self::Error>> + Send;
    fn write(&self, offset: u64, data: &[u8]) -> impl Future<Output = Result<u32, Self::Error>> + Send;
}

pub trait Serve2: Send + Sync + 'static {
    type Error: Display;

    type PathResource<'a>: PathResource<Error = Self::Error, OpenResource = Self::OpenResource<'a>> + 'a;
    type OpenResource<'a>: OpenResource<Error = Self::Error> + 'a;

    fn auth<'a>(&'a self, uname: &str, aname: &str) -> impl Future<Output = Result<Self::OpenResource<'a>, Self::Error>>;
    fn attach<'a>(&'a self, ares: Option<&Self::OpenResource<'a>>, uname: &str, aname: &str) -> impl Future<Output = Result<Self::PathResource<'a>, Self::Error>>;
}