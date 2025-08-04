use std::{io, net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6}, sync::Arc, time::Duration};

use anyhow::bail;
use bytestring::ByteString;
use fs::{Directory, FileReader, Filesystem};
use mediator_proto::{mediator_client::MediatorClient, RendezvousRequest};
use tokio::{io::AsyncReadExt as _, time};
use transport::SecureTransport;
use util::is_unicast_global;

pub mod fs;

const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

async fn tree(dir: &Directory) -> io::Result<()> {
    async fn tree_internal(dir: &Directory, indent: u32) -> io::Result<()> {
        let mut read_dir = dir.try_clone().await?.read_dir().await?;
        while let Some(x) = read_dir.next_entry().await? {
            for _ in 0..indent {
                print!("  ");
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
        name = ByteString::new();
    }

    println!("{name}/");
    tree_internal(dir, 1).await
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    // console_subscriber::init();

    let endpoint = Arc::new(udt::Endpoint::bind("[::]:0".parse()?)?);
    let port = endpoint.local_addr()?.port();

    let Some(addr) = local_ip_address::unix::list_afinet_netifas()?.into_iter()
        .find_map(|(_, addr)| match addr {
            IpAddr::V6(addr) if is_unicast_global(&addr) => Some(addr),
            _ => None
        }) else { bail!("no usable address :(") };

    let mut mediator = MediatorClient::connect("http://[::1]:64344").await?;

    let resp = mediator.rendezvous(RendezvousRequest {
        name: "bugerking".to_owned(),
        endpoint: Some(mediator_proto::Endpoint {
            addr: addr.octets().to_vec(),
            port: port.into(),
            pubkey: Vec::new()
        })
    }).await?.into_inner();
    drop(mediator);

    let Some(ep) = resp.endpoint else { bail!("no endpoint??") };

    let transport = SecureTransport::connect(
        &endpoint,
        SocketAddr::V6(SocketAddrV6::new(
            Ipv6Addr::from(<[u8; 16]>::try_from(&ep.addr[..])?),
            ep.port.try_into()?,
            0, 0
        )),
        transport::Side::Initiator { remote_public_key: &PUBLIC_KEY }
    ).await?;

    let fsys = Filesystem::new(transport).await?;
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