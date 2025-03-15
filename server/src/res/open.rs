use std::{fs::Metadata, io, path::PathBuf};

use bytes::{Bytes, BytesMut};
use tokio::{fs::{self, File}, io::{AsyncReadExt, AsyncSeekExt}};

use crate::{Atom, HandlerError};
use npwire::{put_stat, Qid, Stat};

use super::path::{ShareTable, ROOT_QID, ROOT_STAT, RPC_STAT};

#[derive(Debug)]
enum OpenInner {
    Root {
        mnts: ShareTable,
        rem: Vec<Atom>,
        last_offset: u64
    },
    File(Atom, File),
    Dir {
        name: Atom,
        path: PathBuf,
        rem: Vec<(Atom, Metadata)>,
        last_offset: u64
    }
}

#[derive(Debug)]
pub struct Open(OpenInner);

impl Open {
    pub fn root(mnts: &ShareTable) -> Self {
        Self(OpenInner::Root {
            mnts: mnts.clone(),
            rem: Vec::new(),
            last_offset: 0
        })
    }

    pub async fn new(name: Atom, path: PathBuf) -> io::Result<Self> {
        let meta = fs::metadata(&path).await?;

        if meta.is_file() {
            let f = File::open(path).await?;
            Ok(Self(OpenInner::File(name, f)))
        } else {
            Ok(Self(OpenInner::Dir { name, path, rem: Vec::new(), last_offset: 0 }))
        }
    }

    pub async fn qid(&self) -> io::Result<Qid> {
        match &self.0 {
            OpenInner::File(_, f) => Ok(super::qid(&f.metadata().await?)),
            OpenInner::Dir { name: _, path, rem: _, last_offset: _ } =>
                Ok(super::qid(&fs::metadata(path).await?)),
            OpenInner::Root { .. } => Ok(ROOT_QID)
        }
    }

    pub async fn read(&mut self, offset: u64, count: u32) -> io::Result<Bytes> {
        match &mut self.0 {
            OpenInner::File(_, f) => {
                f.seek(io::SeekFrom::Start(offset)).await?;
                let mut buf = BytesMut::zeroed(count as usize);
                let n = f.read(&mut buf[..]).await?;
                buf.truncate(n);
                Ok(buf.freeze())
            },
            OpenInner::Dir { name: _, path, rem, last_offset } => {
                if offset == 0 {
                    rem.clear();
                    let mut readdir = fs::read_dir(path).await?;
                    while let Some(dent) = readdir.next_entry().await? {
                        let name = dent.file_name();
                        let meta = dent.metadata().await?;
                        if meta.is_symlink() { continue } // Too easy to break, no justifiable use case
                        let Some(name) = name.to_str() else { continue };
                        rem.push((name.into(), meta));
                    }
                } else if offset != *last_offset {
                    return Err(io::Error::other(HandlerError::InvalidArgument));
                }

                let mut buf = BytesMut::new();
                while let Some((name, meta)) = rem.first() {
                    let stat = super::stat(name, meta);

                    let oldlen = buf.len();
                    put_stat(&mut buf, &stat).map_err(io::Error::other)?;
                    if buf.len() > count as usize {
                        buf.truncate(oldlen);
                        break;
                    }

                    rem.remove(0);
                }

                *last_offset += buf.len() as u64;

                Ok(buf.freeze())
            },
            OpenInner::Root { mnts, rem, last_offset } => {
                if offset == 0 {
                    rem.clear();
                    rem.push("rpc".into());
                    rem.extend(mnts.keys().cloned());
                } else if offset != *last_offset {
                    return Err(io::Error::other("Invalid operation!"));
                }

                let mut buf = BytesMut::new();
                while let Some(name) = rem.first() {
                    let stat = if name == "rpc" {
                        RPC_STAT.clone()
                    } else {
                        let path = mnts.get(name).unwrap();
                        super::stat(name, &fs::metadata(path).await?)
                    };

                    let oldlen = buf.len();
                    put_stat(&mut buf, &stat).map_err(io::Error::other)?;
                    if buf.len() > count as usize {
                        buf.truncate(oldlen);
                        break;
                    }

                    rem.remove(0);
                }

                *last_offset += buf.len() as u64;

                Ok(buf.freeze())
            }
        }
    }

    pub async fn stat(&self) -> io::Result<Stat> {
        match &self.0 {
            OpenInner::File(name, f) => {
                let meta = f.metadata().await?;
                Ok(super::stat(name, &meta))
            },
            OpenInner::Dir { name, path, .. } => {
                let meta = fs::metadata(path).await?;
                Ok(super::stat(name, &meta))
            },
            OpenInner::Root { .. } => Ok(ROOT_STAT.clone())
        }
    }
}