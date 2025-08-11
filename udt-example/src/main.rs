use std::{pin::pin, sync::Arc, time::{Duration, Instant}};

use tokio::{task::{self, JoinSet}, time::{interval, sleep}};
use tokio_util::sync::CancellationToken;
use transport::SecureTransport;

const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];
const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut js = JoinSet::<anyhow::Result<_>>::new();

    let ct = CancellationToken::new();
    let cancelled = ct.child_token().cancelled_owned();

    js.spawn(async move {
        let mut cancelled = pin!(cancelled);
        println!("B: my id is: {}", task::id());
        sleep(Duration::from_millis(10)).await;
        let l = Arc::new(udt::Endpoint::bind("[::]:25584".parse()?)?);
        println!("B: bound to {:?}", l.local_addr()?);
        let c = SecureTransport::connect(&l, "[::1]:25583".parse()?, transport::Side::Initiator { remote_public_key: &PUBLIC_KEY }).await?;
        // let c = l.connect_datagram("[::1]:25583".parse()?, false).await?;
        println!("B: connected to {:?}", c.peer_addr()?);

        // let mut rlimit = interval(Duration::from_millis(1));

        loop {
            // rlimit.tick().await;
            tokio::select! {
                _ = &mut cancelled => break,
                res = c.send_with(&[b'o'; 1412], true) => { res?; }
            }
        }

        let t1 = Instant::now();
        c.flush().await?;
        let t2 = Instant::now();
        println!("B: flush time: {} s", (t2-t1).as_secs_f32());

        let t1 = Instant::now();
        c.flush().await?;
        let t2 = Instant::now();
        println!("B: flush time: {} s", (t2-t1).as_secs_f32());

        Ok("sender")
    });

    js.spawn(async move {
        println!("A: my id is: {}", task::id());
        let l = Arc::new(udt::Endpoint::bind("[::]:25583".parse()?)?);//.listen_datagram(16)?;
        println!("A: bound to {:?}", l.local_addr()?);
        let r = SecureTransport::connect(&l, "[::1]:25584".parse()?, transport::Side::Responder { local_private_key: &PRIVATE_KEY }).await?;
        // let r = l.accept().await?;
        println!("A: connected to {:?}", r.peer_addr()?);
        let mut msg = [0; 30000];
        let mut int = interval(Duration::from_secs(5));
        let mut ctr = 0;

        let mut rem = 12;

        int.reset();
        loop {
            tokio::select! {
                biased;
                _ = int.tick() => {
                    let mbps = ctr as f64 * 8. / (1_000_000. * int.period().as_secs_f64());
                    ctr = 0;
                    println!("A: {mbps} mbps");
                    rem -= 1;
                    if rem == 0 {
                        ct.cancel();
                        // return Ok("receiver");
                    }
                }
                len = r.recv(&mut msg) => {
                    ctr += len?;
                }
            }
        }
    });
        
    while let Some(res) = js.join_next_with_id().await {
        println!("{res:?}");
    }

    Ok(())
}
