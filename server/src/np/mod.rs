use std::{convert::Infallible, io, sync::Arc};

use tokio::{net::TcpListener, task::{id, JoinSet}};
use traits::Serve;

pub mod traits;
mod client;

const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];

pub async fn serve_mux<S: Serve>(
    handler: Arc<S>,
    listener: TcpListener,
) -> io::Result<Infallible> {
    let mut conns = JoinSet::new();

    loop {
        tokio::select! {
            biased;
            Some(res) = conns.join_next_with_id() => {
                if let Err(e) = res {
                    eprintln!("{e}")
                }
            },
            res = listener.accept() => {
                let (peer, addr) = res?;
                let handler = handler.clone();
                conns.spawn(async move {
                    let id = id();
                    eprintln!("conn from {addr}, task id {id}");
                    match client::handle_client(peer, handler).await {
                        Ok(()) => eprintln!("disconnect {addr}"),
                        Err(e) => eprintln!("disconnect {addr} with error {e}"),
                    }
                });
            }
        }
    }
}