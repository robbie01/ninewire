use std::{fmt::Debug, rc::Rc};

use compio::runtime::spawn;
use futures::{StreamExt, TryStream, TryStreamExt as _, stream::FuturesUnordered};
use traits::Serve;
use transport::NpTransport;

pub mod traits;
mod client;

pub async fn serve_mux<
    A: Debug + Send + 'static,
    S: Serve,
    T: NpTransport + 'static,
    L: TryStream<Ok = (T, A)> + Unpin
>(handler: Rc<S>, mut listener: L) -> Result<(), L::Error> {
    let mut conns = FuturesUnordered::new();

    loop {
        tokio::select! {
            biased;
            Some(res) = conns.next() => {
                if let Err(e) = res {
                    eprintln!("{e}");
                }
            },
            res = listener.try_next() => {
                let Some((peer, addr)) = res? else { break };
                let handler = handler.clone();
                conns.push(spawn(async move {
                    eprintln!("conn from {addr:?}");
                    match client::handle_client(peer, handler).await {
                        Ok(()) => eprintln!("disconnect {addr:?}"),
                        Err(e) => eprintln!("disconnect {addr:?} with error {e}"),
                    }
                }));
            }
        }
    }

    Ok(())
}