use std::{io, net::{Ipv4Addr, Ipv6Addr}, sync::Arc};

use tokio::net::{TcpListener, TcpStream, UdpSocket};

const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

async fn do_peer(endpoint: &utp::Endpoint, mut peer: TcpStream) -> io::Result<()> {
    let sock = endpoint.connect((Ipv6Addr::LOCALHOST, 64444)).await?;
    let mut noise = util::noise::NoiseStream::new(
        sock,
        util::noise::Side::Initiator { remote_public_key: &PUBLIC_KEY }
    ).await?;

    tokio::io::copy_bidirectional(&mut peer, &mut noise).await?;

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let socket = Arc::new(UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)).await?);
    let endpoint = Arc::new(utp::Endpoint::new(socket, 0));
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 64445)).await?;

    loop {
        let (peer, addr) = listener.accept().await?;

        let endpoint = endpoint.clone();
        tokio::spawn(async move {
            if let Err(e) = do_peer(&*endpoint, peer).await {
                eprintln!("client {addr} died with error {e}");
            }
        });
    }
}