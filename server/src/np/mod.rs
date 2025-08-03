use std::{fmt::Debug, io, sync::Arc};

use futures::{TryStream, TryStreamExt as _};
use tokio::{io::{AsyncRead, AsyncWrite}, task::{id, JoinSet}};
use traits::Serve;

pub mod traits;
mod client;

const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];

pub async fn serve_mux<
    I: AsyncRead + AsyncWrite + Send + 'static,
    A: Debug + Send + 'static,
    S: Serve,
    L: TryStream<Ok = (I, A), Error = io::Error> + Unpin
>(handler: Arc<S>, mut listener: L) -> io::Result<()> {
    let mut conns = JoinSet::new();

    loop {
        tokio::select! {
            biased;
            Some(res) = conns.join_next_with_id() => {
                if let Err(e) = res {
                    eprintln!("{e}");
                }
            },
            res = listener.try_next() => {
                let Some((peer, addr)) = res? else { break };
                let handler = handler.clone();
                conns.spawn(async move {
                    let id = id();
                    eprintln!("conn from {addr:?}, task id {id}");
                    match client::handle_client(peer, handler).await {
                        Ok(()) => eprintln!("disconnect {addr:?}"),
                        Err(e) => eprintln!("disconnect {addr:?} with error {e}"),
                    }
                });
            }
        }
    }

    Ok(())
}