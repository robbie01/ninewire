use std::{collections::HashMap, path::PathBuf, sync::LazyLock};

use crate::Atom;

use npwire::{Qid, Stat, DMDIR, QTDIR, QTFILE};
use tokio::fs;

pub type MountTable = HashMap<Atom, PathBuf>;

#[derive(Debug, Clone)]
pub(super) enum PathInner {
    Root,
    Rpc,
    OnMount { mount: Atom, rem: Vec<Atom> }
}

#[derive(Debug, Clone)]
pub struct Path(pub(super) PathInner);

pub const ROOT_QID: Qid = Qid { type_: QTDIR, version: 0, path: 0 };
pub const RPC_QID: Qid = Qid { type_: QTFILE, version: 0, path: !0 };

pub static ROOT_STAT: LazyLock<Stat> = LazyLock::new(|| Stat {
    type_: 0,
    dev: 0,
    qid: ROOT_QID,
    mode: DMDIR | 0o555,
    atime: 0,
    mtime: 0,
    length: 0,
    name: "/".into(),
    uid: "me".into(),
    gid: "me".into(),
    muid: "me".into()
});

pub static RPC_STAT: LazyLock<Stat> = LazyLock::new(|| Stat {
    type_: 0,
    dev: 0,
    qid: RPC_QID,
    mode: 0o666,
    atime: 0,
    mtime: 0,
    length: 0,
    name: "rpc".into(),
    uid: "me".into(),
    gid: "me".into(),
    muid: "me".into()
});

impl Path {
    pub const fn root() -> Self {
        Self(PathInner::Root)
    }

    pub const fn is_root(&self) -> bool {
        matches!(self.0, PathInner::Root)
    }

    pub const fn is_rpc(&self) -> bool {
        matches!(self.0, PathInner::Rpc)
    }

    fn ascend(&mut self) {
        match &mut self.0 {
            PathInner::Root => (),
            PathInner::Rpc => {
                self.0 = PathInner::Root
            },
            PathInner::OnMount { mount: _, rem } if rem.is_empty() => {
                self.0 = PathInner::Root;
            },
            PathInner::OnMount { mount: _, rem } => {
                rem.pop();
            }
        }
    }

    fn descend(&mut self, component: Atom) -> bool {
        match &mut self.0 {
            PathInner::Root => {
                if component[..] == *"rpc" {
                    self.0 = PathInner::Rpc
                } else {
                    self.0 = PathInner::OnMount { mount: component, rem: Vec::new() };
                }
                true
            },
            PathInner::Rpc => {
                false
            },
            PathInner::OnMount { mount: _, rem } => {
                rem.push(component);
                true
            }
        }
    }

    pub fn real_path(&self, mnts: &MountTable) -> Option<PathBuf> {
        let (mnt, rem) = match &self.0 {
            PathInner::Root | PathInner::Rpc => return None,
            PathInner::OnMount { mount, rem } => (mount, rem)
        };

        let mpath = mnts.get(mnt)?;
        Some(mpath.join(rem.iter().map(|p| AsRef::<std::path::Path>::as_ref(&p[..])).collect::<PathBuf>()))
    }

    async fn qid(&self, mnts: &MountTable) -> Option<Qid> {
        match &self.0 {
            PathInner::Root => Some(ROOT_QID),
            PathInner::Rpc => Some(Qid { type_: QTFILE, version: 0, path: !0 }),
            _ => {
                let path = self.real_path(mnts)?;
                let meta = fs::metadata(path).await.ok()?;
                Some(super::qid(&meta))
            }
        }
    }

    pub async fn walk_one(mut self, mnts: &MountTable, component: Atom) -> Option<(Self, Qid)> {
        if component.contains('/') { return None; }
        if component == *"." { return None; }

        if component == *".." {
            self.ascend();
        } else if !self.descend(component) {
            return None
        }

        let qid = self.qid(mnts).await?;
        Some((self, qid))
    }

    pub fn name(&self) -> Atom {
        match &self.0 {
            PathInner::Root => "/".into(),
            PathInner::Rpc => "rpc".into(),
            PathInner::OnMount { mount, rem } => match rem.last() {
                Some(component) => component.clone(),
                None => mount.clone()
            }
        }
    }

    pub async fn stat(&self, mnts: &MountTable) -> Option<Stat> {
        match &self.0 {
            PathInner::Root => Some(ROOT_STAT.clone()),
            _ => {
                let path = self.real_path(mnts)?;
                let meta = fs::metadata(path).await.ok()?;
                Some(super::stat(&self.name(), &meta))
            }
        }
    }
}