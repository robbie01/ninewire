use std::{path::PathBuf, sync::Arc};

use anyhow::bail;
use npwire::Qid;
use tokio::fs;

use super::*;
use crate::np::traits::{self, Resource};

type Atom = Arc<str>;

#[derive(Debug, Clone)]
enum PathInner {
    Root,
    Rpc,
    OnShare { share: Atom, rem: Vec<Atom> }
}

#[derive(Debug, Clone)]
pub struct PathResource {
    handler: Arc<crate::HandlerInner>,
    session: Arc<crate::Session>,
    qid: Qid,
    inner: PathInner
}

impl PathResource {
    pub fn root(handler: &crate::Handler, session: Arc<crate::Session>) -> Self {
        PathResource {
            handler: handler.inner.clone(),
            session,
            qid: ROOT_QID,
            inner: PathInner::Root
        }
    }

    fn name(&self) -> &str {
        match &self.inner {
            PathInner::Root => "/",
            PathInner::Rpc => "rpc",
            PathInner::OnShare { share, rem } => rem.last().unwrap_or(share)
        }
    }

    fn real_path(&self) -> Option<PathBuf> {
        let (mnt, rem) = match &self.inner {
            PathInner::Root | PathInner::Rpc => return None,
            PathInner::OnShare { share, rem } => (share, rem)
        };

        let mpath = self.handler.shares.get(mnt)?;
        Some(mpath.join(rem.iter().map(|p| AsRef::<std::path::Path>::as_ref(&p[..])).collect::<PathBuf>()))
    }

    async fn walk_one(mut self, component: &str) -> anyhow::Result<Self> {
        if component == ".." {
            match self.inner {
                PathInner::Root | PathInner::Rpc => {
                    self.inner = PathInner::Root;
                    self.qid = ROOT_QID;
                },
                PathInner::OnShare { share: _, ref mut rem } => {
                    if rem.pop().is_none() {
                        self.inner = PathInner::Root;
                        self.qid = ROOT_QID;
                    } else {
                        let meta = fs::metadata(self.real_path().unwrap()).await?;
                        self.qid = qid(&meta);
                    }
                }
            }
        } else {
            match self.inner {
                PathInner::Root => if component == "rpc" {
                    self.inner = PathInner::Rpc;
                    self.qid = RPC_QID;
                } else {
                    if let Some((share, _)) = self.handler.shares.get_key_value(component) {
                        self.inner = PathInner::OnShare { share: share.clone(), rem: Vec::new() };
                        let meta = fs::metadata(self.real_path().unwrap()).await?;
                        self.qid = qid(&meta);
                    } else {
                        bail!("No such file or directory");
                    }
                },
                PathInner::Rpc => bail!("No such file or directory"),
                PathInner::OnShare { share: _, ref mut rem } => {
                    rem.push(component.into());
                    let meta = fs::symlink_metadata(self.real_path().unwrap()).await?;
                    if meta.is_symlink() {
                        bail!("No such file or directory");
                    }
                    self.qid = qid(&meta);
                }
            }
        }

        Ok(self)
    }
}

impl traits::Resource for PathResource {
    type Error = anyhow::Error;
    
    fn qid(&self) -> Qid {
        self.qid
    }

    async fn remove(self) -> Result<(), Self::Error> {
        bail!("permission denied");
    }

    async fn stat(&self) -> Result<npwire::Stat, Self::Error> {
        match &self.inner {
            PathInner::Root => Ok(root_stat(&self.session)),
            PathInner::Rpc => Ok(rpc_stat(&self.session)),
            PathInner::OnShare { .. } => {
                let meta = fs::metadata(self.real_path().unwrap()).await?;
                Ok(stat(&self.session, self.name(), &meta))
            }
        }
    }

    async fn wstat(&self, _stat: npwire::Stat) -> Result<(), Self::Error> {
        bail!("permission denied");
    }
}

impl traits::PathResource for PathResource {
    type OpenResource = super::open::OpenResource;

    async fn create(&self, _name: &str, _perm: u32, _mode: u8) -> Result<Self::OpenResource, Self::Error> {
        bail!("permission denied");
    }

    async fn open(&self, mode: u8) -> Result<Self::OpenResource, Self::Error> {
        if mode != 0 {
            bail!("permission denied");
        }

        let res = match &self.inner {
            PathInner::Root => open::OpenResource::root(self.handler.clone(), self.session.clone()),
            PathInner::Rpc => todo!(),
            PathInner::OnShare { .. } => open::OpenResource::new(
                self.handler.clone(),
                self.session.clone(),
                self.name().to_owned(),
                self.real_path().unwrap(),
                self.qid
            )?
        };
        Ok(res)
    }

    async fn walk(&self, wname: &[&str]) -> Result<(Vec<Qid>, Option<Self>), Self::Error> {
        let mut new = Some(self.clone());

        let mut wqid = Vec::new();

        for &component in wname {
            if let Some(path) = new.take() {
                match path.walk_one(component).await {
                    Ok(path) => {
                        wqid.push(path.qid());
                        new = Some(path);
                    },
                    Err(e) => if wqid.is_empty() {
                        return Err(e);
                    } else {
                        break;
                    }
                }
            }
        }

        if new.is_some() {
            wqid.pop();
        }

        Ok((wqid, new))
    }
}