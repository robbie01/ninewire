use std::{collections::HashMap, convert::Infallible, io, net::Ipv6Addr, path::PathBuf, sync::{atomic::{AtomicU64, Ordering}, Arc}};

use anyhow::bail;
use bytestring::ByteString;
use np::traits;
use tokio::net::UdpSocket;

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


#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<Infallible> {
    console_subscriber::init();

    let sock = Arc::new(UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 64444)).await?);
    let listener = utp::Endpoint::new(sock, 16);

    np::serve_mux(Arc::new(Handler::new([
        ("forfun".into(), "forfun".into()),
        ("ff2".into(), "forfun".into())
    ].into_iter().collect())), listener).await
}
