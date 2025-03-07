use std::{io, mem, sync::Arc};

use npwire::{RMessage, Rattach, Rerror, Tattach, QTDIR};
use util::fidpool::FidHandle;

use super::{Filesystem, FilesystemInner};

#[derive(Debug)]
pub struct Directory {
    pub(super) fsys: Arc<FilesystemInner>,
    pub(super) fid: FidHandle
}

impl Filesystem {
    pub async fn attach(&self, uname: &str, aname: &str) -> io::Result<Directory> {
        let dir = Directory {
            fsys: self.fsys.clone(),
            fid: self.fsys.get_fid().unwrap()
        };

        let resp = self.fsys.transact(Tattach {
            fid: dir.fid.fid(),
            afid: !0,
            uname: uname.into(),
            aname: aname.into()
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
    pub async fn stat(&self) -> io::Result<npwire::Stat> {
        self.fsys.stat(&self.fid).await
    }

    pub async fn try_clone(&self) -> io::Result<Self> {
        self.open_dir_at("").await
    }

    pub async fn open_dir_at(&self, path: impl AsRef<str>) -> io::Result<Self> {
        let dir = Directory {
            fsys: self.fsys.clone(),
            fid: self.fsys.get_fid().unwrap()
        };

        let wname = path.as_ref()
            .split('/')
            .filter(|&c| !(c.is_empty() || c == "."))
            .map(|c| c.into())
            .collect::<Vec<_>>();

        let nc = wname.len();

        let wqid = self.fsys.walk(
            &self.fid,
            &dir.fid,
            wname
        ).await?;

        if wqid.len() < nc {
            return Err(io::ErrorKind::NotFound.into());
        }
        
        if wqid.len() > nc {
            return Err(io::Error::other("invalid response from server"));
        }

        if wqid.last().is_some_and(|qid| qid.type_ & QTDIR != QTDIR) {
            return Err(io::ErrorKind::NotADirectory.into());
        }

        Ok(dir)
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