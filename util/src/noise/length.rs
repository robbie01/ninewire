use std::io;

use bytes::{Buf as _, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

pub struct SixteenBitDelimitedCodec;

impl<'a> Encoder<&'a [u8]> for SixteenBitDelimitedCodec {
    type Error = io::Error;

    fn encode(&mut self, item: &'a [u8], dst: &mut BytesMut) -> io::Result<()> {
        if item.len() > usize::from(u16::MAX) {
            return Err(io::Error::other("too long"));
        }
        
        dst.put_u16(item.len() as u16);
        dst.put_slice(item);
        Ok(())
    }
}

impl Decoder for SixteenBitDelimitedCodec {
    type Error = io::Error;
    type Item = BytesMut;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        if src.len() < 2 { return Ok(None); }
        let n = usize::from(u16::from_be_bytes(src[..2].try_into().unwrap()));
        if src.len() < (n + 2) { return Ok(None); }
        src.advance(2);
        Ok(Some(src.split_to(n)))
    }
}