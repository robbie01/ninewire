use std::{io::{Read as _, Write}, thread};

fn main() -> anyhow::Result<()> {
    thread::scope(|scope| {
        let a = scope.spawn(|| {
            let l = udt::Endpoint::bind("[::]:25583".parse()?)?;//.listen_datagram(16)?;
            println!("A: listening on {:?}", l.local_addr()?);
            let mut r = l.connect_stream("[::1]:25584".parse()?, true)?;
            println!("A: connection from {:?}", r.remote_addr()?);
            let mut msg = String::new();
            r.read_to_string(&mut msg)?;
            // let mut msg = [0; 1024];
            // let len = r.recv(&mut msg)?;
            // let msg = String::from_utf8_lossy(&msg[..len]);
            println!("A: received message {:?}", msg);
            Ok::<(), anyhow::Error>(())
        });

        let b = scope.spawn(|| {
            let l = udt::Endpoint::bind("[::]:25584".parse()?)?;
            println!("B: bound to {:?}", l.local_addr()?);
            let mut c = l.connect_stream("[::1]:25583".parse()?, true)?;
            println!("B: connected to {:?}", c.remote_addr()?);
            c.write_all(b"Hello, world!")?;
            // println!("B: sent {len} bytes");
            Ok::<(), anyhow::Error>(())
        });
        
        if let Err(e) = a.join().unwrap() {
            println!("A: {e:?}");
        }
        if let Err(e) = b.join().unwrap() {
            println!("B: {e:?}");
        }
    });
    Ok(())
}
