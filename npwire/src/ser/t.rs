use super::*;

impl Tversion {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(self.version.len() + 7);
        buf.put_u8(TypeId::Tversion.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.msize);
        put_string(&mut buf, &self.version)?;
        Ok(buf.freeze())
    }
}

impl Tauth {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(7 + self.uname.len() + self.aname.len());
        buf.put_u8(TypeId::Tauth.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.afid);
        put_string(&mut buf, &self.uname)?;
        put_string(&mut buf, &self.aname)?;
        Ok(buf.freeze())
    }
}

impl Tflush {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(5);
        buf.put_u8(TypeId::Tflush.into());
        buf.put_u16_le(tag);
        buf.put_u16_le(self.oldtag);
        Ok(buf.freeze())
    }
}

impl Tattach {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(11 + self.uname.len() + self.aname.len());
        buf.put_u8(TypeId::Tattach.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        buf.put_u32_le(self.afid);
        put_string(&mut buf, &self.uname)?;
        put_string(&mut buf, &self.aname)?;
        Ok(buf.freeze())
    }
}

impl Twalk {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(13 + self.wname.iter().map(|s| s.len() + 2).sum::<usize>());
        buf.put_u8(TypeId::Twalk.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        buf.put_u32_le(self.newfid);
        buf.put_u16_le(self.wname.len().try_into().map_err(|_| SerializeError)?);
        for name in &self.wname {
            put_string(&mut buf, name)?;
        }
        Ok(buf.freeze())
    }
}

impl Topen {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(8);
        buf.put_u8(TypeId::Topen.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        buf.put_u8(self.mode);
        Ok(buf.freeze())
    }
}

impl Tcreate {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(13 + self.name.len());
        buf.put_u8(TypeId::Tcreate.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        put_string(&mut buf, &self.name)?;
        buf.put_u32_le(self.perm);
        buf.put_u8(self.mode);
        Ok(buf.freeze())
    }
}

impl Tread {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(16);
        buf.put_u8(TypeId::Tread.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        buf.put_u64_le(self.offset);
        buf.put_u32_le(self.count);
        Ok(buf.freeze())
    }
}

impl Twrite {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(16 + self.data.len());
        buf.put_u8(TypeId::Twrite.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        buf.put_u64_le(self.offset);
        buf.put_u32_le(self.data.len().try_into().map_err(|_| SerializeError)?);
        buf.put(&self.data[..]);
        Ok(buf.freeze())
    }
}

impl Tclunk {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(7);
        buf.put_u8(TypeId::Tclunk.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        Ok(buf.freeze())
    }
}

impl Tremove {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(7);
        buf.put_u8(TypeId::Tremove.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        Ok(buf.freeze())
    }
}

impl Tstat {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(7);
        buf.put_u8(TypeId::Tstat.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        Ok(buf.freeze())
    }
}

impl Twstat {
    pub fn serialize(&self, tag: u16) -> Result<Bytes, SerializeError> {
        let mut buf = BytesMut::with_capacity(9);
        buf.put_u8(TypeId::Twstat.into());
        buf.put_u16_le(tag);
        buf.put_u32_le(self.fid);
        let lenpos = buf.len();
        buf.put_u16_le(0);
        let lenstart = buf.len();
        put_stat(&mut buf, &self.stat)?;
        
        // Update the stat size
        let statlen = buf.len() - lenstart;
        buf[lenpos..lenpos+2].copy_from_slice(&u16::try_from(statlen).map_err(|_| SerializeError)?.to_le_bytes());
        Ok(buf.freeze())
    }
}