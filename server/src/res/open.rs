use std::{fs::{read_dir, File}, io, mem, os::unix::fs::FileExt, path::PathBuf, sync::Arc};

use anyhow::bail;
use bytes::BytesMut;
use npwire::{put_stat, Qid, QTDIR};
use tokio::{fs, io::{AsyncReadExt, AsyncWriteExt, Empty}, sync::Mutex, task};

use super::*;
use crate::np::traits;

#[derive(Debug, Default)]
struct RootState {
    rem: Vec<Arc<str>>,
    last_offset: u64
}

#[derive(Debug, Default)]
struct DirState {
    rem: Vec<(Arc<str>, Metadata)>,
    last_offset: u64
}

#[derive(Debug)]
enum OpenInner {
    Root(Mutex<RootState>),
    File(File),
    Dir {
        path: PathBuf,
        dir_state: Mutex<DirState>
    },
    Rpc(Mutex<Empty>)
}

#[derive(Debug)]
pub struct OpenResource {
    handler: Arc<crate::Config>,
    session: Arc<crate::Session>,
    qid: Qid,
    name: String,
    inner: OpenInner
}

impl OpenResource {
    pub fn root(handler: Arc<crate::Config>, session: Arc<crate::Session>) -> Self {
        Self {
            handler,
            session,
            qid: ROOT_QID,
            name: String::from("/"),
            inner: OpenInner::Root(Mutex::default())
        }
    }

    pub fn rpc(handler: Arc<crate::Config>, session: Arc<crate::Session>) -> Self {
        Self {
            handler,
            session,
            qid: RPC_QID,
            name: String::from("rpc"),
            inner: OpenInner::Rpc(Mutex::new(tokio::io::empty()))
        }
    }

    pub fn new(handler: Arc<crate::Config>, session: Arc<crate::Session>, name: String, path: PathBuf, qid: Qid) -> io::Result<Self> {
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

impl traits::Resource for OpenResource {
    type Error = anyhow::Error;

    fn qid(&self) -> Qid {
        self.qid
    }

    async fn remove(self) -> Result<(), Self::Error> {
        bail!("Function not implemented");
    }

    async fn stat(&self) -> Result<npwire::Stat, Self::Error> {
        match &self.inner {
            OpenInner::Root { .. } => Ok(root_stat(&self.session)),
            OpenInner::File(file) => {
                let file = file.try_clone()?;
                let meta = task::spawn_blocking(move || file.metadata()).await??;
                Ok(stat(&self.session, &self.name, &meta))
            },
            OpenInner::Dir { path, .. } => {
                let meta = fs::metadata(path).await?;
                Ok(stat(&self.session, &self.name, &meta))
            },
            OpenInner::Rpc(..) => Ok(rpc_stat(&self.session))
        }
    }

    async fn wstat(&self, stat: npwire::Stat) -> Result<(), Self::Error> {
        match self.inner {
            OpenInner::Rpc(..) => {
                if stat.qid.type_ != !0 || stat.qid.version != !0 || stat.qid.path != !0 || stat.mode != !0 || !stat.name.is_empty() || !stat.uid.is_empty() || !stat.gid.is_empty() || !stat.muid.is_empty() {
                    bail!("permission denied")
                }
                Ok(())
            },
            _ => bail!("permission denied")
        }
    }
}

impl traits::OpenResource for OpenResource {
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
                let DirState { ref mut rem, ref mut last_offset } = *dir_state.lock().await;

                if offset == 0 {
                    rem.clear();
                    let mut rem2 = mem::take(rem);
                    let readdir = read_dir(path)?;
                    *rem = task::spawn_blocking(move || {
                        rem2.clear();
                        rem2.extend(readdir.filter_map(|dent| {
                            let dent = dent.unwrap();
                            let name = dent.file_name();
                            let meta = dent.metadata().unwrap();
                            if meta.is_symlink() { return None }
                            Some((name.to_str()?.into(), meta))
                        }));
                        rem2
                    }).await?;
                } else if offset != *last_offset {
                    bail!("Invalid argument");
                }

                let mut buf = BytesMut::new();
                while let Some((name, meta)) = rem.first() {
                    let stat = stat(&self.session, name, meta);

                    let oldlen = buf.len();
                    put_stat(&mut buf, &stat)?;
                    if buf.len() > count as usize {
                        buf.truncate(oldlen);
                        break;
                    }

                    rem.remove(0);
                }

                *last_offset += buf.len() as u64;

                Ok(buf.freeze())
            },
            OpenInner::Root(dir_state) => {
                let mut state = dir_state.lock().await;
                
                if offset == 0 {
                    state.rem.clear();
                    state.last_offset = 0;
                    
                    state.rem.push("rpc".into());
                    state.rem.extend(self.handler.shares.keys().cloned());
                } else if offset != state.last_offset {
                    bail!("Invalid offset for root directory read");
                }
                
                let mut buf = BytesMut::new();
                
                while let Some(name) = state.rem.first().cloned() {
                    let stat = if *name == *"rpc" {
                        rpc_stat(&self.session)
                    } else {
                        let path = &self.handler.shares[&name];
                        let meta = fs::metadata(path).await?;
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
                
                state.last_offset += u64::try_from(buf.len())?;
                Ok(buf.freeze())
            },
            OpenInner::Rpc(rpc) => {
                let mut buf = BytesMut::zeroed(count.try_into()?);
                let n = rpc.lock().await.read(&mut buf).await?;
                buf.truncate(n);
                Ok(buf.freeze())
            }
        }
    }

    async fn write(&self, _offset: u64, data: &[u8]) -> Result<u32, Self::Error> {
        match &self.inner {
            OpenInner::File(..) | OpenInner::Dir { .. } | OpenInner::Root(..) => {
                bail!("fid not open for write");
            }
            OpenInner::Rpc(rpc) => {
                let n = rpc.lock().await.write(data).await?;
                Ok(n.try_into()?)
            }
        }
    }
}