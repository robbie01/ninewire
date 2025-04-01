use std::{collections::HashMap, pin::pin, sync::Arc};

use anyhow::bail;
use bytestring::ByteString;
use futures::{SinkExt, StreamExt as _, TryStreamExt};
use npwire::{deserialize_r, RMessage, Rerror, TMessage, Tversion, Twrite, TWRITE_OVERHEAD};
use pool::TagPool;
use tokio::{io::{AsyncRead, AsyncWrite}, sync::{mpsc, oneshot}};
use tokio_util::codec::LengthDelimitedCodec;

mod pool;
mod transact;
mod dir;
mod file;
mod readdir;

pub use dir::*;
pub use readdir::*;
pub use file::*;
use tracing::trace;
use util::{fidpool::{FidHandle, FidPool}, polymur};

#[derive(Debug)]
struct Request {
    message: TMessage,
    reply_to: oneshot::Sender<RMessage>
}

const MAX_MESSAGE_SIZE: u32 = 65535 - 16;

#[derive(Debug)]
pub(crate) struct FilesystemInner {
    sender: mpsc::Sender<Request>,
    fids: FidPool
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
    pub fn new(stream: impl AsyncRead + AsyncWrite + Send + 'static) -> Self {
        let (sender, mut rcv) = mpsc::channel::<Request>(1);

        let _handle = tokio::spawn(async move {
            let stream = pin!(stream);
            let mut framed = LengthDelimitedCodec::builder()
                .little_endian()
                .length_field_type::<u32>()
                .length_adjustment(-4)
                .new_framed(stream);

            let ver = TMessage::Tversion(Tversion {
                msize: MAX_MESSAGE_SIZE,
                version: ByteString::from_static("9P2000")
            });
            framed.send(ver.serialize(!0).unwrap()).await?;
            trace!("sent request {ver:?}");

            let Some(ver) = framed.try_next().await? else {
                return Ok(())
            };
            
            let (_, ver) = deserialize_r(ver.freeze())?;
            trace!("received reply {ver:?}");
            let RMessage::Rversion(ver) = ver else {
                bail!("invalid version response")
            };

            if ver.version != "9P2000" {
                bail!("protocol not supported")
            }
            let maxlen = ver.msize.checked_sub(4).unwrap() as usize;
            framed.codec_mut().set_max_frame_length(maxlen);

            let mut tags = TagPool::default();

            let mut replies = HashMap::<_, _, polymur::RandomState>::default();

            loop {
                tokio::select! {
                    Some(mut req) = rcv.recv() => {
                        let tag = tags.get().unwrap();

                        replies.insert(tag, req.reply_to);

                        // Bound writes by the max message size
                        if let TMessage::Twrite(Twrite { ref mut data, .. }) = req.message {
                            data.truncate(maxlen - TWRITE_OVERHEAD);
                        }

                        let data = req.message
                            .serialize(tag)
                            .unwrap_or_else(|e| Rerror::from(e).serialize(tag).unwrap());

                        framed.send(data).await?;
                        trace!("sent request with tag {tag}, {:?}", req.message);
                    },
                    Some(resp) = framed.next() => {
                        if let Ok((tag, resp)) = deserialize_r(resp?.freeze()) {
                            trace!("received reply with tag {tag}, {resp:?}");

                            if let Some(reply_to) = replies.remove(&tag) {
                                tags.put(tag);
                                let _ = reply_to.send(resp);
                            }
                        }
                    },
                    else => break
                }
            }

            Ok(())
        });

        Self {
            fsys: Arc::new(FilesystemInner {
                sender,
                fids: FidPool::default()
            })
        }
    }
}