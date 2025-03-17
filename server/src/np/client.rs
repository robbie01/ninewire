use std::{collections::HashMap, future::Future, pin::{pin, Pin}, sync::Arc, task::{Context, Poll}};

use futures::{io, stream::FuturesUnordered, SinkExt, StreamExt};
use pin_project::pin_project;
use tokio::{io::{AsyncRead, AsyncWrite}, sync::RwLock};
use tokio_util::codec::LengthDelimitedCodec;
use npwire::*;
use util::noise::{NoiseStream, Side};

use super::{traits::{OpenResource as _, PathResource as _, Resource as _}, Serve2};

const MAX_IN_FLIGHT: usize = 16;
const MAX_MESSAGE_SIZE: u32 = 65535 - 16;

#[derive(Debug)]
enum Resource<S: Serve2> {
    Path(S::PathResource),
    Open(S::OpenResource)
}

struct ResourceManager<S: Serve2> {
    resources: RwLock<HashMap<u32, Resource<S>>>,
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

async fn dispatch<S: Serve2>(
    resource_mgr: Arc<ResourceManager<S>>,
    request: TMessage,
    _maxlen: usize
) -> Result<RMessage, Rerror> {
    match request {
        TMessage::Tversion(..) | TMessage::Tflush(..) => {
            unimplemented!()
        },
        TMessage::Tauth(Tauth { afid, uname, aname }) => {
            if afid == !0 {
                return Err(Rerror { ename: "Invalid argument".into() });
            }

            let mut resources = resource_mgr.resources.write().await;

            if resources.contains_key(&afid) {
                return Err(Rerror { ename: "Fid in use".into() });
            }

            let res = resource_mgr.handler.auth(&uname, &aname).await?;
            let aqid = res.qid();
            
            resources.insert(afid, Resource::Open(res));
            
            Ok(Rauth { aqid }.into())
        },
        TMessage::Tattach(Tattach { fid, afid, uname, aname }) => {
            if fid == !0 {
                return Err(Rerror { ename: "Invalid argument".into() });
            }

            let mut resources = resource_mgr.resources.write().await;

            if resources.contains_key(&fid) {
                return Err(Rerror { ename: "Fid in use".into() });
            }

            let ares = if afid != !0 {
                if let Some(res) = resources.get(&afid) {
                    if let Resource::Open(res) = res {
                        Some(res)
                    } else {
                        return Err(Rerror { ename: "Invalid argument".into() });
                    }
                } else {
                    return Err(Rerror { ename: "Fid not found".into() });
                }
            } else {
                None
            };
            
            let res = resource_mgr.handler.attach(ares, &uname, &aname).await?;
            let qid = res.qid();
            
            resources.insert(fid, Resource::Path(res));
            
            Ok(Rattach { qid }.into())
        },
        TMessage::Twalk(Twalk { fid, newfid, wname }) => {
            if newfid == !0 {
                return Err(Rerror { ename: "Invalid argument".into() });
            }

            let mut resources = resource_mgr.resources.write().await;

            if resources.contains_key(&newfid) {
                return Err(Rerror { ename: "Fid in use".into() });
            }
            let resource = resources.get(&fid).ok_or_else(|| Rerror { ename: "Fid not".into() })?;
            
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
                Err(Rerror { ename: "Invalid argument".into() })
            }
        },
        TMessage::Topen(Topen { fid, mode }) => {
            let mut resources = resource_mgr.resources.write().await;
            let resource = resources.get_mut(&fid).ok_or_else(|| Rerror { ename: "Fid not found".into() })?;
            
            if let Resource::Path(path_resource) = resource {
                let open_resource = path_resource.open(mode).await?;
                let qid = open_resource.qid();
                
                *resource = Resource::Open(open_resource);
                
                Ok(Ropen { qid, iounit: 0 }.into())
            } else {
                Err(Rerror { ename: "Invalid argument".into() })
            }
        },
        TMessage::Tcreate(Tcreate { fid, name, perm, mode }) => {
            let mut resources = resource_mgr.resources.write().await;
            let resource = resources.get_mut(&fid).ok_or_else(|| Rerror { ename: "Fid not found".into() })?;
            
            if let Resource::Path(resource) = resource {
                let open_resource = resource.create(&name, perm, mode).await?;
                let qid = open_resource.qid();
                
                resources.insert(fid, Resource::Open(open_resource));
                
                Ok(Rcreate { qid, iounit: 0 }.into())
            } else {
            Err(Rerror { ename: "Invalid argument".into() })
            }
        },
        TMessage::Tread(Tread { fid, offset, count }) => {
            let resources = resource_mgr.resources.read().await;
            let resource = resources.get(&fid).ok_or_else(|| Rerror { ename: "unknown fid".into() })?;
            
            if let Resource::Open(resource) = resource {
                let data = resource.read(offset, count).await?;
                Ok(Rread { data }.into())
            } else {
                Err(Rerror { ename: "Invalid argument".into() })
            }
        },
        TMessage::Twrite(Twrite { fid, offset, data }) => {
            let resources = resource_mgr.resources.read().await;
            let resource = resources.get(&fid).ok_or_else(|| Rerror { ename: "unknown fid".into() })?;
            
            if let Resource::Open(resource) = resource {
            let count = resource.write(offset, &data).await?;
            Ok(Rwrite { count }.into())
            } else {
            Err(Rerror { ename: "Invalid argument".into() })
            }
        },
        TMessage::Tclunk(Tclunk { fid }) => {
            let mut resources = resource_mgr.resources.write().await;
            if resources.remove(&fid).is_some() {
                Ok(Rclunk.into())
            } else {
                Err(Rerror { ename: "Fid not found".into() })
            }
        },
        TMessage::Tremove(Tremove { fid }) => {
            let mut resources = resource_mgr.resources.write().await;
            let resource = resources.remove(&fid).ok_or_else(|| Rerror { ename: "Fid not found".into() })?;
            
            match resource {
                Resource::Path(res) => res.remove().await?,
                Resource::Open(res) => res.remove().await?
            };
            
            Ok(Rremove.into())
        },
        TMessage::Tstat(Tstat { fid }) => {
            let resources = resource_mgr.resources.read().await;
            let resource = resources.get(&fid).ok_or_else(|| Rerror { ename: "Fid not found".into() })?;
            
            let stat = match resource {
                Resource::Path(res) => res.stat().await?,
                Resource::Open(res) => res.stat().await?
            };
            
            Ok(Rstat { stat }.into())
        },
        TMessage::Twstat(Twstat { fid, stat }) => {
            let resources = resource_mgr.resources.read().await;
            let resource = resources.get(&fid).ok_or_else(|| Rerror { ename: "unknown fid".into() })?;
            
            match resource {
                Resource::Path(res) => res.wstat(stat).await?,
                Resource::Open(res) => res.wstat(stat).await?,
            };
            
            Ok(Rwstat.into())
        }
    }
}

