use std::{collections::{HashSet, VecDeque}, io, net::{SocketAddr, SocketAddrV6}, sync::{Arc, Mutex, Weak}, time::SystemTime};

use bytes::{Bytes, BytesMut};
use ringbuf::{storage::Heap, traits::{Observer, Producer}, LocalRb};
use tokio::net::UdpSocket;
use weak_table::WeakValueHashMap;
use wire::{put_header, yank_header, Header};

mod wire;

const MTU: u16 = 1280;
const MAX_OUTGOING: u16 = MTU - 48;
const MAX_UDP: u16 = 65527;

const WND_SIZE: u32 = 131072; // TODO

const ST_DATA: u8 = 0;
const ST_FIN: u8 = 1;
const ST_STATE: u8 = 2;
const ST_RESET: u8 = 3;
const ST_SYN: u8 = 4;

fn timestamp() -> u32 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_micros() as u32
}

struct EndpointState {
    socket: UdpSocket,
    connections: Mutex<WeakValueHashMap<(SocketAddrV6, u16), Weak<Mutex<ConnectionInner>>>>
}

pub struct Endpoint {
    shared: Arc<EndpointState>
}

impl Endpoint {
    pub async fn new(addr: SocketAddrV6) -> io::Result<Self> {
        let sock = UdpSocket::bind(addr).await?;
        let shared = Arc::new(EndpointState {
            socket: sock,
            connections: Mutex::default()
        });
        {
            let shared = shared.clone();
            tokio::spawn(async move {
                let mut connections_to_ack = HashSet::new();
                loop {
                    connections_to_ack.clear();
                    shared.socket.readable().await?;

                    loop {
                        let mut buffer = BytesMut::zeroed(MAX_UDP.into());
                        let (n, peer) = match shared.socket.try_recv_from(&mut buffer) {
                            Ok(k) => k,
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            e => e?
                        };
                        buffer.truncate(n);
                        let SocketAddr::V6(peer) = peer else { continue };
                        let Ok(hdr) = yank_header(&mut buffer) else { continue };
                        let key = (peer, hdr.connection_id);
                        let Some(con) = shared.connections.lock().unwrap().get(&key) else { continue };
                        if con.lock().unwrap().handle_incoming(&hdr, &buffer) {
                            connections_to_ack.insert(key);
                        }
                    }

                    for key@(peer, _) in connections_to_ack.drain() {
                        let Some(con) = shared.connections.lock().unwrap().get(&key) else { continue };
                        let ack = con.lock().unwrap().generate_ack();
                        shared.socket.send_to(&ack, peer).await?;
                    }
                }
                #[allow(unreachable_code)]
                Ok::<_, io::Error>(())
            });
        }
        Ok(Self {
            shared
        })
    }

    pub async fn connect(&self, peer: SocketAddrV6) -> io::Result<Connection> {
        todo!()
    }
}

struct ConnectionInner {
    receive_buffer: LocalRb<Heap<u8>>,
    unacked_packets: VecDeque<(u16, Bytes)>,
    seq_nr: u16, // next sequence number to be transmitted
    ack_nr: u16, // last sequence number received
    id_outgoing: u16,
    id_incoming: u16,
    last_recv_timestamp: u32,
    remote_wnd_size: u32
}

impl ConnectionInner {
    fn handle_incoming(&mut self, header: &Header, payload: &[u8]) -> bool {
        if header.seq_nr != self.ack_nr.wrapping_add(1) || header.type_ >= ST_SYN {
            // out of order, ignore for now (look into selective repeat)
            return false;
        }

        match header.type_ {
            ST_DATA => {
                if payload.len() > self.receive_buffer.vacant_len() {
                    // no space left, do not ack
                    return false;
                }

                self.receive_buffer.push_slice(&payload);
            },
            ST_FIN => todo!(),
            ST_STATE => (),
            ST_RESET => todo!(),
            _ => unreachable!()
        }

        self.last_recv_timestamp = header.timestamp_us;
        self.remote_wnd_size = header.wnd_size;
        self.unacked_packets.retain(|(seq_nr, _)| *seq_nr <= header.ack_nr);
        self.ack_nr = header.seq_nr;

        header.type_ != ST_STATE
    }

    fn generate_ack(&mut self) -> Bytes {
        let seq_nr = self.seq_nr;
        let ts = timestamp();

        let hdr = Header {
            type_: ST_STATE,
            ver: 1,
            connection_id: self.id_outgoing,
            timestamp_us: ts,
            timestamp_delta_us: ts.wrapping_sub(self.last_recv_timestamp),
            wnd_size: self.receive_buffer.vacant_len() as u32,
            seq_nr: self.seq_nr,
            ack_nr: self.ack_nr,
            extensions: vec![]
        };
        let mut buf = BytesMut::new();
        put_header(&hdr, &mut buf);
        let buf = buf.freeze();
        self.unacked_packets.push_back((seq_nr, buf.clone()));
        buf
    }
}

pub struct Connection {
    shared: Arc<EndpointState>,
    inner: Arc<Mutex<ConnectionInner>>
}