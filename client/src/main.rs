use std::{io, net::Ipv4Addr, time::Duration};

use bytestring::ByteString;
use fs::{Directory, FileReader, Filesystem};
use tokio::{io::AsyncReadExt as _, net::TcpStream, time};

pub mod fs;

const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

async fn tree(dir: &Directory) -> io::Result<()> {
    async fn tree_internal(dir: &Directory, indent: u32) -> io::Result<()> {
        let mut read_dir = dir.try_clone().await?.read_dir().await?;
        while let Some(x) = read_dir.next_entry().await? {
            for _ in 0..indent {
                print!("  ")
            }
            if x.mode & npwire::DMDIR == npwire::DMDIR {
                println!("{}/", x.name);
                Box::pin(tree_internal(&dir.open_dir_at(x.name).await?, indent+1)).await?;
            } else {
                println!("{}", x.name);
            }
        }
    
        Ok(())
    }

    let mut name = dir.stat().await?.name;
    if name == "/" {
        name = ByteString::new()
    }

    println!("{name}/");
    tree_internal(dir, 1).await
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    
    let sock = TcpStream::connect((Ipv4Addr::LOCALHOST, 64444)).await?;
    let noise = util::noise::NoiseStream::new(
        sock,
        util::noise::Side::Initiator { remote_public_key: &PUBLIC_KEY }
    ).await?;

    let fsys = Filesystem::new(noise);
    {
        let root = fsys.attach("anonymous", "").await?;
        tree(&root).await?;
        let f = root.open_at("ff2/dvd/video/ch1").await?;
        let mut file = FileReader::new(&f);

        let mut s = String::new();
        file.read_to_string(&mut s).await?;
        print!("{s}");
    }

    time::sleep(Duration::from_millis(100)).await;

    Ok(())
}