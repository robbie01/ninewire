use std::io;

use bytestring::ByteString;
use npwire::{RMessage, Rclunk, Rerror, Ropen, Rstat, Rwalk, TMessage, Tclunk, Topen, Tstat, Twalk};
use tokio::sync::oneshot;
use util::fidpool::FidHandle;

use super::{FilesystemInner, Request};

impl FilesystemInner {
    pub(super) async fn transact(&self, message: impl Into<TMessage>) -> io::Result<RMessage> {
        let (reply_to, rcv) = oneshot::channel();
        self.sender.send(Request { message: message.into(), reply_to }).await.map_err(|_| io::ErrorKind::BrokenPipe)?;
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
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&ename[..])),
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
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&ename[..])),
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
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&ename[..])),
            RMessage::Rclunk(Rclunk) => Ok(()),
            _ => Err(io::Error::other("unexpected message type"))
        }
    }
}