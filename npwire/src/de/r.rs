use super::*;

impl Rversion {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let msize = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let version = yank_string(&mut buf, tag)?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { msize, version })
    }
}

impl Rauth {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let mut aqid = [0; 13];
        buf.try_copy_to_slice(&mut aqid).map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let aqid = aqid.into();
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { aqid })
    }
}

impl Rerror {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let ename = yank_string(&mut buf, tag)?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { ename })
    }
}

impl Rflush {
    fn deserialize(buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self)
    }
}

impl Rattach {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let mut qid = [0; 13];
        buf.try_copy_to_slice(&mut qid).map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let qid = qid.into();
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { qid })
    }
}

impl Rwalk {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let nwqid = buf.try_get_u16_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?.into();
        let mut wqid = Vec::with_capacity(nwqid);
        for _ in 0..nwqid {
            let mut qid = [0; 13];
            buf.try_copy_to_slice(&mut qid).map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
            let qid = qid.into();
            wqid.push(qid);
        }
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { wqid })
    }
}

impl Ropen {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let mut qid = [0; 13];
        buf.try_copy_to_slice(&mut qid).map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let qid = qid.into();
        let iounit = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { qid, iounit })
    }
}

impl Rcreate {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let mut qid = [0; 13];
        buf.try_copy_to_slice(&mut qid).map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        let qid = qid.into();
        let iounit = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { qid, iounit })
    }
}

impl Rread {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let count = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })? as usize;
        if buf.len() < count {
            return Err(DeserializeError::TooShort { tag: Some(tag) });
        }
        let data = if count == 0 {
            Bytes::new()
        } else {
            buf.split_to(count)
        };
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { data })
    }
}

impl Rwrite {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let count = buf.try_get_u32_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { count })
    }
}

impl Rclunk {
    fn deserialize(buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self)
    }
}

impl Rremove {
    fn deserialize(buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self)
    }
}

impl Rstat {
    fn deserialize(mut buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        let stat_len = buf.try_get_u16_le().map_err(|_| DeserializeError::TooShort { tag: Some(tag) })?.into();
        if buf.len() < stat_len {
            return Err(DeserializeError::TooShort { tag: Some(tag) });
        }
        let stat = yank_stat(&mut buf, tag)?;
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self { stat })
    }
}

impl Rwstat {
    fn deserialize(buf: Bytes, tag: u16) -> Result<Self, DeserializeError> {
        if !buf.is_empty() {
            return Err(DeserializeError::TooLong { tag });
        }
        Ok(Self)
    }
}

/* NOTE: buf should not have a length prefix */
pub fn deserialize_r(mut buf: Bytes) -> Result<(u16, RMessage), DeserializeError> {
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
        TypeId::Rversion => Rversion::deserialize(buf, tag)?.into(),
        TypeId::Rauth => Rauth::deserialize(buf, tag)?.into(),
        TypeId::Rerror => Rerror::deserialize(buf, tag)?.into(),
        TypeId::Rflush => Rflush::deserialize(buf, tag)?.into(),
        TypeId::Rattach => Rattach::deserialize(buf, tag)?.into(),
        TypeId::Rwalk => Rwalk::deserialize(buf, tag)?.into(),
        TypeId::Ropen => Ropen::deserialize(buf, tag)?.into(),
        TypeId::Rcreate => Rcreate::deserialize(buf, tag)?.into(),
        TypeId::Rread => Rread::deserialize(buf, tag)?.into(),
        TypeId::Rwrite => Rwrite::deserialize(buf, tag)?.into(),
        TypeId::Rclunk => Rclunk::deserialize(buf, tag)?.into(),
        TypeId::Rremove => Rremove::deserialize(buf, tag)?.into(),
        TypeId::Rstat => Rstat::deserialize(buf, tag)?.into(),
        TypeId::Rwstat => Rwstat::deserialize(buf, tag)?.into(),
        _ => return Err(DeserializeError::UnsupportedType { type_, tag })
    }))
}