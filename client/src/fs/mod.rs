use std::{collections::BTreeMap, io, mem, sync::Arc};

use bytes::BytesMut;
use bytestring::ByteString;
use npwire::{deserialize_r, RMessage, TMessage, Tversion};
use parking_lot::Mutex;

mod transact;
mod dir;
mod file;
mod readdir;

pub use dir::*;
pub use readdir::*;
pub use file::*;
use tokio::sync::oneshot;
use tracing::trace;
use transport::{RecvHalf, SendHalf};
use util::fidpool::{FidHandle, FidPool};

const MAX_MESSAGE_SIZE: u32 = 1280 - 64 - 8 - 16;

// todo: AtomicBool flag in case recv task dies
#[derive(Debug)]
pub(crate) struct FilesystemInner {
    transport: tokio::sync::Mutex<SendHalf>,
    inflight: Mutex<BTreeMap<u16, oneshot::Sender<RMessage>>>,
    fids: FidPool,
    maxlen: usize
}

#[derive(Debug)]
pub struct Filesystem {
    fsys: Arc<FilesystemInner>
}

impl FilesystemInner {
    fn get_fid(&self) -> Option<FidHandle> {
        self.fids.get()
    }
}

impl Filesystem {
    pub async fn new((transport, mut rcv): (SendHalf, RecvHalf)) -> io::Result<Self> {
        let mut inner = FilesystemInner {
            transport: transport.into(),
            inflight: Default::default(),
            fids: FidPool::new(),
            maxlen: 0
        };

        let ver = TMessage::Tversion(Tversion {
            msize: MAX_MESSAGE_SIZE,
            version: ByteString::from_static("9P2000")
        });
        inner.transport.try_lock().unwrap().send(ver.serialize(!0).unwrap()).await?;
        trace!("sent request {ver:?}");

        let mut ver = BytesMut::zeroed(MAX_MESSAGE_SIZE as usize);
        let n = rcv.recv(&mut ver).await?;
        ver.truncate(n);
        
        let (_, ver) = deserialize_r(ver.freeze()).map_err(io::Error::other)?;
        trace!("received reply {ver:?}");
        let RMessage::Rversion(ver) = ver else {
            return Err(io::Error::other("invalid version response"))
        };

        if ver.version != "9P2000" {
            return Err(io::Error::other("protocol not supported"))
        }
        inner.maxlen = ver.msize as usize;

        let inner = Arc::new(inner);
        let inner2 = inner.clone();

        let _handle = tokio::spawn(async move {
            let mut resp = BytesMut::zeroed(MAX_MESSAGE_SIZE as usize);
            loop {
                let n = rcv.recv(&mut resp).await?;
                let mut resp = mem::replace(&mut resp, BytesMut::zeroed(MAX_MESSAGE_SIZE as usize));
                resp.truncate(n);
                if let Ok((tag, resp)) = deserialize_r(resp.freeze()) {
                    trace!("received reply with tag {tag}, {resp:?}");

                    if let Some(reply_to) = inner2.inflight.lock().remove(&tag) {
                        let _ = reply_to.send(resp);
                    }
                }
            }

            #[allow(unreachable_code)]
            Ok::<_, anyhow::Error>(())
        });

        tokio::spawn(async move {
            let res = _handle.await.unwrap();
            if let Err(e) = res {
                println!("fatal error: {e:?}")
            }
        });

        Ok(Self {
            fsys: inner
        })
    }
}