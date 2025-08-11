use std::{collections::HashMap, future::{ready, Future}, mem, pin::{pin, Pin}, sync::Arc, task::{Context, Poll, Waker}};

use bytes::BytesMut;
use bytestring::ByteString;
use futures::{io, stream::FuturesUnordered, FutureExt as _, Stream, StreamExt as _};
use pin_project::pin_project;
use tokio::sync::RwLock;
use npwire::*;
use transport::SecureTransport;
use util::polymur;

use super::{traits::{OpenResource as _, PathResource as _, Resource as _}, Serve};

const MAX_IN_FLIGHT: usize = 16;

// 1280: IPv6 MTU
// 64: UDT combined overhead (IP+UDP+UDT)
// 8/16: nonce/tag
const MAX_MESSAGE_SIZE: u32 = 1280 - 64 - 8 - 16;

#[derive(Debug)]
enum Resource<S: Serve> {
    Path(S::PathResource),
    Open(S::OpenResource)
}

struct ResourceManager<S: Serve> {
    resources: RwLock<HashMap<u32, Resource<S>, polymur::RandomState>>,
    handler: Arc<S>,
}

#[pin_project]
struct TaggedFuture<T> {
    tag: u16,
    flushes: Option<u16>,
    #[pin]
    hdl: T
}

impl<T: Future> Future for TaggedFuture<T> {
    type Output = (u16, Option<u16>, T::Output);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        me.hdl.poll(cx).map(|v| (
            *me.tag,
            *me.flushes,
            v
        ))
    }
}

const fn rerror(ename: &'static str) -> Rerror {
    Rerror { ename: ByteString::from_static(ename) }
}

