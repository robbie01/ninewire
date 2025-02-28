use std::mem;

use npwire::{Rwalk, Twalk};

use super::*;

#[derive(Debug)]
pub struct Directory {
    pub(super) fsys: Arc<FilesystemInner>,
    pub(super) fid: FidHandle
}

impl Filesystem {
    pub async fn mount(&self) -> io::Result<Directory> {
        let dir = Directory {
            fsys: self.0.clone(),
            fid: self.0.get_fid().unwrap()
        };

        let resp = self.0.transact(Tattach {
            fid: dir.fid.fid(),
            afid: !0,
            uname: ByteString::from_static("anonymous"),
            aname: ByteString::new()
        }).await?;

        match resp {
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&ename[..])),
            RMessage::Rattach(Rattach { qid }) => {
                if qid.type_ & QTDIR != QTDIR {
                    Err(io::ErrorKind::NotADirectory.into())
                } else {
                    Ok(dir)
                }                
            },
            _ => Err(io::Error::other("unexpected message type"))
        }
    }
}

impl Directory {
    pub async fn try_clone(&self) -> io::Result<Self> {
        let dir = Directory {
            fsys: self.fsys.clone(),
            fid: self.fsys.get_fid().unwrap()
        };

        let resp = self.fsys.transact(Twalk {
            fid: self.fid.fid(),
            newfid: dir.fid.fid(),
            wname: Vec::new()
        }).await?;

        match resp {
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&ename[..])),
            RMessage::Rwalk(Rwalk { wqid }) => {
                if !wqid.is_empty() {
                    return Err(io::Error::other("idek bro"));
                }

                Ok(dir)       
            },
            _ => Err(io::Error::other("unexpected message type"))
        }
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let fsys = self.fsys.clone();
        let fid = mem::take(&mut self.fid);
        
        tokio::spawn(async move {
            let _ = fsys.clunk(fid).await;
        });
    }
}