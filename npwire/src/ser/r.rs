use super::*;

impl Rversion {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(self.version.len()+3);
        buf.put_u8(TypeId::Rversion.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.msize);
        put_string(&mut buf, &self.version)?;
        Ok(buf.freeze())
    }
}

impl Rflush {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(3);
        buf.put_u8(TypeId::Rflush.into());
        buf.put_u16_le(tag);
        Ok(buf.freeze())
    }
}

impl Rwalk {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(5 + self.wqid.len() * 13);
        buf.put_u8(TypeId::Rwalk.into());
        buf.put_u16_le(tag);
        buf.put_u16_le(self.wqid.len().try_into().map_err(|_| SerializeError)?);
        for qid in &self.wqid {
            buf.put_slice(&<[u8; 13]>::from(*qid));
        }
        Ok(buf.freeze())
    }
}

impl Rread {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(7 + self.data.len());
        buf.put_u8(TypeId::Rread.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.data.len().try_into().map_err(|_| SerializeError)?);
        buf.put(&self.data[..]);
        Ok(buf.freeze())
    }
}

impl Rreads {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(15 + self.data.len());
        buf.put_u8(TypeId::Rreads.into());
        buf.put_u16_le(tag);
        buf.put_u64_le(self.offset);
        buf.put_u32_le(self.data.len().try_into().map_err(|_| SerializeError)?);
        buf.put(&self.data[..]);
        Ok(buf.freeze())
    }
}

impl Rwrite {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(7);
        buf.put_u8(TypeId::Rwrite.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.count);
        Ok(buf.freeze())
    }
}

impl Rclunk {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(3);
        buf.put_u8(TypeId::Rclunk.into());
        buf.put_u16_le(tag);
        Ok(buf.freeze())
    }
}

impl Rremove {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(3);
        buf.put_u8(TypeId::Rremove.into());
        buf.put_u16_le(tag);
        Ok(buf.freeze())
    }
}

impl Rauth {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(16);
        buf.put_u8(TypeId::Rauth.into());
        buf.put_u16_le(tag);
        buf.put_slice(&<[u8; 13]>::from(self.aqid));
        Ok(buf.freeze())
    }
}

impl Rattach {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(16);
        buf.put_u8(TypeId::Rattach.into());
        buf.put_u16_le(tag);
        buf.put_slice(&<[u8; 13]>::from(self.qid));
        Ok(buf.freeze())
    }
}

impl Ropen {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(20);
        buf.put_u8(TypeId::Ropen.into());
        buf.put_u16_le(tag);
        buf.put_slice(&<[u8; 13]>::from(self.qid));
        buf.put_u32_le(self.iounit);
        Ok(buf.freeze())
    }
}

impl Rcreate {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(20);
        buf.put_u8(TypeId::Rcreate.into());
        buf.put_u16_le(tag);
        buf.put_slice(&<[u8; 13]>::from(self.qid));
        buf.put_u32_le(self.iounit);
        Ok(buf.freeze())
    }
}

impl Rstat {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(16); // idk just guessin
        buf.put_u8(TypeId::Rstat.into());
        buf.put_u16_le(tag);
        let lenpos = buf.len();
        buf.put_u16_le(0);
        let lenstart=  buf.len();
        put_stat(&mut buf, &self.stat)?;

        // Yes, we have two sizes. This is spec.
        let statlen = buf.len() - lenstart;
        buf[lenpos..lenpos+2].copy_from_slice(&u16::try_from(statlen).map_err(|_| SerializeError)?.to_le_bytes());
        Ok(buf.freeze())
    }
}

impl Rwstat {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(3);
        buf.put_u8(TypeId::Rwstat.into());
        buf.put_u16_le(tag);
        Ok(buf.freeze())
    }
}

impl Rerror {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(5+self.ename.len());
        buf.put_u8(TypeId::Rerror.into());
        buf.put_u16_le(tag);
        put_string(&mut buf, &self.ename)?;
        Ok(buf.freeze())
    }
}

impl RMessage {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        match self {
            RMessage::Rreads(v) => v.serialize(tag),
            RMessage::Rversion(v) => v.serialize(tag),
            RMessage::Rauth(v) => v.serialize(tag),
            RMessage::Rflush(v) => v.serialize(tag),
            RMessage::Rattach(v) => v.serialize(tag),
            RMessage::Rwalk(v) => v.serialize(tag),
            RMessage::Ropen(v) => v.serialize(tag),
            RMessage::Rcreate(v) => v.serialize(tag),
            RMessage::Rread(v) => v.serialize(tag),
            RMessage::Rwrite(v) => v.serialize(tag),
            RMessage::Rclunk(v) => v.serialize(tag),
            RMessage::Rremove(v) => v.serialize(tag),
            RMessage::Rstat(v) => v.serialize(tag),
            RMessage::Rwstat(v) => v.serialize(tag),
            RMessage::Rerror(v) => v.serialize(tag),
        }
    }
}