async fn dispatch<S: Serve>(
    resource_mgr: &ResourceManager<S>,
    request: TMessage,
    maxlen: usize
) -> Result<RMessage, Rerror> {
    match request {
        TMessage::Tversion(..) | TMessage::Tflush(..) => {
            unimplemented!()
        },
        TMessage::Tauth(Tauth { afid, uname, aname }) => {
            if afid == !0 {
                return Err(rerror("fid invalid"));
            }

            let mut resources = resource_mgr.resources.write().await;

            if resources.contains_key(&afid) {
                return Err(rerror("fid in use"));
            }

            let res = resource_mgr.handler.auth(&uname, &aname).await?;
            let aqid = res.qid();
            
            resources.insert(afid, Resource::Open(res));
            
            Ok(Rauth { aqid }.into())
        },
        TMessage::Tattach(Tattach { fid, afid, uname, aname }) => {
            if fid == !0 {
                return Err(rerror("fid invalid"));
            }

            let mut resources = resource_mgr.resources.write().await;

            if resources.contains_key(&fid) {
                return Err(rerror("fid in use"));
            }

            let ares = if afid == !0 {
                None
            } else if let Some(Resource::Open(res)) = resources.get(&afid) {
                Some(res)
            } else {
                return Err(rerror("fid invalid"));
            };
            
            let res = resource_mgr.handler.attach(ares, &uname, &aname).await?;
            let qid = res.qid();
            
            resources.insert(fid, Resource::Path(res));
            
            Ok(Rattach { qid }.into())
        },
        TMessage::Twalk(Twalk { fid, newfid, wname }) => {
            if newfid == !0 {
                return Err(rerror("Invalid argument"));
            }

            let mut resources = resource_mgr.resources.write().await;

            if resources.contains_key(&newfid) {
                return Err(rerror("fid in use"));
            }
            let resource = resources.get(&fid).ok_or_else(|| rerror("Fid not"))?;
            
            if let Resource::Path(resource) = resource {
                let wname = wname.iter().map(|s| &s[..]).collect::<Vec<_>>();
                let (mut wqid, new_resource) = resource.walk(&wname).await?;
                
                if let Some(new_resource) = new_resource {
                    if !wname.is_empty() {
                        wqid.push(new_resource.qid());
                    }
                    resources.insert(newfid, Resource::Path(new_resource));
                }
                
                Ok(Rwalk { wqid }.into())
            } else {
                Err(rerror("fid open for I/O"))
            }
        },
        TMessage::Topen(Topen { fid, mode }) => {
            let mut resources = resource_mgr.resources.write().await;
            let resource = resources.get_mut(&fid).ok_or_else(|| rerror("fid invalid"))?;
            
            if let Resource::Path(path_resource) = resource {
                let open_resource = path_resource.open(mode).await?;
                let qid = open_resource.qid();
                
                *resource = Resource::Open(open_resource);
                
                Ok(Ropen { qid, iounit: 0 }.into())
            } else {
                Err(rerror("fid open for I/O"))
            }
        },
        TMessage::Tcreate(Tcreate { fid, name, perm, mode }) => {
            let mut resources = resource_mgr.resources.write().await;
            let resource = resources.get_mut(&fid).ok_or_else(|| rerror("fid invalid"))?;
            
            if let Resource::Path(resource) = resource {
                let open_resource = resource.create(&name, perm, mode).await?;
                let qid = open_resource.qid();
                
                resources.insert(fid, Resource::Open(open_resource));
                
                Ok(Rcreate { qid, iounit: 0 }.into())
            } else {
                Err(rerror("fid open for I/O"))
            }
        },
        TMessage::Tread(Tread { fid, offset, count }) => {
            let resources = resource_mgr.resources.read().await;
            let resource = resources.get(&fid).ok_or_else(|| rerror("fid invalid"))?;
            
            if let Resource::Open(resource) = resource {
                let mut data = resource.read(offset, count).await?;
                data.truncate(maxlen - RREAD_OVERHEAD);
                Ok(Rread { data }.into())
            } else {
                Err(rerror("fid not open for read"))
            }
        },
        TMessage::Treads(_) => {
            Err(rerror("not implemented"))
        }
        TMessage::Twrite(Twrite { fid, offset, data }) => {
            let resources = resource_mgr.resources.read().await;
            let resource = resources.get(&fid).ok_or_else(|| rerror("fid invalid"))?;
            
            if let Resource::Open(resource) = resource {
                let count = resource.write(offset, &data).await?;
                Ok(Rwrite { count }.into())
            } else {
                Err(rerror("fid not open for write"))
            }
        },
        TMessage::Tclunk(Tclunk { fid }) => {
            let mut resources = resource_mgr.resources.write().await;
            if resources.remove(&fid).is_some() {
                Ok(Rclunk.into())
            } else {
                Err(rerror("fid invalid"))
            }
        },
        TMessage::Tremove(Tremove { fid }) => {
            let mut resources = resource_mgr.resources.write().await;
            let resource = resources.remove(&fid).ok_or_else(|| rerror("fid invalid"))?;
            
            match resource {
                Resource::Path(res) => res.remove().await?,
                Resource::Open(res) => res.remove().await?
            };
            
            Ok(Rremove.into())
        },
        TMessage::Tstat(Tstat { fid }) => {
            let resources = resource_mgr.resources.read().await;
            let resource = resources.get(&fid).ok_or_else(|| rerror("fid invalid"))?;
            
            let stat = match resource {
                Resource::Path(res) => res.stat().await?,
                Resource::Open(res) => res.stat().await?
            };
            
            Ok(Rstat { stat }.into())
        },
        TMessage::Twstat(Twstat { fid, stat }) => {
            let resources = resource_mgr.resources.read().await;
            let resource = resources.get(&fid).ok_or_else(|| rerror("fid invalid"))?;
            
            match resource {
                Resource::Path(res) => res.wstat(stat).await?,
                Resource::Open(res) => res.wstat(stat).await?,
            };
            
            Ok(Rwstat.into())
        }
    }
}

fn poll_no_context<S: Stream + Unpin>(stream: &mut S) -> Poll<Option<S::Item>> {
    stream.poll_next_unpin(&mut Context::from_waker(Waker::noop()))
}

