use std::num::NonZeroU8;

use bytes::{Buf, BufMut, Bytes, BytesMut, TryGetError};

const EXT_SELECTIVE_ACK: NonZeroU8 = NonZeroU8::new(1).unwrap();

#[derive(Debug, Clone)]
pub struct Header {
    pub type_: u8,
    pub ver: u8,
    pub connection_id: u16,
    pub timestamp_us: u32,
    pub timestamp_delta_us: u32,
    pub wnd_size: u32,
    pub seq_nr: u16,
    pub ack_nr: u16,
    pub extensions: Vec<Extension>
}

#[derive(Debug, Clone)]
pub struct SelectiveAck {
    pub bitmask: Vec<u32>
}

#[derive(Debug, Clone)]
pub enum Extension {
    SelectiveAck(SelectiveAck),
    Unknown(NonZeroU8, Bytes)
}

impl Extension {
    pub fn type_(&self) -> NonZeroU8 {
        match *self {
            Self::SelectiveAck(..) => EXT_SELECTIVE_ACK,
            Self::Unknown(type_, ..) => type_
        }
    }
}

pub fn yank_header(buf: &mut impl Buf) -> Result<Header, TryGetError> {
    let tv = buf.try_get_u8()?;
    let type_ = tv >> 4;
    let ver = tv & 0xF;
    let mut extension = buf.try_get_u8()?;
    let connection_id = buf.try_get_u16()?;
    let timestamp_us = buf.try_get_u32()?;
    let timestamp_delta_us = buf.try_get_u32()?;
    let wnd_size = buf.try_get_u32()?;
    let seq_nr = buf.try_get_u16()?;
    let ack_nr = buf.try_get_u16()?;
    let mut extensions = Vec::new();

    while let Some(ext) = NonZeroU8::new(extension) {
        extension = buf.try_get_u8()?;
        let len = buf.try_get_u8()?.into();
        if ext == EXT_SELECTIVE_ACK && len >= 4 && len % 4 == 0 {
            let mut bitmask = Vec::with_capacity(len/4);
            for _ in 0..len/4 {
                bitmask.push(buf.try_get_u32()?);
            }
            extensions.push(Extension::SelectiveAck(SelectiveAck { bitmask }));
        } else {
            if buf.remaining() < len {
                return Err(TryGetError { requested: len, available: buf.remaining() });
            }
            extensions.push(Extension::Unknown(ext, buf.copy_to_bytes(len)));
        }
    }

    Ok(Header {
        type_,
        ver,
        connection_id,
        timestamp_us,
        timestamp_delta_us,
        wnd_size,
        seq_nr,
        ack_nr,
        extensions
    })
}

pub fn put_header(hdr: &Header, buf: &mut impl BufMut) {
    let tv = (hdr.type_ << 4) | (hdr.ver & 0xF);
    buf.put_u8(tv);
    buf.put_u8(hdr.extensions.first().map(Extension::type_).map_or(0, Into::into));
    buf.put_u16(hdr.connection_id);
    buf.put_u32(hdr.timestamp_us);
    buf.put_u32(hdr.timestamp_delta_us);
    buf.put_u32(hdr.wnd_size);
    buf.put_u16(hdr.seq_nr);
    buf.put_u16(hdr.ack_nr);

    for (i, ext) in hdr.extensions.iter().enumerate() {
        let extension = hdr.extensions.get(i+1).map(Extension::type_).map_or(0, Into::into);
        let data = match ext {
            Extension::SelectiveAck(SelectiveAck { bitmask }) => {
                let mut bitbuf = BytesMut::with_capacity(4 * bitmask.len());
                for &z in bitmask {
                    bitbuf.put_u32(z);
                }
                bitbuf.freeze()
            },
            Extension::Unknown(_, data) => data.clone()
        };
        buf.put_u8(extension);
        buf.put_u8(data.len().try_into().unwrap()); // TODO
        buf.put_slice(&data);
    }
}