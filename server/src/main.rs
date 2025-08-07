use std::{collections::HashMap, error::Error, future::ready, net::{IpAddr, Ipv6Addr, SocketAddrV6}, path::PathBuf, pin::pin, sync::{atomic::{AtomicU64, Ordering}, Arc}};

use anyhow::{anyhow, bail};
use bytestring::ByteString;
use futures::{stream::abortable, StreamExt};
use mediator_proto::{mediator_client::MediatorClient, register_request, RegisterReply, RegisterRequest, Registration};
use np::traits;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream};
use transport::{SecureTransport, Side};
use util::is_unicast_global;

mod np;
mod res;

type ShareTable = HashMap<Arc<str>, PathBuf>;

#[derive(Debug)]
struct Config {
    shares: ShareTable
}

#[derive(Debug)]
struct Handler {
    session_ctr: AtomicU64,
    inner: Arc<Config>
}

#[derive(Debug)]
struct Session {
    #[allow(unused)]
    id: u64,
    uname: ByteString
}

impl Handler {
    fn new(shares: ShareTable) -> Self {
        Self { 
            session_ctr: AtomicU64::new(1),
            inner: Arc::new(Config { shares })
        }
    }
}

impl traits::Serve for Handler {
    type Error = anyhow::Error;
    type PathResource = res::path::PathResource;
    type OpenResource = res::open::OpenResource;

    async fn auth(&self, _uname: &str, _aname: &str) -> Result<Self::OpenResource, Self::Error> {
        bail!("Function not implemented");
    }

    async fn attach(&self, ares: Option<&Self::OpenResource>, uname: &str, aname: &str) -> Result<Self::PathResource, Self::Error> {
        if ares.is_some() {
            bail!("permission denied");
        }

        if !aname.is_empty() {
            bail!("No such file or directory");
        }

        let session = Arc::new(Session {
            id: self.session_ctr.fetch_add(1, Ordering::Relaxed),
            uname: uname.into()
        });

        Ok(res::path::PathResource::root(self, session))
    }
}

const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];
const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    // console_subscriber::init();

    let Some(addr) = local_ip_address::unix::list_afinet_netifas()?.into_iter()
        .find_map(|(_, addr)| match addr {
            IpAddr::V6(addr) if is_unicast_global(&addr) => Some(addr),
            _ => None
        }) else { bail!("no usable address :(") };
    
    let endpoint = Arc::new(udt::Endpoint::bind("[::]:0".parse()?)?);

    println!("bound to [{addr:?}]:{}", endpoint.local_addr()?.port());

    let mut mediator = MediatorClient::connect("http://[::1]:64344").await?;

    let (registration, r2) = mpsc::channel(1);
    let incoming = mediator.register(ReceiverStream::new(r2)).await?.into_inner();
    println!("brug");

    registration.send(RegisterRequest {
        req: Some(register_request::Req::Registration(Registration {
            name: "bugerking".to_owned(),
            endpoint: Some(mediator_proto::Endpoint {
                addr: addr.octets().to_vec(),
                port: endpoint.local_addr()?.port().into(),
                pubkey: PUBLIC_KEY.to_vec()
            })
        }))
    }).await.map_err(|_| anyhow!("bruh moment"))?;

    let listener = incoming
        .filter_map(|req| ready({
            match req {
                Ok(RegisterReply { request_id: 0, .. }) => None,
                _ => Some(async {
                    let req = req?;
                    registration.send(RegisterRequest {
                        req: Some(register_request::Req::ApproveId(req.request_id))
                    }).await.map_err(|_| anyhow!("bruh moment2"))?;
                    let ep = req.endpoint.unwrap();
                    let addr = Ipv6Addr::from(<[u8; 16]>::try_from(&ep.addr[..])?);
                    let port = ep.port.try_into()?;
                    let ep = SocketAddrV6::new(addr, port, 0, 0);
                    Ok::<_, anyhow::Error>((
                        SecureTransport::connect(&endpoint, ep.into(), Side::Responder { local_private_key: &PRIVATE_KEY }).await?,
                        ep
                    ))
                })
            }
        }))
        .buffer_unordered(16);

    let listener = listener.filter(|e| ready(match e {
        Ok(_) => true,
        Err(e) => {
            let is_tonic_error = e.chain()
                .find_map(<dyn Error>::downcast_ref::<tonic::Status>)
                .is_some();
            if !is_tonic_error {
                println!("connection failed: {e}");
            }
            is_tonic_error
        }
    }));

    let (listener, _handle) = abortable(listener);
    // ctrlc::set_handler(move || handle.abort())?;

    np::serve_mux(Arc::new(Handler::new([
        ("forfun".into(), "forfun".into()),
        ("ff2".into(), "forfun".into())
    ].into_iter().collect())), pin!(listener)).await?;
    
    Ok(())
}
