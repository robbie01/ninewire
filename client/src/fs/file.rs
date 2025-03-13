use std::{future::pending, io, mem, pin::Pin, sync::Arc, task::{ready, Context, Poll}};

use bytes::{Buf, Bytes};
use npwire::{RMessage, Rerror, Rread, Rwrite, Tread, Twrite, QTDIR};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};
use tokio_util::sync::ReusableBoxFuture;
use util::fidpool::FidHandle;

use super::{Directory, FilesystemInner};

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

    pub async fn write_at(&self, data: Bytes, offset: u64) -> io::Result<u32> {
        let resp = self.fsys.transact(Twrite {
            fid: self.fid.fid(),
            offset,
            data
        }).await?;

        match resp {
            RMessage::Rerror(Rerror { ename }) => Err(io::Error::other(&ename[..])),
            RMessage::Rwrite(Rwrite { count }) => Ok(count),
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

pub struct FileReader<'a> {
    file: &'a File,
    offset: u64,
    fut_valid: bool,
    fut: ReusableBoxFuture<'a, io::Result<Bytes>>
}

impl<'a> FileReader<'a> {
    pub fn new(file: &'a File) -> Self {
        Self {
            file,
            offset: 0,
            fut_valid: false,
            fut: ReusableBoxFuture::new(pending())
        }
    }
}

impl AsyncRead for FileReader<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
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

impl AsyncSeek for FileReader<'_> {
    fn start_seek(mut self: Pin<&mut Self>, position: io::SeekFrom) -> io::Result<()> {
        match position {
            io::SeekFrom::Start(offset) => {
                if offset != self.offset {
                    self.fut_valid = false;
                }
                self.offset = offset;
                Ok(())
            },
            io::SeekFrom::Current(offset) => {
                let offset = self.offset.checked_add_signed(offset).ok_or(io::ErrorKind::InvalidInput)?;
                if offset != self.offset {
                    self.fut_valid = false;
                }
                self.offset = offset;
                Ok(())
            },
            io::SeekFrom::End(_) => {
                todo!()
            }
        }
    }

    fn poll_complete(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        Poll::Ready(Ok(self.offset))
    }
}

pub struct FileWriter<'a> {
    file: &'a File,
    offset: u64,
    buffer: Bytes,
    fut_valid: bool,
    fut: ReusableBoxFuture<'a, io::Result<u32>>
}

impl<'a> FileWriter<'a> {
    pub fn new(file: &'a File) -> Self {
        Self {
            file,
            offset: 0,
            buffer: Bytes::new(),
            fut_valid: false,
            fut: ReusableBoxFuture::new(pending())
        }
    }
}

impl AsyncWrite for FileWriter<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.as_mut().poll_flush(cx))?;
        self.buffer = Bytes::copy_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while !self.buffer.is_empty() {
            if !self.fut_valid {
                let fut = self.file.write_at(self.buffer.clone(), self.offset);
                self.fut.set(fut);
                self.fut_valid = true;
            }
    
            let res = ready!(self.fut.poll(cx))?;
            self.fut_valid = false;
            self.buffer.advance(res as usize);
            self.offset += u64::from(res);
        }

        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl AsyncSeek for FileWriter<'_> {
    fn start_seek(mut self: Pin<&mut Self>, position: io::SeekFrom) -> io::Result<()> {
        if !self.buffer.is_empty() || self.fut_valid {
            return Err(io::ErrorKind::Other.into());
        }

        match position {
            io::SeekFrom::Start(offset) => {
                self.offset = offset;
                Ok(())
            },
            io::SeekFrom::Current(offset) => {
                self.offset = self.offset.checked_add_signed(offset).ok_or(io::ErrorKind::InvalidInput)?;
                Ok(())
            },
            io::SeekFrom::End(_) => {
                todo!()
            }
        }
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        ready!(self.as_mut().poll_flush(cx))?;
        Poll::Ready(Ok(self.offset))
    }
}