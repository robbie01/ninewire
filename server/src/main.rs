use std::{collections::HashMap as StdHashMap, fmt::Debug, io, net::Ipv4Addr, path::PathBuf, sync::Arc};

use bytes::Bytes;
use np::Serve;
use npwire::{Qid, Stat};
use res::path::ROOT_QID;
use scc::{hash_map::Entry, HashMap};
use thiserror::Error;
use tokio::net::TcpListener;
use string_cache::DefaultAtom as Atom;

mod np;
mod res;

#[derive(Debug, Error)]
enum HandlerError {
    #[error("Function not implemented")]
    Unimplemented,
    #[error("fid is already in use")]
    FidInUse,
    #[error("Unknown fid")]
    FidNotFound,
    #[error("Invalid argument")]
    InvalidArgument,
    #[error("No such file or directory")]
    NoEnt,
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Transient")]
    Transient
}

#[derive(Debug)]
enum Resource {
    Path(res::path::Path),
    Open(res::open::Open)
}

type MountTable = StdHashMap<Atom, PathBuf>;

#[derive(Debug)]
struct Handler<Fid:  np::traits::Fid + Debug> {
    mounts: MountTable,
    fids: HashMap<Fid, Resource>
}

impl<Fid: np::traits::Fid + Debug> Handler<Fid> {
    fn new(mounts: MountTable) -> Self {
        Self {
            mounts,
            fids: HashMap::new()
        }
    }
}

impl<Fid: np::traits::Fid + Debug> Serve<Fid> for Handler<Fid> {
    type Error = anyhow::Error;

    async fn auth(&self, _afid: Fid, _uname: &str, _aname: &str) -> anyhow::Result<Qid> {
        Err(HandlerError::Unimplemented.into())
    }

    async fn attach(&self, fid: Fid, afid: Fid, _uname: &str, aname: &str) -> anyhow::Result<Qid> {
        if fid.is_nofid() || !afid.is_nofid() {
            return Err(HandlerError::InvalidArgument.into());
        }

        if !aname.is_empty() {
            return Err(HandlerError::NoEnt.into());
        }

        match self.fids.entry_async(fid).await {
            Entry::Occupied(_) => Err(HandlerError::FidInUse.into()),
            Entry::Vacant(v) => {
                v.insert_entry(Resource::Path(res::path::Path::root()));
                Ok(ROOT_QID)
            }
        }
    }

    async fn walk(&self, fid: Fid, newfid: Fid, wname: Vec<&str>) -> anyhow::Result<impl IntoIterator<Item = Qid>> {
        if newfid.is_nofid() {
            return Err(HandlerError::InvalidArgument.into());
        }

        // Hacking around avoiding a potential deadlock
        let path = if fid == newfid {
            None
        } else {
            self.fids.read_async(&fid, |_, res| match res {
                Resource::Path(path) => Ok(path.clone()),
                _ => Err(HandlerError::InvalidArgument)
            }).await.transpose()?.ok_or(HandlerError::FidNotFound)?.into()
        };

        let newres = self.fids.entry_async(newfid).await;
    
        let path = match path {
            None => match &newres {
                Entry::Occupied(o) => match o.get() {
                    Resource::Path(path) => path.clone(),
                    _ => return Err(HandlerError::InvalidArgument.into())
                },
                Entry::Vacant(_) => return Err(HandlerError::FidNotFound.into())
            },
            Some(path) => match newres {
                Entry::Occupied(_) => return Err(HandlerError::FidInUse.into()),
                Entry::Vacant(_) => path
            }
        };
        
        if wname.is_empty() {
            newres.insert_entry(Resource::Path(path));
            return Ok(Vec::new());
        }

        let mut path = Some(path);

        let mut qids = Vec::with_capacity(wname.len());
        for component in wname.iter().copied() {
            let component = component.into();
            let Some((p, qid)) = path.take().unwrap().walk_one(&self.mounts, component).await else { break };
            path = Some(p);
            qids.push(qid);
        }

        assert_eq!(path.is_some(), qids.len() == wname.len());

        if qids.is_empty() {
            Err(HandlerError::NoEnt.into())
        } else {
            if let Some(path) = path {
                newres.insert_entry(Resource::Path(path));
            }
            Ok(qids)
        }
    }

    async fn open(&self, fid: Fid, mode: u8) -> anyhow::Result<(Qid, u32)> {
        let mut res = self.fids.get_async(&fid).await.ok_or(HandlerError::FidNotFound)?;

        let path = match &*res {
            Resource::Path(path) => path,
            _ => return Err(HandlerError::InvalidArgument.into())
        };

        if mode != 0 {
            return Err(HandlerError::Unimplemented.into());
        }

        let open = if path.is_root() {
            res::open::Open::root(&self.mounts)
        } else {
            let real = path.real_path(&self.mounts).ok_or(HandlerError::Transient)?;
            res::open::Open::new(path.name(), real).await?
        };
        let qid = open.qid().await?;
        res.insert(Resource::Open(open));
        Ok((qid, 0))
    }

    async fn create(&self, _fid: Fid, _name: &str, _perm: u32, _mode: u8) -> anyhow::Result<(Qid, u32)> {
        Err(HandlerError::Unimplemented.into())
    }

    async fn read(&self, fid: Fid, offset: u64, count: u32) -> anyhow::Result<Bytes> {
        let mut res = self.fids.get_async(&fid).await.ok_or(HandlerError::FidNotFound)?;
        match res.get_mut() {
            Resource::Open(o) => Ok(o.read(offset, count).await?),
            _ => Err(HandlerError::InvalidArgument.into())
        }
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> anyhow::Result<u32> {
        Err(HandlerError::Unimplemented.into())
    }

    async fn clunk(&self, fid: Fid) -> anyhow::Result<()> {
        match self.fids.remove_async(&fid).await {
            Some(_) => Ok(()),
            None => Err(HandlerError::FidNotFound.into())
        }
    }

    async fn remove(&self, fid: Fid) -> anyhow::Result<()> {
        self.clunk(fid).await?;
        Err(HandlerError::PermissionDenied.into())
    }

    async fn stat(&self, fid: Fid) -> anyhow::Result<Stat> {
        // NOTE: this is an unnecessary exclusive borrow
        let res = self.fids.get_async(&fid).await.ok_or(HandlerError::FidNotFound)?;

        match res.get() {
            Resource::Path(path) => path.stat(&self.mounts).await.ok_or(HandlerError::Transient.into()),
            Resource::Open(o) => Ok(o.stat().await?)
        }
    }

    async fn wstat(&self, _fid: Fid, _stat: Stat) -> anyhow::Result<()> {
        Err(HandlerError::Unimplemented.into())
    }
    
    async fn clunk_where(&self, mut matcher: impl FnMut(Fid) -> bool + Send) {
        self.fids.retain_async(|&k, _| !matcher(k)).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 64444)).await?;

    np::serve(Arc::new(Handler::new([
        ("forfun".into(), "forfun".into()),
        ("ff2".into(), "forfun".into())
    ].into_iter().collect())), listener).await?;

    Ok(())
}
