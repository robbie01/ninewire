use std::{collections::VecDeque, mem, u32};

use npwire::{yank_stat, Stat};

use super::*;

#[derive(Debug)]
pub struct ReadDir {
    offset: u64,
    file: File,
    buffer: VecDeque<npwire::Stat>
}

impl Directory {
    pub async fn read_dir(mut self) -> io::Result<ReadDir> {
        let fsys = self.fsys.clone();
        let fid = mem::take(&mut self.fid);
        mem::forget(self);

        let file = File { fsys, fid };
        let qid = file.fsys.open(&file.fid).await?;

        if qid.type_ & QTDIR != QTDIR {
            return Err(io::ErrorKind::NotADirectory.into());
        }

        Ok(ReadDir { offset: 0, file, buffer: VecDeque::new() })
    }
}

impl ReadDir {
    pub async fn stat(&self) -> io::Result<npwire::Stat> {
        self.file.stat().await
    }

    pub async fn next_entry(&mut self) -> io::Result<Option<Stat>> {
        if let Some(stat) = self.buffer.pop_front() {
            return Ok(Some(stat));
        }
        
        let mut data = self.file.read_at(u32::MAX, self.offset).await?;
        self.offset += data.len() as u64;
        
        while !data.is_empty() {
            let len = u16::from_le_bytes(data[..2].try_into().map_err(io::Error::other)?);
            let stat = data.split_to(usize::from(len)+2);
            let stat = yank_stat(stat, !0).map_err(io::Error::other)?;
            self.buffer.push_back(stat);
        }

        Ok(self.buffer.pop_front())
    }

    pub fn rewind(&mut self) {
        self.offset = 0;
        self.buffer.clear();
    }
}