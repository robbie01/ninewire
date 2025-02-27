use std::{convert::Infallible, io, sync::Arc};

use tokio::{net::TcpListener, task::{id, Id}};
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

pub async fn serve<S: Serve<Fid> + 'static>(
    handler: Arc<S>,
    listener: TcpListener,
) -> io::Result<Infallible> {
    loop {
        let (peer, addr) = listener.accept().await?;
        eprintln!("conn from {addr}");

        let handler = handler.clone();
        tokio::spawn(async move {
            let id = id();
            match client::handle_client(peer, id, handler.clone()).await {
                Ok(_) => eprintln!("disconnect {addr}"),
                Err(e) => eprintln!("disconnect {addr} with error {e}")
            }
            handler.clunk_where(|fid| fid.connection_id == id).await;
        });
    }
}