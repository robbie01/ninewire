use std::{collections::HashMap, convert::Infallible, fmt::Debug, io, net::Ipv4Addr, path::PathBuf, sync::Arc};

use anyhow::bail;
use bytestring::ByteString;
use np::traits;
use tokio::net::TcpListener;

type Atom = Arc<str>;

mod np;
mod res;

type ShareTable = HashMap<Arc<str>, PathBuf>;

#[derive(Debug)]
struct HandlerInner {
    shares: ShareTable
}

#[derive(Debug)]
struct Handler {
    inner: Arc<HandlerInner>
}

#[derive(Debug)]
struct Session {
    uname: ByteString
}

impl Handler {
    fn new(shares: ShareTable) -> Self {
        Self { inner: Arc::new(HandlerInner { shares }) }
    }
}

impl traits::Serve2 for Handler {
    type Error = anyhow::Error;
    type PathResource = res::path::PathResource;
    type OpenResource = res::open::OpenResource;

    async fn auth(&self, _uname: &str, _aname: &str) -> Result<Self::OpenResource, Self::Error> {
        bail!("Function not implemented");
    }

    async fn attach(&self, ares: Option<&Self::OpenResource>, uname: &str, aname: &str) -> Result<Self::PathResource, Self::Error> {
        if ares.is_some() {
            bail!("Permission denied");
        }

        if !aname.is_empty() {
            bail!("No such file or directory");
        }

        let session = Arc::new(Session {
            uname: uname.into()
        });

        Ok(res::path::PathResource::root(self, session))
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
