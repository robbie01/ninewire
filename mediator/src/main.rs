use std::{collections::HashMap, pin::Pin, sync::{Arc, LazyLock}};

use async_stream::try_stream;
use mediator_proto::{mediator_server, register_request, Endpoint, RegisterReply, RegisterRequest, RendezvousReply, RendezvousRequest};
use scc::hash_map::Entry;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::Stream;
use tokio_util::sync::CancellationToken;
use tonic::Status;
use util::polymur;

#[derive(Debug)]
struct PlsRendezvous {
    ep: Endpoint,
    reply: oneshot::Sender<Option<Endpoint>>
}

#[derive(Debug, Default)]
struct Handler {
    mappings: Arc<scc::HashMap<String, mpsc::Sender<PlsRendezvous>>>
}

fn validate_endpoint(ep: &Endpoint, server: bool) -> tonic::Result<()> {
    if ep.addr.len() != 16 {
        return Err(Status::invalid_argument("bad address"));
    }
    if !(1..65536).contains(&ep.port) {
        return Err(Status::invalid_argument("bad port"));
    }
    if !server && !ep.pubkey.is_empty() {
        return Err(Status::invalid_argument("you don't get a public key"));
    }
    if server && ep.pubkey.len() != 32 {
        return Err(Status::invalid_argument("bad public key"))
    }
    Ok(())
}

#[tonic::async_trait]
impl mediator_server::Mediator for Handler {
    type RegisterStream = Pin<Box<dyn Stream<Item = tonic::Result<RegisterReply>> + Send>>;//Either<ReceiverStream<tonic::Result<RegisterReply>>, Empty<tonic::Result<RegisterReply>>>;

    async fn rendezvous(&self, req: tonic::Request<RendezvousRequest>) -> tonic::Result<tonic::Response<RendezvousReply>> {
        let req = req.into_inner();
        let Some(ep) = req.endpoint else { return Err(Status::invalid_argument("unspecified endpoint")) };
        validate_endpoint(&ep, false)?;
        let Some(pls_sender) = self.mappings.get_async(&req.name).await else { return Err(Status::not_found("unknown name")) };
        let (pls_req, pls_rep) = oneshot::channel();
        pls_sender.send(PlsRendezvous {
            ep,
            reply: pls_req
        }).await.map_err(|_| Status::internal("mapping is broken"))?;

        let rep = pls_rep.await.map_err(|_| Status::unavailable("no reply from peer"))?;

        let remote_ep = rep.ok_or_else(|| Status::permission_denied("rendezvous denied by peer"))?;
        Ok(RendezvousReply {
            endpoint: Some(remote_ep)
        }.into())
    }

    async fn register(&self, req: tonic::Request<tonic::Streaming<RegisterRequest>>) -> tonic::Result<tonic::Response<Self::RegisterStream>> {
        let mappings = self.mappings.clone();
        Ok(tonic::Response::new(Box::pin(try_stream! {
            let mut req = req.into_inner();

            let Some(first) = req.message().await? else { return };
            let Some(register_request::Req::Registration(reg)) = first.req else { Err(Status::invalid_argument("first request must be registration"))? };

            let name = reg.name;
            let Some(endpoint) = reg.endpoint else { Err(Status::invalid_argument("unspecified endpoint"))? };
            validate_endpoint(&endpoint, true)?;

            let ent = mappings.entry_async(name.clone()).await;
            let mut rdv_requests = match ent {
                Entry::Occupied(mut ent) => {
                    if !ent.is_closed() {
                        Err(Status::already_exists("requested name is in use"))?
                    }
                    let (inc, rdv_request) = mpsc::channel(1);
                    ent.insert(inc);
                    rdv_request
                },
                Entry::Vacant(ent) => {
                    let (inc, rdv_request) = mpsc::channel(1);
                    ent.insert_entry(inc);
                    rdv_request
                }
            };

            println!("new binding for {name}");

            let mut ctr = 1u64;
            let mut inflight = HashMap::<u64, oneshot::Sender<Option<Endpoint>>, polymur::RandomState>::default();

            loop {
                tokio::select! {
                    rdv_req = rdv_requests.recv() => {
                        match rdv_req {
                            None => break,
                            Some(rdv_req) => {
                                let id = ctr;
                                ctr += 1;
                                inflight.insert(id, rdv_req.reply);
                                yield RegisterReply {
                                    request_id: id,
                                    endpoint: Some(rdv_req.ep)
                                };
                            }
                        }
                    }
                    rdv_reply = req.message() => {
                        let rdv_reply = rdv_reply.unwrap(); // todo
                        match rdv_reply {
                            None => break,
                            Some(rdv_reply) => {
                                match rdv_reply.req {
                                    Some(register_request::Req::ApproveId(id)) => {
                                        if let Some(reply) = inflight.remove(&id) {
                                            let _ = reply.send(Some(endpoint.clone()));
                                        }
                                    },
                                    Some(register_request::Req::DenyId(id)) => {
                                        if let Some(reply) = inflight.remove(&id) {
                                            let _ = reply.send(None);
                                        }
                                    }
                                    _ => ()
                                }
                            }
                        }
                    }
                }
                inflight.retain(|_, v| !v.is_closed());
            }
        })))
    }
}

static SHUTDOWN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ctrlc::set_handler(|| SHUTDOWN.cancel())?;

    let addr = "[::]:64344".parse()?;
    println!("listening on {addr}");
    tonic::transport::Server::builder()
        .add_service(tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(mediator_proto::FILE_DESCRIPTOR_SET)
            .build_v1()?)
        .add_service(mediator_server::MediatorServer::new(Handler::default()))
        .serve_with_shutdown(addr, SHUTDOWN.cancelled()).await?;
    println!("shutting down");
    Ok(())
}
