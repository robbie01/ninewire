#![deny(unsafe_code)]

use std::{collections::BTreeMap, path::PathBuf, pin::pin, rc::Rc, sync::{Arc, atomic::{AtomicU64, Ordering}}};

use anyhow::bail;
use bytestring::ByteString;
use compio::net::TcpListener;
use futures::StreamExt;
use np::traits;
use transport::compio::PlainTransport;

mod np;
mod res;

type ShareTable = BTreeMap<Arc<str>, PathBuf>;

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

#[compio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let listener = TcpListener::bind("127.0.0.1:8998").await?;

    let incoming = listener.incoming().map(|r| r.and_then(|s| {
        let a = s.peer_addr()?;
        Ok((PlainTransport::new(s), a))
    }));

    np::serve_mux(Rc::new(Handler::new([
        ("forfun".into(), "forfun".into()),
        ("ff2".into(), "forfun".into()),
        ("home".into(), "/Users/robbie".into())
    ].into_iter().collect())), pin!(incoming)).await?;
    
    Ok(())
}
