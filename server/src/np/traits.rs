use std::{fmt::Display, future::Future};

use bytes::Bytes;

use npwire::{Qid, Stat};

pub trait Resource {
    type Error: Display;

    fn qid(&self) -> Qid;
    fn remove(self) -> impl Future<Output = Result<(), Self::Error>>;
    fn stat(&self) -> impl Future<Output = Result<Stat, Self::Error>>;
    fn wstat(&self, stat: Stat) -> impl Future<Output = Result<(), Self::Error>>;
}

pub trait PathResource: Resource + Sized {
    type OpenResource: OpenResource;

    fn walk(&self, wname: &[&str]) -> impl Future<Output = Result<(Vec<Qid>, Option<Self>), Self::Error>>;
    fn open(&self, mode: u8) -> impl Future<Output = Result<Self::OpenResource, Self::Error>>;
    fn create(&self, name: &str, perm: u32, mode: u8) -> impl Future<Output = Result<Self::OpenResource, Self::Error>>;
}

pub trait OpenResource: Resource {
    fn read(&self, offset: u64, count: u32) -> impl Future<Output = Result<Bytes, Self::Error>>;
    fn write(&self, offset: u64, data: &[u8]) -> impl Future<Output = Result<u32, Self::Error>>;
}

pub trait Serve: 'static {
    type Error: Display;

    type PathResource: PathResource<Error = Self::Error, OpenResource = Self::OpenResource>;
    type OpenResource: OpenResource<Error = Self::Error>;

    fn auth(&self, uname: &str, aname: &str) -> impl Future<Output = Result<Self::OpenResource, Self::Error>>;
    fn attach(&self, ares: Option<&Self::OpenResource>, uname: &str, aname: &str) -> impl Future<Output = Result<Self::PathResource, Self::Error>>;
}