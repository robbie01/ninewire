use std::{net::Ipv4Addr, time::Duration};

use fs::{Filesystem, ReadableFile};
use rand::random;
use tokio::{io::AsyncReadExt as _, net::TcpStream, time};

pub mod fs;

const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let privkey = random::<[u8; 32]>();
    let sock = TcpStream::connect((Ipv4Addr::LOCALHOST, 64444)).await?;
    let noise = util::noise::NoiseStream::new_init(
        sock,
        &privkey,
        util::noise::Side::Initiator { remote_public_key: &PUBLIC_KEY }
    ).await?;

    let fsys = Filesystem::new(noise);
    {
        let root = fsys.mount().await?;
        let mut readdir = root.open_dir_at("ff2/").await?.read_dir().await?;
        while let Some(x) = readdir.next_entry().await? {
            if x.mode & npwire::DMDIR == npwire::DMDIR {
                println!("{}/", x.name);
            } else {
                println!("{}", x.name);
            }
        }
        let f = root.open_at("ff2/dvd/video/ch1").await?;
        let mut file = ReadableFile::new(&f);

        let mut s = String::new();
        file.read_to_string(&mut s).await?;
        print!("{s}");
    }

    time::sleep(Duration::from_millis(100)).await;

    Ok(())
}