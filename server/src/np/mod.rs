use std::{convert::Infallible, fmt::Debug, io, net::SocketAddr, ops::RangeInclusive, sync::Arc, time::Duration};

use bytes::BytesMut;
use rand::{rng, Rng as _};
use tokio::{io::{AsyncRead, AsyncWrite}, net::UdpSocket, task::{id, JoinSet}, time::sleep};
use tokio_util::net::Listener;
use traits::Serve;

pub mod traits;
mod client;

const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];

async fn _hole_punch(sock: &UdpSocket, dest: SocketAddr) -> io::Result<Infallible> {
    const MTU: usize = 1232;
    const INTERVAL_RANGE: RangeInclusive<Duration> = Duration::from_millis(10)..=Duration::from_millis(200);

    let mut rng = rng();
    let mut buffer = BytesMut::zeroed(MTU);

    loop {
        rng.fill(&mut buffer[..]);
        buffer[0] &= 0xF0; // mask out version
        sock.send_to(&buffer, dest).await?;

        sleep(rng.random_range(INTERVAL_RANGE)).await;
    }
}

pub async fn serve_mux<S: Serve, L: Listener>(
    handler: Arc<S>,
    mut listener: L,
) -> io::Result<Infallible> where
    L::Io: AsyncRead + AsyncWrite + Send + 'static,
    L::Addr: Debug + Send + 'static
{
    let mut conns = JoinSet::new();

    loop {
        tokio::select! {
            biased;
            Some(res) = conns.join_next_with_id() => {
                if let Err(e) = res {
                    eprintln!("{e}");
                }
            },
            res = listener.accept() => {
                let (peer, addr) = res?;
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
}