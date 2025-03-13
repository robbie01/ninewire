use std::io;

use bytes::{Buf as _, BufMut as _, BytesMut};
use snow::TransportState;
use tokio_util::codec::{Decoder, Encoder};

pub struct NoiseCodec {
    st: TransportState
}

impl NoiseCodec {
    pub fn new(st: TransportState) -> Self {
        Self { st }
    }

    pub fn get_remote_static(&self) -> Option<&[u8]> {
        self.st.get_remote_static()
    }
}

impl<'a> Encoder<&'a [u8]> for NoiseCodec {
    type Error = io::Error;

    fn encode(&mut self, mut item: &'a [u8], dst: &mut BytesMut) -> io::Result<()> {
        while !item.is_empty() {
            let buf = &item[..item.len().min(super::MAX_PAYLOAD)];
            item = &item[buf.len()..];

            let n = item.len() + usize::from(super::TAG_LEN);
            dst.put_u16(n as u16);
            let pos = dst.len();
            dst.put_bytes(0, n.into());
            let n2 = self.st.write_message(item, &mut dst[pos..]).map_err(io::Error::other)?;
            assert_eq!(usize::from(n), n2);
        }
        Ok(())
    }
}

impl Decoder for NoiseCodec {
    type Error = io::Error;
    type Item = BytesMut;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        if src.len() < 2 { return Ok(None); }
        let n = usize::from(u16::from_be_bytes(src[..2].try_into().unwrap()));
        if src.len() < (n + 2) { return Ok(None); }
        src.advance(2);
        let Some(n2) = n.checked_sub(super::TAG_LEN.into()) else { return Err(io::Error::other("too short")) };
        let mut pt = BytesMut::zeroed(n2.into());
        let k = self.st.read_message(&src[..n], &mut pt).map_err(io::Error::other)?;
        src.advance(n);
        assert_eq!(usize::from(n2), k);
        Ok(Some(pt))
    }
}