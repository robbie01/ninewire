use std::sync::Arc;

use futures::FutureExt;
use tokio::{io::AsyncWriteExt, net::UdpSocket};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    console_subscriber::init();

    println!("{:?}", UdpSocket::bind("127.0.0.1:0").await?.local_addr()?);

    let socket = Arc::new(UdpSocket::bind("127.0.0.1:9000").await?);
    let endpoint = utp::Endpoint::new(socket, 16);
    loop {
        let (peer, addr) = endpoint.accept().await?;
        println!("con from {addr}");
        tokio::spawn(async move {
            let (mut rd, mut wr) = tokio::io::split(peer);
            wr.write_all(b"helo\n").await?;
            tokio::io::copy(&mut rd, &mut wr).await?;
            let mut peer = rd.unsplit(wr);
            peer.shutdown().await?;
            println!("disconnect {addr}");
            Ok::<_, anyhow::Error>(())
        }.map(Result::unwrap));
    }
}
