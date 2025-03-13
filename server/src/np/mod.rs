use std::{convert::Infallible, io, sync::Arc};

use tokio::{net::TcpListener, task::{id, Id, JoinSet}};
pub use traits::Serve;

pub mod traits;
mod client;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fid {
    connection_id: Id,
    fid: u32
}

impl Fid {
    const fn new(connection_id: Id, fid: u32) -> Self {
        Self { connection_id, fid }
    }
}

impl traits::Fid for Fid {
    fn is_nofid(self) -> bool {
        self.fid == !0
    }
}

const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];

pub async fn serve<S: Serve<Fid>>(
    handler: Arc<S>,
    listener: TcpListener,
) -> io::Result<Infallible> {
    let mut conns = JoinSet::new();

    loop {
        tokio::select! {
            res = listener.accept() => {
                let (peer, addr) = res?;
                let handler = handler.clone();
                conns.spawn(async move {
                    let id = id();
                    eprintln!("conn from {addr}, task id {id}");
                    match client::handle_client(peer, id, handler).await {
                        Ok(()) => eprintln!("disconnect {addr}"),
                        Err(e) => eprintln!("disconnect {addr} with error {e}"),
                    }
                });
            },
            Some(res) = conns.join_next_with_id() => {
                let id = match res {
                    Ok((id, _)) => id,
                    Err(ref e) => e.id()
                };

                if let Err(e) = res {
                    eprintln!("{e}")
                }

                handler.clunk_where(|fid| fid.connection_id == id).await;
            }
        }
    }
}