pub async fn handle_client<S: Serve>(
    peer: SecureTransport,
    handler: Arc<S>
) -> io::Result<()> {
    let resource_mgr = ResourceManager {
        resources: RwLock::default(),
        handler: handler.clone(),
    };

    let mut inflight = pin!(FuturesUnordered::new());

    let mut initialized = false;
    let mut next_session = None;

    loop {
        if inflight.is_empty() {
            if let Some(Tversion { msize, version }) = next_session.take() {
                // in-flight requests have been completely flushed out
                resource_mgr.resources.write().await.clear();

                if msize < 256 {
                    peer.send(rerror(
                        "Tversion: message size too small"
                    ).serialize(!0).unwrap()).await?;
                } else {
                    let msize = msize.min(MAX_MESSAGE_SIZE);
                    let version: &'static str = if version == "9P2000" { "9P2000" } else { "unknown" };
                    peer.send(Rversion { msize, version: ByteString::from_static(version) }.serialize(!0).unwrap()).await?;
    
                    initialized = true;
                }
            }
        }

        // 2025-03-31: I have realized that I reinvented StreamExt::buffer_unordered
        // from first principles. Luckily, that method doesn't actually work directly
        // with what I need to do because of the flush stuff.
        let mut buffer = BytesMut::zeroed(MAX_MESSAGE_SIZE as usize);
        tokio::select! {
            biased;
            incoming_n = peer.recv(&mut buffer), if inflight.len() < MAX_IN_FLIGHT && next_session.is_none() => {
                let incoming_n = incoming_n?;
                let mut incoming = mem::replace(&mut buffer, BytesMut::zeroed(MAX_MESSAGE_SIZE as usize));
                incoming.truncate(incoming_n);

                let des = deserialize_t(incoming.freeze());

                if !initialized && !matches!(des, Ok((_, TMessage::Tversion(_)))) {
                    // just throw out any messages before the first Tversion
                    continue;
                }

                match des {
                    Ok((tag, req)) => {
                        match req {
                            TMessage::Tversion(tversion) => {
                                if tag == !0 {
                                    next_session = Some(tversion);
                                } else {
                                    inflight.push(TaggedFuture {
                                        tag,
                                        flushes: None,
                                        hdl: ready(rerror(
                                            "Tversion: invalid tag"
                                        ).into()).right_future()
                                    });
                                }
                            },
                            TMessage::Tflush(Tflush { oldtag }) => {
                                if let Some(fut) = inflight.as_mut().iter_pin_mut().find(|h| h.tag == oldtag) {
                                    *fut.project().flushes = Some(tag);
                                } else {
                                    inflight.push(TaggedFuture {
                                        tag,
                                        flushes: None,
                                        hdl: ready(Rflush.into()).right_future()
                                    });
                                }
                            },
                            req => {
                                inflight.push(TaggedFuture {
                                    tag,
                                    flushes: None,
                                    hdl: dispatch(
                                        &resource_mgr,
                                        req,
                                        MAX_MESSAGE_SIZE as usize
                                    ).map(|resp| resp.unwrap_or_else(RMessage::from)).left_future()
                                });
                            }
                        }
                    },
                    Err(e) => {
                        if let Some(tag) = e.tag() {
                            inflight.push(TaggedFuture {
                                tag,
                                flushes: None,
                                hdl: ready(Rerror {
                                    ename: e.to_string().into()
                                }.into()).right_future()
                            });
                        }
                    }
                }
            },
            Some((mut tag, mut flushes, mut resp)) = inflight.next() => {
                // Desperate attempt to replicate the behavior of StreamExt::forward
                // (Maybe I should just implement my own buffered stream at this point?)
                loop {
                    let serialized = resp
                        .serialize(tag)
                        .unwrap_or_else(|e| Rerror::from(e).serialize(tag).unwrap());

                    peer.send(serialized).await?;

                    if let Some(flush) = flushes {
                        peer.send(Rflush.serialize(flush).unwrap()).await?;
                    }

                    if let Poll::Ready(Some(tfr)) = poll_no_context(&mut inflight) {
                        tag = tfr.0;
                        flushes = tfr.1;
                        resp = tfr.2;
                    } else {
                        break;
                    }
                }
            },
            else => break
        }
    }

    peer.flush().await?;

    Ok(())
}