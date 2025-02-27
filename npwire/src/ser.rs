use bytes::{BufMut, Bytes, BytesMut};
use thiserror::Error;

use crate::data::*;

mod t;
mod r;

#[derive(Debug, Clone, Copy, Error)]
#[error("serialize error")]
pub struct SerializeError;

impl From<Qid> for [u8; 13] {
    fn from(value: Qid) -> Self {
        let mut buf = [0; 13];
        buf[0] = value.type_;
        buf[1..5].copy_from_slice(&value.version.to_le_bytes());
        buf[5..13].copy_from_slice(&value.path.to_le_bytes());
        buf
    }
}

fn put_string(buf: &mut impl BufMut, str: &str) -> Result<(), SerializeError> {
    buf.put_u16_le(str.len().try_into().map_err(|_| SerializeError)?);
    buf.put(str.as_bytes());
    Ok(())
}

pub fn put_stat(buf: &mut BytesMut, stat: &Stat) -> Result<(), SerializeError> {
    let lenpos = buf.len();
    buf.put_u16_le(0);
    let lenstart = buf.len();
    buf.put_u16_le(stat.type_);
    buf.put_u32_le(stat.dev);
    buf.put(&<[u8; 13]>::from(stat.qid)[..]);
    buf.put_u32_le(stat.mode);
    buf.put_u32_le(stat.atime);
    buf.put_u32_le(stat.mtime);
    buf.put_u64_le(stat.length);
    put_string(buf, &stat.name)?;
    put_string(buf, &stat.uid)?;
    put_string(buf, &stat.gid)?;
    put_string(buf, &stat.muid)?;
    let len = buf.len() - lenstart;

    // Yes, we have two sizes. This is spec.
    buf[lenpos..lenpos+2].copy_from_slice(&u16::try_from(len).map_err(|_| SerializeError)?.to_le_bytes());
    Ok(())
}