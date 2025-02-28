use std::{future::pending, mem, pin::Pin, task::{ready, Poll}};

use bytes::Bytes;
use npwire::{Rread, Tread};
use tokio_util::sync::ReusableBoxFuture;

use super::*;

#[derive(Debug)]
pub struct File {
    pub(super) fsys: Arc<FilesystemInner>,
    pub(super) fid: FidHandle
}

impl Directory {
    pub async fn open_at(&self, path: impl AsRef<str>) -> io::Result<File> {
        let file = File {
            fsys: self.fsys.clone(),
            fid: self.fsys.get_fid().unwrap()
        };

        let wname = path.as_ref()
            .split('/')
            .filter(|&c| !(c.is_empty() || c == "."))
            .map(|c| c.into())
            .collect::<Vec<_>>();

        if wname.is_empty() {
            return Err(io::ErrorKind::IsADirectory.into());
        }

        let nc = wname.len();

        let wqid = self.fsys.walk(
            &self.fid,
            &file.fid,
            wname).await?;

        if wqid.len() < nc {
            return Err(io::ErrorKind::NotFound.into());
        }
        
        if wqid.len() > nc {
            return Err(io::Error::other("invalid response from server"));
        }

        if wqid.last().unwrap().type_ & QTDIR == QTDIR {
            return Err(io::ErrorKind::IsADirectory.into());
        }

        let qid = self.fsys.open(&file.fid).await?;

        assert_eq!(wqid.last(), Some(&qid));

        Ok(file)
    }
}

impl File {
    pub async fn stat(&self) -> io::Result<npwire::Stat> {
        self.fsys.stat(&self.fid).await
    }

    pub async fn read_at(&self, count: u32, offset: u64) -> io::Result<Bytes> {
        let resp = self.fsys.transact(Tread {
            fid: self.fid.fid(),
            offset,
            count
        }).await?;

        match resp {
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&ename[..])),
            RMessage::Rread(Rread { data }) => Ok(data),
            _ => Err(io::Error::other("unexpected message type"))
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        let fsys = self.fsys.clone();
        let fid = mem::take(&mut self.fid);
        
        tokio::spawn(async move {
            let _ = fsys.clunk(fid).await;
        });
    }
}

pub struct ReadableFile<'a> {
    file: &'a File,
    offset: u64,
    fut_valid: bool,
    fut: ReusableBoxFuture<'a, io::Result<Bytes>>
}

impl<'a> ReadableFile<'a> {
    pub fn new(file: &'a File) -> Self {
        Self {
            file,
            offset: 0,
            fut_valid: false,
            fut: ReusableBoxFuture::new(pending())
        }
    }
}

impl AsyncRead for ReadableFile<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        if !self.fut_valid {
            let count = buf.remaining().try_into().unwrap_or(u32::MAX);
            let fut = self.file.read_at(count, self.offset);
            self.fut.set(fut);
            self.fut_valid = true;
        }

        let res = ready!(self.fut.poll(cx))?;
        self.fut_valid = false;
        let n = res.len().min(buf.remaining());
        buf.put_slice(&res[..n]);
        self.offset += n as u64;

        Poll::Ready(Ok(()))
    }
}