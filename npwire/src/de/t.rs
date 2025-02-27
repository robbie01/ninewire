use super::*;

impl Tversion {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let msize = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let version = yank_string(&mut buf, tag)?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self {
            msize,
            version
        })
    }
}

impl Tflush {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let oldtag = buf.try_get_u16_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { oldtag })
    }
}

impl Twalk {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let newfid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let nwname = buf.try_get_u16_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?.into();
        let mut wname = Vec::with_capacity(nwname);
        for _ in 0..nwname {
            wname.push(yank_string(&mut buf, tag)?);
        }
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { fid, newfid, wname })
    }
}

impl Tread {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let offset = buf.try_get_u64_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let count = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { fid, offset, count })
    }
}

impl Twrite {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let offset = buf.try_get_u64_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let count = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })? as usize;

        match buf.len().cmp(&count) {
            Ordering::Less => Err(DeserializeError::TooShort { tag: Some(tag) }),
            Ordering::Greater => Err(DeserializeError::TooLong { tag }),
            Ordering::Equal => Ok(Self { fid, offset, data: buf })
        }
    }
}

impl Tclunk {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { fid })
    }
}

impl Tremove {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { fid })
    }
}

impl Tauth {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let afid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let uname = yank_string(&mut buf, tag)?;
        let aname = yank_string(&mut buf, tag)?;
        Ok(Self { afid, uname, aname })
    }
}

impl Tattach {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let afid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let uname = yank_string(&mut buf, tag)?;
        let aname = yank_string(&mut buf, tag)?;
        Ok(Self { fid, afid, uname, aname })
    }
}

impl Topen {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let mode = buf.try_get_u8().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { fid, mode })
    }
}

impl Tcreate {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let name = yank_string(&mut buf, tag)?;
        let perm = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let mode = buf.try_get_u8().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { fid, name, perm, mode })
    }
}

impl Tstat {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { fid })
    }
}

impl Twstat {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let fid = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let stat_len = buf.try_get_u16_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?.into();
        if buf.len() < stat_len {
            return Err(DeserializeError::TooShort { tag: Some(tag) });
        }
        let stat = yank_stat(buf.split_to(stat_len), tag)?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { fid, stat })
    }
}

#[derive(Debug, Clone)]
pub enum TMessage {
    Tversion(Tversion),
    Tauth(Tauth),
    Tflush(Tflush),
    Tattach(Tattach),
    Twalk(Twalk),
    Topen(Topen),
    Tcreate(Tcreate),
    Tread(Tread),
    Twrite(Twrite),
    Tclunk(Tclunk),
    Tremove(Tremove),
    Tstat(Tstat),
    Twstat(Twstat),
}

impl From<Tversion> for TMessage {
    fn from(value: Tversion) -> Self {
        Self::Tversion(value)
    }
}

impl From<Tauth> for TMessage {
    fn from(value: Tauth) -> Self {
        Self::Tauth(value)
    }
}

impl From<Tflush> for TMessage {
    fn from(value: Tflush) -> Self {
        Self::Tflush(value)
    }
}

impl From<Tattach> for TMessage {
    fn from(value: Tattach) -> Self {
        Self::Tattach(value)
    }
}

impl From<Twalk> for TMessage {
    fn from(value: Twalk) -> Self {
        Self::Twalk(value)
    }
}

impl From<Topen> for TMessage {
    fn from(value: Topen) -> Self {
        Self::Topen(value)
    }
}

impl From<Tcreate> for TMessage {
    fn from(value: Tcreate) -> Self {
        Self::Tcreate(value)
    }
}

impl From<Tread> for TMessage {
    fn from(value: Tread) -> Self {
        Self::Tread(value)
    }
}

impl From<Twrite> for TMessage {
    fn from(value: Twrite) -> Self {
        Self::Twrite(value)
    }
}

impl From<Tclunk> for TMessage {
    fn from(value: Tclunk) -> Self {
        Self::Tclunk(value)
    }
}

impl From<Tremove> for TMessage {
    fn from(value: Tremove) -> Self {
        Self::Tremove(value)
    }
}

impl From<Tstat> for TMessage {
    fn from(value: Tstat) -> Self {
        Self::Tstat(value)
    }
}

impl From<Twstat> for TMessage {
    fn from(value: Twstat) -> Self {
        Self::Twstat(value)
    }
}

/* NOTE: buf should not have a length prefix */
pub fn deserialize_t(mut buf: Bytes) -> Result<(u16, TMessage), DeserializeError> {
    let type_ = TypeId::try_from(
        buf.try_get_u8().map_err(|_| DeserializeError::TooShort { tag: None })?
    );

    let tag = buf.try_get_u16_le().ok();

    let (type_, tag) = match (type_, tag) {
        (Ok(ty), Some(t)) => (ty, t),
        (Err(type_), Some(tag)) => return Err(DeserializeError::UnknownType { type_, tag }),
        (_, None) => return Err(DeserializeError::TooShort { tag: None })
    };

    Ok((tag, match type_ {
        TypeId::Tversion => Tversion::deserialize(buf, tag)?.into(),
        TypeId::Tflush => Tflush::deserialize(buf, tag)?.into(),
        TypeId::Twalk => Twalk::deserialize(buf, tag)?.into(),
        TypeId::Tread => Tread::deserialize(buf, tag)?.into(),
        TypeId::Twrite => Twrite::deserialize(buf, tag)?.into(),
        TypeId::Tclunk => Tclunk::deserialize(buf, tag)?.into(),
        TypeId::Tremove => Tremove::deserialize(buf, tag)?.into(),
        TypeId::Tauth => Tauth::deserialize(buf, tag)?.into(),
        TypeId::Tattach => Tattach::deserialize(buf, tag)?.into(),
        TypeId::Topen => Topen::deserialize(buf, tag)?.into(),
        TypeId::Tcreate => Tcreate::deserialize(buf, tag)?.into(),
        TypeId::Tstat => Tstat::deserialize(buf, tag)?.into(),
        TypeId::Twstat => Twstat::deserialize(buf, tag)?.into(),
        _ => return Err(DeserializeError::UnsupportedType { type_, tag })
    }))
}