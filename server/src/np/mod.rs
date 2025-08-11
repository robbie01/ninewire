use std::{fmt::Debug, sync::Arc};

use futures::{TryStream, TryStreamExt as _};
use tokio::task::{id, JoinSet};
use traits::Serve;
use transport::{RecvHalf, SendHalf};

pub mod traits;
mod client;

pub async fn serve_mux<
    A: Debug + Send + 'static,
    S: Serve,
    L: TryStream<Ok = ((SendHalf, RecvHalf), A)> + Unpin
>(handler: Arc<S>, mut listener: L) -> Result<(), L::Error> {
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