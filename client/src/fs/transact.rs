use std::io;

use bytestring::ByteString;
use npwire::{RMessage, Rclunk, Rerror, Ropen, Rstat, Rwalk, TMessage, Tclunk, Topen, Tstat, Twalk, Twrite, TWRITE_OVERHEAD};
use tokio::sync::oneshot;
use tracing::trace;
use util::fidpool::FidHandle;

use super::FilesystemInner;

impl FilesystemInner {
    pub(super) async fn transact(&self, message: impl Into<TMessage>) -> io::Result<RMessage> {
        let mut message = message.into();
        
        let (tag, rcv) = {
            let mut inflight = self.inflight.lock();
            let mut tag = 0;
            while inflight.contains_key(&tag) {
                // todo: prevent tags from equaling NOTAG (!0) and wait if the queue is full
                tag = tag.checked_add(1).unwrap();
            }

            let (reply_to, rcv) = oneshot::channel();
            inflight.insert(tag, reply_to);
            (tag, rcv)
        };

        // Bound writes by the max message size
        if let TMessage::Twrite(Twrite { ref mut data, .. }) = message {
            data.truncate(self.maxlen - TWRITE_OVERHEAD);
        }

        let data = message
            .serialize(tag)
            .unwrap_or_else(|e| Rerror::from(e).serialize(tag).unwrap());

        self.transport.send(data).await?;
        trace!("sent request with tag {tag}, {:?}", message);

        rcv.await.map_err(|_| io::ErrorKind::UnexpectedEof.into())
    }

    pub(super) async fn stat(&self, fid: &FidHandle) -> io::Result<npwire::Stat> {
        assert!(fid.is_of(&self.fids));

        let resp = self.transact(Tstat {
            fid: fid.fid()
        }).await?;

        match resp {
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&ename[..])),
            RMessage::Rstat(Rstat { stat }) => Ok(stat),
            _ => Err(io::Error::other("unexpected message type"))
        }
    }

    pub(super) async fn open(&self, fid: &FidHandle) -> io::Result<npwire::Qid> {
        let resp = self.transact(Topen {
            fid: fid.fid(),
            mode: 0
        }).await?;

        match resp {
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&*ename)),
            RMessage::Ropen(Ropen { qid, iounit: _ }) => Ok(qid),
            _ => Err(io::Error::other("unexpected message type"))
        }
    }

    pub(super) async fn walk(&self, fid: &FidHandle, newfid: &FidHandle, wname: Vec<ByteString>) -> io::Result<Vec<npwire::Qid>> {
        let resp = self.transact(Twalk {
            fid: fid.fid(),
            newfid: newfid.fid(),
            wname
        }).await?;

        match resp {
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&*ename)),
            RMessage::Rwalk(Rwalk { wqid }) => Ok(wqid),
            _ => Err(io::Error::other("unexpected message type"))
        }
    }

    pub(super) async fn clunk(&self, fid: FidHandle) -> io::Result<()> {
        assert!(fid.is_of(&self.fids));

        let resp = self.transact(Tclunk {
            fid: fid.fid()
        }).await?;

        match resp {
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&*ename)),
            RMessage::Rclunk(Rclunk) => Ok(()),
            _ => Err(io::Error::other("unexpected message type"))
        }
    }
}