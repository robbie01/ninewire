use std::{path::PathBuf, sync::Arc};

use anyhow::{bail, ensure};
use npwire::Qid;
use tokio::fs;

use super::{helpers::*, open};
use crate::{np::traits, res::path::{ROOT_QID, ROOT_STAT, RPC_STAT}, Atom};

#[derive(Debug, Clone)]
enum PathInner {
    Root,
    Rpc,
    OnShare { share: Atom, rem: Vec<Atom> }
}

#[derive(Debug)]
pub struct PathResource<'a> {
    handler: &'a super::Handler,
    valid: bool,
    session: Arc<super::Session>,
    qid: Qid,
    inner: PathInner
}

impl<'a> PathResource<'a> {
    pub(super) fn root(handler: &'a super::Handler, session: Arc<super::Session>) -> Self {
        PathResource {
            handler,
            valid: true,
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
}

impl<'a> traits::Resource for PathResource<'a> {
    type Error = anyhow::Error;
    
    fn qid(&self) -> Qid {
        self.qid
    }

    async fn remove(self) -> Result<(), Self::Error> {
        ensure!(self.valid);
        bail!("Permission denied");
    }

    async fn stat(&self) -> Result<npwire::Stat, Self::Error> {
        ensure!(self.valid);

        match &self.inner {
            PathInner::Root => Ok(ROOT_STAT.clone()),
            PathInner::Rpc => Ok(RPC_STAT.clone()),
            PathInner::OnShare { .. } => {
                let meta = fs::metadata(self.real_path().unwrap()).await?;
                Ok(stat(&self.session, self.name(), &meta))
            }
        }
    }

    async fn wstat(&self, _stat: npwire::Stat) -> Result<(), Self::Error> {
        ensure!(self.valid);
        bail!("Permission denied");
    }
}

impl<'a> traits::PathResource for PathResource<'a> {
    type OpenResource = super::open::OpenResource<'a>;

    async fn create(&self, _name: &str, _perm: u32, _mode: u8) -> Result<Self::OpenResource, Self::Error> {
        ensure!(self.valid);
        bail!("Permission denied");
    }

    async fn open(&mut self, mode: u8) -> Result<Self::OpenResource, Self::Error> {
        ensure!(self.valid);

        if mode != 0 {
            bail!("Permission denied");
        }

        let res = match &self.inner {
            PathInner::Root => open::OpenResource::root(self.handler, self.session.clone()),
            PathInner::Rpc => todo!(),
            PathInner::OnShare { .. } => open::OpenResource::new(
                self.handler,
                self.session.clone(),
                self.name().to_owned(),
                self.real_path().unwrap(),
                self.qid
            )?
        };
        
        self.valid = false;
        Ok(res)
    }

    async fn walk(&self, wname: &[&str]) -> Result<(Vec<Qid>, Self), Self::Error> {
        ensure!(self.valid);
        todo!()
    }
}