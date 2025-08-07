use std::{future::ready, io, net::SocketAddr, ops::RangeInclusive, sync::{atomic::{AtomicU64, Ordering}, Arc}};

use parking_lot::Mutex;
use range_set::RangeSet;
use scc::Bag;
use snow::StatelessTransportState;
use udt::{DatagramConnection, Endpoint};

#[derive(Debug)]
pub struct SecureTransport {
    inner: DatagramConnection,
    crypto: StatelessTransportState,
    buffers: Bag<Vec<u8>>,
    nonce_outgoing: AtomicU64,
    // There's a potential DoS here. A malicious sender could only use non-consecutive nonces
    // (i.e. all evens) and force the receiver to allocate a ton of memory here. We should
    // terminate the connection if there's an unreasonable number of nonce gaps.
    // This shouldn't affect well-behaved senders because we don't expire any messages.
    nonce_incoming: Mutex<RangeSet<[RangeInclusive<u64>; 1]>>
}

#[derive(Debug, Clone, Copy)]
pub enum Side<'a> {
    Initiator { remote_public_key: &'a [u8] },
    Responder { local_private_key: &'a [u8] }
}

impl SecureTransport {
    // Feature: add support for 0/0.5 RTT data (e.g. a fast Tversion/Rversion)
    // Of course, fast-open is a pipe dream considering the very nature of rendezvous sockets.
    pub async fn connect(ep: &Arc<Endpoint>, addr: SocketAddr, side: Side<'_>) -> io::Result<Self> {
        let inner = ep.connect_datagram_async(addr, true).await?;
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

        Ok(Self {
            inner,
            crypto: crypto.into_stateless_transport_mode().map_err(io::Error::other)?,
            buffers: Bag::new(),
            nonce_outgoing: AtomicU64::new(0),
            nonce_incoming: RangeSet::new().into()
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut tmp = self.buffers.pop().unwrap_or_default();
        let tgt = buf.len() + 8 + 16;
        if tmp.len() < tgt {
            tmp.resize(tgt, 0);
        }

        let (mut guard, nonce, n) = loop {
            let n = self.inner.recv(&mut tmp).await?;
            let nonce = u64::from_be_bytes(tmp[..8].try_into().unwrap());
            
            let guard = self.nonce_incoming.lock();
            if !guard.contains(nonce) {
                break (guard, nonce, n)
            }
        };

        let res = self.crypto.read_message(nonce, &tmp[8..n], buf).map_err(io::Error::other);
        self.buffers.push(tmp);
        let res = res?;
        guard.insert(nonce);
        Ok(res)
    }

    pub async fn send_with(&self, buf: &[u8], inorder: bool) -> io::Result<usize> {
        let nonce = self.nonce_outgoing.fetch_add(1, Ordering::Relaxed);

        let mut tmp = self.buffers.pop().unwrap_or_default();
        let tgt = buf.len() + 8 + 16;
        if tmp.len() < tgt {
            tmp.resize(tgt, 0);
        }

        let n = self.crypto.write_message(nonce, buf, &mut tmp[8..]).map_err(io::Error::other)?;
        assert_eq!(n, tgt - 8);

        tmp[..8].copy_from_slice(&nonce.to_be_bytes());

        // Force inorder if the nonce is 0 so that transport messages aren't reordered behind handshake messages
        let n = self.inner.send_with(&tmp[..tgt], None, inorder || nonce == 0).await?;
        assert_eq!(n, tgt);

        self.buffers.push(tmp);

        Ok(buf.len())
    }

    pub async fn send(&self, buf: impl AsRef<[u8]>) -> io::Result<usize> {
        self.send_with(buf.as_ref(), true).await
    }

    pub fn flush(&self) -> impl Future<Output = io::Result<()>> {
        // TODO: implement a proper flush (i.e. wait until UDT_SNDDATA is zero)
        ready(Ok(()))
    }
}