pub async fn handle_client<S: Serve2>(
    peer: impl AsyncRead + AsyncWrite,
    handler: Arc<S>
) -> io::Result<()> {
    let resource_mgr = Arc::new(ResourceManager {
        resources: RwLock::default(),
        handler: handler.clone(),
    });

    let peer = pin!(peer);
    let peer = NoiseStream::new(peer, Side::Responder { local_private_key: &super::PRIVATE_KEY }).await?;
    let mut framed = LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_type::<u32>()
        .length_adjustment(-4)
        .max_frame_length(MAX_MESSAGE_SIZE as usize - 4)
        .new_framed(peer);

    let mut inflight = pin!(FuturesUnordered::<TaggedFuture<_>>::new());

    let mut initialized = false;
    let mut next_session = None;

    loop {
        if inflight.is_empty() {
            if let Some(Tversion { msize, version }) = next_session.take() {
                // in-flight requests have been completely flushed out

                resource_mgr.resources.write().await.clear();

                let msize = msize.min(MAX_MESSAGE_SIZE);
                let version = if version == "9P2000" { "9P2000" } else { "unknown" };
                framed.codec_mut().set_max_frame_length(msize.checked_sub(4).unwrap() as usize);
                framed.send(Rversion { msize, version: version.into() }.serialize(!0).unwrap()).await?;

                initialized = true;
            }
        }

        tokio::select! {
            Some(incoming) = framed.next(), if inflight.len() < MAX_IN_FLIGHT && next_session.is_none() => {
                let incoming = incoming?;

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
                                    framed.send(Rerror {
                                        ename: "expected NOTAG".into()
                                    }.serialize(tag).unwrap()).await?;
                                }
                            },
                            TMessage::Tflush(Tflush { oldtag }) => {
                                if let Some(flushes) = inflight.as_mut().iter_pin_mut().find_map(|h| (h.tag == oldtag).then_some(h.project().flushes)) {
                                    *flushes = Some(tag);
                                } else {
                                    framed.send(Rflush.serialize(tag).unwrap()).await?;
                                }
                            },
                            req => {
                                inflight.push(TaggedFuture {
                                    tag,
                                    flushes: None,
                                    hdl: tokio::spawn(dispatch(
                                        resource_mgr.clone(),
                                        req,
                                        framed.codec().max_frame_length()
                                    ))
                                });
                            }
                        }
                    },
                    Err(e) => {
                        if let Some(tag) = e.tag() {
                            framed.send(Rerror {
                                ename: e.to_string().into()
                            }.serialize(tag).unwrap()).await?;
                        }
                    }
                }
            },
            Some((tag, flushes, resp)) = inflight.next() => {
                let resp = resp
                    .unwrap_or_else(|e| Err(e.into()))
                    .unwrap_or_else(RMessage::from);

                let resp = resp
                    .serialize(tag)
                    .unwrap_or_else(|e| Rerror::from(e).serialize(tag).unwrap());

                framed.feed(resp).await?;

                if let Some(flush) = flushes {
                    framed.feed(Rflush.serialize(flush).unwrap()).await?;
                }

                framed.flush().await?;
            },
            else => break
        }
    }

    Ok(())
}