use std::{pin::pin, sync::atomic::{AtomicU64, Ordering}};

use tokio_util::sync::CancellationToken;

use crate::mpv::{self, StreamError};

pub struct NineStream {
    handle: tokio::runtime::Handle,
    cancel: CancellationToken,
    file: Option<client::File>,
    size: AtomicU64,
    pos: AtomicU64
}

impl NineStream {
    pub fn new(file: client::File) -> Self {
        Self {
            handle: tokio::runtime::Handle::current(),
            cancel: CancellationToken::new(),
            file: Some(file),
            size: AtomicU64::new(u64::MAX),
            pos: AtomicU64::new(0)
        }
    }

    fn file(&self) -> &client::File {
        self.file.as_ref().unwrap()
    }

    fn size(&self) -> u64 {
        let cancelled = pin!(self.cancel.cancelled());

        let mut n = self.size.load(Ordering::Relaxed);

        if n == u64::MAX {
            n = self.handle.block_on(async {
                tokio::select! {
                    biased;
                    () = cancelled => u64::MAX,
                    r = self.file().stat() => match r {
                        Ok(stat) => stat.length,
                        Err(_) => u64::MAX
                    }
                }
            });
            self.size.store(n, Ordering::Relaxed);
        }

        n
    }
}

unsafe impl mpv::Stream for NineStream {
    const SIZEABLE: bool = true;
    const SEEKABLE: bool = true;
    const CANCELABLE: bool = true;

    fn read(&self, buf: &mut [u8]) -> Result<u64, mpv::StreamError> {
        let cancelled = pin!(self.cancel.cancelled());
        let _guard = self.handle.enter();

        let pos = self.pos.load(Ordering::Relaxed);

        let b = self.handle.block_on(async {
            tokio::select! {
                biased;
                () = cancelled => Err(StreamError),
                r = self.file().read_at(
                    buf.len().min(u32::MAX as usize) as u32,
                    pos
                ) => r.map_err(|_| StreamError)
            }
        })?;

        buf[..b.len()].copy_from_slice(&b);
        self.pos.store(pos + b.len() as u64, Ordering::Relaxed);

        Ok(b.len() as u64)
    }

    fn size(&self) -> Result<u64, StreamError> {
        let _guard = self.handle.enter();

        let size = self.size();

        if size == 0 || size == u64::MAX {
            Err(StreamError)
        } else {
            Ok(size)
        }
    }

    fn seek(&self, offset: u64) -> Result<u64, StreamError> {
        let _guard = self.handle.enter();

        let size = self.size();

        if size == 0 || size == u64::MAX {
            Err(StreamError)
        } else {
            let pos = offset.min(size);
            self.pos.store(pos, Ordering::Relaxed);
            Ok(pos)
        }
    }

    fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for NineStream {
    fn drop(&mut self) {
        let _guard = self.handle.enter();
        drop(self.file.take().unwrap())
    }
}