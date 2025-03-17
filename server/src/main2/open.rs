use std::{fs::File, io, os::unix::fs::FileExt, path::PathBuf, sync::Arc};

use anyhow::bail;
use bytes::BytesMut;
use npwire::{Qid, QTDIR};
use tokio::{sync::Mutex, task};

use super::helpers::*;
use crate::{np::traits, res::path::{ROOT_QID, ROOT_STAT, RPC_STAT}};

#[derive(Debug, Default)]
struct DirState {
    rem: Vec<Arc<str>>,
    last_offset: u64
}

#[derive(Debug)]
enum OpenInner {
    Root(Mutex<DirState>),
    File(File),
    Dir {
        path: PathBuf,
        dir_state: Mutex<DirState>
    }
}

#[derive(Debug)]
pub struct OpenResource<'a> {
    handler: &'a super::Handler,
    session: Arc<super::Session>,
    qid: Qid,
    name: String,
    inner: OpenInner
}

impl<'a> OpenResource<'a> {
    pub fn root(handler: &'a super::Handler, session: Arc<super::Session>) -> Self {
        Self {
            handler,
            session,
            qid: ROOT_QID,
            name: String::from("/"),
            inner: OpenInner::Root(Mutex::default())
        }
    }

    pub fn new(handler: &'a super::Handler, session: Arc<super::Session>, name: String, path: PathBuf, qid: Qid) -> io::Result<Self> {
        if qid.type_ & QTDIR == QTDIR {
            Ok(Self {
                handler,
                session,
                qid,
                name,
                inner: OpenInner::Dir { 
                    path, 
                    dir_state: Mutex::default()
                }
            })
        } else {
            let file = File::open(&path)?;
            Ok(Self {
                handler,
                session,
                qid,
                name,
                inner: OpenInner::File(file)
            })
        }
    }
}

impl<'a> traits::Resource for OpenResource<'a> {
    type Error = anyhow::Error;

    fn qid(&self) -> Qid {
        self.qid
    }

    async fn remove(self) -> Result<(), Self::Error> {
        bail!("Function not implemented");
    }

    async fn stat(&self) -> Result<npwire::Stat, Self::Error> {
        match &self.inner {
            OpenInner::Root { .. } => Ok(ROOT_STAT.clone()),
            OpenInner::File(file) => {
                let file = file.try_clone()?;
                let meta = task::spawn_blocking(move || file.metadata()).await??;
                Ok(stat(&self.session, &self.name, &meta))
            },
            OpenInner::Dir { path, .. } => {
                let path = path.clone();
                let meta = task::spawn_blocking(move || std::fs::metadata(path)).await??;
                Ok(stat(&self.session, &self.name, &meta))
            }
        }
    }

    async fn wstat(&self, _stat: npwire::Stat) -> Result<(), Self::Error> {
        bail!("Function not implemented");
    }
}

impl<'a> traits::OpenResource for OpenResource<'a> {
    async fn read(&self, offset: u64, count: u32) -> Result<bytes::Bytes, Self::Error> {
        match &self.inner {
            OpenInner::File(file) => {
                let file = file.try_clone()?;
                Ok(task::spawn_blocking(move || {
                    let mut buf = BytesMut::zeroed(count as usize);
                    let n = file.read_at(&mut buf, offset)?;
                    buf.truncate(n);
                    Ok::<_, io::Error>(buf.freeze())
                }).await??)
            },
            OpenInner::Dir { path, dir_state } => {
                let mut state = dir_state.lock().await;
                
                if offset == 0 {
                    state.rem.clear();
                    state.last_offset = 0;
                    
                    let path = path.clone();

                    let entries = task::spawn_blocking(move || {
                        let mut entries = Vec::new();
                        let readdir = std::fs::read_dir(path)?;
                        for entry in readdir {
                            let entry = entry?;
                            let name = entry.file_name();
                            let meta = entry.metadata()?;
                            if meta.is_symlink() { continue } // Skip symlinks
                            if let Some(name) = name.to_str() {
                                entries.push(name.into());
                            }
                        }
                        Ok::<_, io::Error>(entries)
                    }).await??;
                    
                    state.rem = entries;
                } else if offset != state.last_offset {
                    bail!("Invalid offset for directory read");
                }
                
                let mut buf = BytesMut::new();
                
                while let Some(name) = state.rem.first() {
                    let file_path = path.join(&**name);
                    let meta = task::spawn_blocking(move || {
                        std::fs::metadata(file_path)
                    }).await??;
                    
                    let stat = stat(&self.session, &name, &meta);
                    
                    let oldlen = buf.len();
                    npwire::put_stat(&mut buf, &stat)?;
                    if buf.len() > count as usize {
                        buf.truncate(oldlen);
                        break;
                    }
                    
                    state.rem.remove(0);
                }
                
                state.last_offset += buf.len() as u64;
                Ok(buf.freeze())
            },
            OpenInner::Root(dir_state) => {
                let mut state = dir_state.lock().await;
                
                if offset == 0 {
                    state.rem.clear();
                    state.last_offset = 0;
                    
                    state.rem.push("rpc".into());
                    for share in self.handler.shares.keys() {
                        state.rem.push(share.clone());
                    }
                } else if offset != state.last_offset {
                    bail!("Invalid offset for root directory read");
                }
                
                let mut buf = BytesMut::new();
                
                while let Some(name) = state.rem.first().cloned() {
                    let stat = if *name == *"rpc" {
                        RPC_STAT.clone()
                    } else {
                        let path = self.handler.shares.get(&name).unwrap().clone();
                        let meta = task::spawn_blocking(move || {
                            std::fs::metadata(path)
                        }).await??;
                        stat(&self.session, &name, &meta)
                    };
                    
                    let oldlen = buf.len();
                    npwire::put_stat(&mut buf, &stat)?;
                    if buf.len() > count as usize {
                        buf.truncate(oldlen);
                        break;
                    }
                    
                    state.rem.remove(0);
                }
                
                state.last_offset += buf.len() as u64;
                Ok(buf.freeze())
            }
        }
    }

    async fn write(&self, _offset: u64, _data: &[u8]) -> Result<u32, Self::Error> {
        match &self.inner {
            OpenInner::File(_) => {
                bail!("Permission denied");
            }
            _ => {
                bail!("Function not implemented");
            }
        }
    }
}