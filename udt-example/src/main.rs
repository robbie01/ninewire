use std::{sync::Arc, time::Duration};

use tokio::{task::JoinSet, time::sleep};
use transport::SecureTransport;

const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];
const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut js = JoinSet::new();

    js.spawn(async {
        let l = Arc::new(udt::Endpoint::bind("[::]:25583".parse()?)?);//.listen_datagram(16)?;
        println!("A: bound to {:?}", l.local_addr()?);
        let r = SecureTransport::connect(&l, "[::1]:25584".parse()?, transport::Side::Responder { local_private_key: &PRIVATE_KEY }).await?;
        println!("A: connected to {:?}", r.remote_addr()?);
        let mut msg = [0; 1024];
        let len = r.recv(&mut msg).await?;
        let msg = String::from_utf8_lossy(&msg[..len]);
        println!("A: received message {:?}", msg);
        Ok::<(), anyhow::Error>(())
    });

    js.spawn(async {
        sleep(Duration::from_millis(10)).await;
        let l = Arc::new(udt::Endpoint::bind("[::]:25584".parse()?)?);
        println!("B: bound to {:?}", l.local_addr()?);
        let c = SecureTransport::connect(&l, "[::1]:25583".parse()?, transport::Side::Initiator { remote_public_key: &PUBLIC_KEY }).await?;
        println!("B: connected to {:?}", c.remote_addr()?);
        c.send_with(b"Hello, world!", true).await?;
        c.flush().await?;
        // println!("B: sent {len} bytes");
        Ok::<(), anyhow::Error>(())
    });
        
    js.join_all().await;
    sleep(Duration::from_millis(10)).await;
    Ok(())
}
