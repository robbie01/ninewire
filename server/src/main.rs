#![forbid(unsafe_code)]

use std::{collections::BTreeMap, error::Error, future::{Future, ready}, net::{IpAddr, Ipv6Addr, SocketAddrV6}, path::PathBuf, pin::pin, sync::{Arc, atomic::{AtomicU64, Ordering}}};

use anyhow::{anyhow, bail};
use bytestring::ByteString;
use futures::{StreamExt, stream::abortable};
use mediator_proto::{mediator_client::MediatorClient, register_request, RegisterReply, RegisterRequest, Registration};
use np::traits;
use tokio::{net::TcpListener, sync::mpsc};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use transport::{PlainTcpTransport, SecureTransport, Side};
use util::is_unicast_global;

use crate::np::traits::{IsCancelSafe, cancel_safe};

mod np;
mod res;

type ShareTable = BTreeMap<Arc<str>, PathBuf>;

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

    fn auth(&self, _uname: &str, _aname: &str) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + IsCancelSafe + Send {
        cancel_safe(async move {
            bail!("Function not implemented");
        })
    }

    fn attach(&self, ares: Option<&Self::OpenResource>, uname: &str, aname: &str) -> impl Future<Output = Result<Self::PathResource, Self::Error>> + IsCancelSafe + Send {
        cancel_safe(async move {
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
        })
    }
}

const PRIVATE_KEY: [u8; 32] = [127, 93, 161, 223, 213, 211, 245, 80, 69, 165, 77, 133, 169, 40, 130, 112, 218, 255, 225, 74, 78, 69, 83, 20, 154, 244, 58, 224, 51, 34, 61, 102];
const PUBLIC_KEY: [u8; 32] = [241, 1, 228, 0, 247, 163, 248, 66, 94, 57, 122, 30, 59, 183, 146, 22, 39, 145, 26, 136, 130, 145, 111, 87, 19, 2, 218, 116, 17, 82, 71, 40];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    // console_subscriber::init();

    if false {
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

        let _listener = listener.filter(|e| ready(match e {
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

        // let _listener = select(
        //     listener.map_ok(|(s, a)| (Box::new(s) as Box<dyn NpTransport + Send>, SocketAddr::V6(a))),
        //     listener_plain.map(|r| match r {
        //         Ok(s) => {
        //             let a = s.peer_addr()?;
        //             Ok((Box::new(PlainTcpTransport::new(s)) as Box<dyn NpTransport + Send>, a))
        //         },
        //         Err(e) => Err(anyhow::Error::new(e))
        //     })
        // );
    }

    let listener_plain = TcpListenerStream::new(TcpListener::bind("127.0.0.1:8998").await?);

    let listener = listener_plain.map(|r| match r {
        Ok(s) => {
            let a = s.peer_addr()?;
            Ok((PlainTcpTransport::new(s), a))
        },
        Err(e) => Err(e)
    });

    let (listener, _handle) = abortable(listener);
    // ctrlc::set_handler(move || handle.abort())?;

    np::serve_mux(Arc::new(Handler::new([
        ("forfun".into(), "forfun".into()),
        ("ff2".into(), "forfun".into()),
        ("home".into(), "/Users/robbie".into())
    ].into_iter().collect())), pin!(listener)).await?;
    
    Ok(())
}
