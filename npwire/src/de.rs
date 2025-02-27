use std::cmp::Ordering;

use bytes::{Buf as _, Bytes};
use bytestring::ByteString;
use thiserror::Error;

use crate::*;

mod t;
mod r;

pub use t::*;
pub use r::*;

#[derive(Debug, Clone, Copy, Error)]
pub enum DeserializeError {
    #[error("too short")]
    TooShort {
        tag: Option<u16>
    },
    #[error("too long")]
    TooLong {
        tag: u16
    },
    #[error("unknown type id {type_}")]
    UnknownType {
        type_: u8,
        tag: u16
    },
    #[error("cannot deserialize {type_:?}")]
    UnsupportedType {
        type_: TypeId,
        tag: u16
    },
    #[error("Invalid UTF-8 string")]
    InvalidUTF8 { tag: u16 }
}

impl DeserializeError {
    pub fn tag(self) -> Option<u16> {
        match self {
            DeserializeError::TooShort { tag } => tag,
            DeserializeError::TooLong { tag } => Some(tag),
            DeserializeError::UnknownType { tag, .. } => Some(tag),
            DeserializeError::UnsupportedType { tag, .. } => Some(tag),
            DeserializeError::InvalidUTF8 { tag } => Some(tag),
        }
    }
}

impl From<[u8; 13]> for Qid {
    fn from(value: [u8; 13]) -> Self {
        Self {
            type_: value[0],
            version: u32::from_le_bytes(value[1..5].try_into().unwrap()),
            path: u64::from_le_bytes(value[5..13].try_into().unwrap())
        }
    }
}

fn yank_string(buf: &mut Bytes, tag: u16) -> Result<ByteString, DeserializeError> {
    let len = buf.try_get_u16_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?.into();
    if buf.remaining() < len {
        return Err(DeserializeError::TooShort { tag: Some(tag) });
    }
    let str = ByteString::try_from(buf.split_to(len)).map_err(|_| DeserializeError::InvalidUTF8 { tag })?;
    Ok(str)
}

pub fn yank_stat(mut buf: Bytes, tag: u16) -> Result<Stat, DeserializeError> {
    let len = buf.try_get_u16_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?.into();
    if buf.remaining() < len {
        return Err(DeserializeError::TooShort { tag: Some(tag) });
    }

    let type_ = buf.try_get_u16_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
    let dev = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
    let mut qid = [0; 13];
    if buf.remaining() < 13 {
        return Err(DeserializeError::TooShort { tag: Some(tag) });
    }
    buf.copy_to_slice(&mut qid);
    let qid = Qid::from(qid);
    let mode = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
    let atime = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
    let mtime = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
    let length = buf.try_get_u64_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
    let name = yank_string(&mut buf, tag)?[..].into();
    let uid = yank_string(&mut buf, tag)?[..].into();
    let gid = yank_string(&mut buf, tag)?[..].into();
    let muid = yank_string(&mut buf, tag)?[..].into();

    if !buf.is_empty() {
        return Err(DeserializeError::TooLong { tag });
    }
    
    Ok(Stat {
        type_, dev, qid, mode, atime, mtime,
        length, name, uid, gid, muid
    })
}