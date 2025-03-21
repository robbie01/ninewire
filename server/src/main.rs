use std::{collections::HashMap, convert::Infallible, fmt::Debug, io, net::{Ipv4Addr, SocketAddrV6}, path::PathBuf, sync::{atomic::{AtomicU64, Ordering}, Arc}, task::{ready, Context, Poll}};

use anyhow::bail;
use bytestring::ByteString;
use libutp_rs2::{Transport, UtpContext, UtpStream};
use np::traits;
use tokio::net::TcpListener;
use tokio_util::{net::Listener, sync::ReusableBoxFuture};

mod np;
mod res;

type ShareTable = HashMap<Arc<str>, PathBuf>;

#[derive(Debug)]
struct Config {
    shares: ShareTable
}

#[derive(Debug)]
struct Handler {
    session_ctr: AtomicU64,
    inner: Arc<Config>
}

#[derive(Debug)]
struct Session {
    #[allow(unused)]
    id: u64,
    uname: ByteString
}

impl Handler {
    fn new(shares: ShareTable) -> Self {
        Self { 
            session_ctr: AtomicU64::new(1),
            inner: Arc::new(Config { shares })
        }
    }
}

impl traits::Serve for Handler {
    type Error = anyhow::Error;
    type PathResource = res::path::PathResource;
    type OpenResource = res::open::OpenResource;

    async fn auth(&self, _uname: &str, _aname: &str) -> Result<Self::OpenResource, Self::Error> {
        bail!("Function not implemented");
    }

    async fn attach(&self, ares: Option<&Self::OpenResource>, uname: &str, aname: &str) -> Result<Self::PathResource, Self::Error> {
        if ares.is_some() {
            bail!("permission denied");
        }

        if !aname.is_empty() {
            bail!("No such file or directory");
        }

        let session = Arc::new(Session {
            id: self.session_ctr.fetch_add(1, Ordering::Relaxed),
            uname: uname.into()
        });

        Ok(res::path::PathResource::root(self, session))
    }
}

struct UtpListener<T> {
    ctx: Arc<UtpContext<T>>,
    accept: ReusableBoxFuture<'static, anyhow::Result<UtpStream<T>>>
}

impl<T: Transport> UtpListener<T> {
    pub fn new(ctx: Arc<UtpContext<T>>) -> Self {
        Self {
            accept: ReusableBoxFuture::new({
                let ctx = ctx.clone();
                async move { ctx.accept().await }
            }),
            ctx
        }
    }
}

impl<T: Transport> Listener for UtpListener<T> {
    type Addr = SocketAddrV6;
    type Io = UtpStream<T>;

    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<(Self::Io, Self::Addr)>> {
        let con = ready!(self.accept.poll(cx)).map_err(io::Error::other)?;
        con.

        todo!()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<Infallible> {
    console_subscriber::init();

    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 64444)).await?;

    np::serve_mux(Arc::new(Handler::new([
        ("forfun".into(), "forfun".into()),
        ("ff2".into(), "forfun".into())
    ].into_iter().collect())), listener).await
}
