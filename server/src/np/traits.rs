use std::{fmt::Display, future::Future, hash::Hash};

use bytes::Bytes;

use npwire::{Qid, Stat};

pub trait Fid: Copy + Send + Sync + Eq + Hash {
    fn is_nofid(self) -> bool;
}

impl Fid for u32 {
    fn is_nofid(self) -> bool {
        self == !0
    }
}

pub trait Serve<FID: Fid>: Send + Sync {
    type Error: Display;

    fn auth(&self, afid: FID, uname: &str, aname: &str) -> impl Future<Output = Result<Qid, Self::Error>> + Send;
    fn attach(&self, fid: FID, afid: FID, uname: &str, aname: &str) -> impl Future<Output = Result<Qid, Self::Error>> + Send;
    fn walk(&self, fid: FID, newfid: FID, wname: Vec<&str>) -> impl Future<Output = Result<impl IntoIterator<Item = Qid>, Self::Error>> + Send;
    fn open(&self, fid: FID, mode: u8) -> impl Future<Output = Result<(Qid, u32), Self::Error>> + Send;
    fn create(&self, fid: FID, name: &str, perm: u32, mode: u8) -> impl Future<Output = Result<(Qid, u32), Self::Error>> + Send;
    fn read(&self, fid: FID, offset: u64, count: u32) -> impl Future<Output = Result<Bytes, Self::Error>> + Send;
    fn write(&self, fid: FID, offset: u64, data: &[u8]) -> impl Future<Output = Result<u32, Self::Error>> + Send;
    fn clunk(&self, fid: FID) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn remove(&self, fid: FID) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn stat(&self, fid: FID) -> impl Future<Output = Result<Stat, Self::Error>> + Send;
    fn wstat(&self, fid: FID, stat: Stat) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn clunk_where(&self, matcher: impl Fn(FID) -> bool + Send) -> impl Future<Output = ()> + Send;
}