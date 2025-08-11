// mod v2;

use std::{io, net::SocketAddr, sync::Arc};

use scc::Bag;
use snow::StatelessTransportState;
use tracing::trace;
use udt::{Connection, Endpoint};

#[derive(Debug)]
pub struct SecureTransport {
    inner: Connection,
    crypto: StatelessTransportState,
    buffers: Bag<Vec<u8>>
}

#[derive(Debug)]
pub struct SendHalf {
    inner: Arc<SecureTransport>,
    nonce: u64
}

#[derive(Debug)]
pub struct RecvHalf {
    inner: Arc<SecureTransport>,
    nonce: u64
}

#[derive(Debug, Clone, Copy)]
pub enum Side<'a> {
    Initiator { remote_public_key: &'a [u8] },
    Responder { local_private_key: &'a [u8] }
}

impl SecureTransport {
    // Feature: add support for 0/0.5 RTT data (e.g. a fast Tversion/Rversion)
    // Of course, fast-open is a pipe dream considering the very nature of rendezvous sockets.
    pub async fn connect(ep: &Arc<Endpoint>, addr: SocketAddr, side: Side<'_>) -> io::Result<(SendHalf, RecvHalf)> {
        let inner = ep.connect_datagram(addr, true).await?;
        // TODO: negotiate AES for accelerated hosts, and ChaChaPoly otherwise
        let crypto = snow::Builder::new("Noise_NK_25519_AESGCM_SHA256".parse().unwrap());
        let mut crypto = match side {
            Side::Initiator { remote_public_key } => crypto
                .remote_public_key(remote_public_key).map_err(io::Error::other)?
                .build_initiator().unwrap(),
            Side::Responder { local_private_key } => crypto
                .local_private_key(local_private_key).map_err(io::Error::other)?
                .build_responder().unwrap()
        };

        // Reasonable yet lean buffer size for pure handshake messages
        let mut buf = [0; 64];
        while !crypto.is_handshake_finished() {
            if crypto.is_my_turn() {
                let n = crypto.write_message(&[], &mut buf).map_err(io::Error::other)?;
                inner.send(&buf[..n]).await?;
            } else {
                let n = inner.recv(&mut buf).await?;
                crypto.read_message(&buf[..n], &mut []).map_err(io::Error::other)?;
            }
        }

        let sec = Arc::new(Self {
            inner,
            crypto: crypto.into_stateless_transport_mode().map_err(io::Error::other)?,
            buffers: Bag::new()
        });
        Ok((
            SendHalf { inner: sec.clone(), nonce: 0 },
            RecvHalf { inner: sec, nonce: 0 }
        ))
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }
}

impl Drop for SecureTransport {
    fn drop(&mut self) {
        trace!(name: "closed", num_buffers = self.buffers.len());
    }
}

impl SendHalf {
    pub fn inner(&self) -> &SecureTransport { return &self.inner; }

    pub async fn send(&mut self, buf: impl AsRef<[u8]>) -> io::Result<usize> {
        if self.nonce == u64::MAX {
            return Err(io::Error::other("too many messages"));
        }

        let buf = buf.as_ref();

        let nonce = self.nonce;
        self.nonce += 1;

        let mut tmp = self.inner.buffers.pop().unwrap_or_default();
        let tgt = buf.len() + 16;
        if tmp.len() < tgt {
            tmp.resize(tgt, 0);
        }

        let n = self.inner.crypto.write_message(nonce, buf, &mut tmp[..]).map_err(io::Error::other)?;
        assert_eq!(n, tgt);

        let n = self.inner.inner.send_with(&tmp[..tgt], None, true).await?;
        assert_eq!(n, tgt);

        self.inner.buffers.push(tmp);

        Ok(buf.len())
    }

    pub fn flush(&self) -> impl Future<Output = io::Result<()>> {
        self.inner.inner.flush()
    }
}

impl RecvHalf {
    pub fn inner(&self) -> &SecureTransport { return &self.inner; }

    pub async fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.nonce == u64::MAX {
            return Err(io::Error::other("too many messages"));
        }

        let mut tmp = self.inner.buffers.pop().unwrap_or_default();
        let tgt = buf.len() + 16;
        if tmp.len() < tgt {
            tmp.resize(tgt, 0);
        }

        let n = self.inner.inner.recv(&mut tmp).await?;

        let res = self.inner.crypto.read_message(self.nonce, &tmp[..n], buf).map_err(io::Error::other);
        self.inner.buffers.push(tmp);
        let res = res?;
        self.nonce += 1;
        Ok(res)
    }
}