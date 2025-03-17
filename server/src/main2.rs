use std::sync::Arc;

use anyhow::bail;
use bytestring::ByteString;

use crate::{np::traits, ShareTable};

mod open;
mod path;

#[derive(Debug)]
struct Handler {
    shares: ShareTable
}

#[derive(Debug)]
struct Session {
    uname: ByteString
}

impl traits::Serve2 for Handler {
    type Error = anyhow::Error;
    type PathResource<'a> = path::PathResource<'a>;
    type OpenResource<'a> = open::OpenResource<'a>;

    async fn auth<'a>(&'a self, _uname: &str, _aname: &str) -> Result<Self::OpenResource<'a>, Self::Error> {
        bail!("Function not implemented");
    }

    async fn attach<'a>(&'a self, ares: Option<&Self::OpenResource<'a>>, uname: &str, aname: &str) -> Result<Self::PathResource<'a>, Self::Error> {
        if ares.is_some() {
            bail!("Permission denied");
        }

        if !aname.is_empty() {
            bail!("No such file or directory");
        }

        let session = Arc::new(Session {
            uname: uname.into()
        });

        Ok(path::PathResource::root(self, session))
    }
}

mod helpers {
    use std::fs::Metadata;

    use npwire::{Qid, Stat, DMDIR, QTDIR, QTFILE};

    fn inode(meta: &Metadata) -> u64 {
        cfg_if::cfg_if! {
            if #[cfg(unix)] {
                std::os::unix::fs::MetadataExt::ino(meta)
            } else {
                compile_error!("implement inode")
            }
        }
    }
    
    pub fn qid(meta: &Metadata) -> Qid {
        Qid {
            type_: if meta.is_dir() { QTDIR } else { QTFILE },
            version: 0,
            path: inode(meta)
        }
    }
    
    pub fn stat(session: &super::Session, name: &str, meta: &Metadata) -> Stat {
        let uname = &session.uname;
    
        Stat {
            type_: 0,
            dev: 0,
            qid: qid(meta),
            mode: if meta.is_dir() { DMDIR | 0o555 } else { 0o444 },
            atime: 0,
            mtime: 0,
            length: if meta.is_dir() { 0 } else { meta.len() },
            name: name.into(),
            uid: uname.clone(),
            gid: uname.clone(),
            muid: uname.clone()
        }
    }
}