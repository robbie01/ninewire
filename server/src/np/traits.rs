use std::{fmt::Display, future::Future, io, net::SocketAddr};

use bytes::Bytes;

use npwire::{Qid, Stat};
use tokio::{io::{AsyncRead, AsyncWrite}, net::{TcpListener, TcpStream}};

pub trait Resource: Send {
    type Error: Display;

    fn qid(&self) -> Qid;
    fn remove(self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn stat(&self) -> impl Future<Output = Result<Stat, Self::Error>> + Send;
    fn wstat(&self, stat: Stat) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub trait PathResource: Resource + Sized + Send + Sync {
    type OpenResource: OpenResource;

    fn walk(&self, wname: &[&str]) -> impl Future<Output = Result<(Vec<Qid>, Option<Self>), Self::Error>> + Send;
    fn open(&self, mode: u8) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + Send;
    fn create(&self, name: &str, perm: u32, mode: u8) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + Send;
}

pub trait OpenResource: Resource + Send + Sync {
    fn read(&self, offset: u64, count: u32) -> impl Future<Output = Result<Bytes, Self::Error>> + Send;
    fn write(&self, offset: u64, data: &[u8]) -> impl Future<Output = Result<u32, Self::Error>> + Send;
}

pub trait Serve: Send + Sync + 'static {
    type Error: Display;

    type PathResource: PathResource<Error = Self::Error, OpenResource = Self::OpenResource>;
    type OpenResource: OpenResource<Error = Self::Error>;

    fn auth(&self, uname: &str, aname: &str) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + Send;
    fn attach(&self, ares: Option<&Self::OpenResource>, uname: &str, aname: &str) -> impl Future<Output = Result<Self::PathResource, Self::Error>> + Send;
}

pub trait Listener {
    type Io: AsyncRead + AsyncWrite;
    type Addr;

    async fn accept(&mut self) -> io::Result<(Self::Io, Self::Addr)>;
}

impl Listener for TcpListener {
    type Io = TcpStream;
    type Addr = SocketAddr;

     fn accept(&mut self) -> impl Future<Output = io::Result<(Self::Io, Self::Addr)>> {
        TcpListener::accept(&*self)
    }
}