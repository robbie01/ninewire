use std::{io::{Read, Write}, net::{TcpListener, TcpStream}, time::{Duration, Instant}};

use tokio::{task::JoinSet, time::sleep};
// use transport::SecureTransport;

// const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];
// const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

/* 
 * OBSERVATIONS:
 * UDT datagram has like 16 mbps of average throughput despite being reorderable.
 * UDT stream has about 1 gbps.
 * TCP has about 10 gbps.
 * 
 * Something needs to be reworked. Maybe congestion control?
 */

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut js = JoinSet::new();

    let use_udt = true;

    js.spawn_blocking(move || {
        let mut cu;
        let mut ct;
        let r: &mut dyn Read;
        if use_udt {
            let l = udt::Endpoint::bind("[::]:25583".parse()?)?.listen_stream(16)?;
            println!("A: bound to {:?}", l.local_addr()?);
            cu = l.accept()?;
            println!("A: connected to {:?}", cu.peer_addr()?);
            r = &mut cu;
        } else {
            let l = TcpListener::bind("[::]:25583")?;
            println!("A: bound to {:?}", l.local_addr()?);
            (ct, _) = l.accept()?;
            println!("A: connected to {:?}", ct.peer_addr()?);
            r = &mut ct;
        }

        let mut msg = [0; 30000];
        let mut t = Instant::now();
        let mut timeout = t + Duration::from_secs(5);
        let mut ctr = 0;
        loop {
            let len = r.read(&mut msg)?;
            if len == 0 { break }

            ctr += len;
            let now = Instant::now();
            if now > timeout {
                let mbps = ctr as f64 * 8. / (1_000_000. * (now - t).as_secs_f64());
                ctr = 0;
                println!("A: {mbps} mbps");
                t = now;
                timeout = now + Duration::from_secs(5);
            }

            // let msg = String::from_utf8_lossy(&msg[..len]);
            // println!("A: received message {:?}", msg.len());
        }
        Ok::<(), anyhow::Error>(())
    });

    js.spawn_blocking(move || {
        // sleep(Duration::from_millis(10)).await;
        // let l = udt::Endpoint::bind("[::]:0".parse()?)?;
        // println!("B: bound to {:?}", l.local_addr()?);
        // let mut c = l.connect_stream("[::1]:25583".parse()?, false)?;
        let mut cu;
        let mut ct;
        let c: &mut dyn Write;
        if use_udt {
            let l = udt::Endpoint::bind("[::]:0".parse()?)?;
            println!("B: bound to {:?}", l.local_addr()?);
            cu = l.connect_stream("[::1]:25583".parse()?, false)?;
            println!("B: connected to {:?}", cu.peer_addr()?);
            c = &mut cu;
        } else {
            ct = TcpStream::connect("[::1]:25583")?;
            println!("B: connected to {:?}", ct.peer_addr()?);
            c = &mut ct;
        }

        loop {
            c.write(&[b'o'; 1192])?;
        }
        // println!("B: sent {len} bytes");
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });
        
    println!("{:?}", js.join_all().await);
    sleep(Duration::from_millis(10)).await;
    Ok(())
}
