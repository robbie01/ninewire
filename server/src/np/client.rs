use std::{collections::BTreeMap, future::ready, mem, sync::Arc};

use async_stream::try_stream;
use bytes::Bytes;
use bytestring::ByteString;
use futures::{FutureExt as _, Stream, StreamExt as _, io};
use tokio::sync::RwLock;
use npwire::*;
use transport::NpTransport;
use util::pooled::Pool;

use crate::np::client::{pin_array::PinArray, tagged::Tagged};

use super::{traits::{OpenResource as _, PathResource as _, Resource as _}, Serve};

mod pin_array;
mod tagged;

const MAX_IN_FLIGHT: usize = 16;

// 1280: IPv6 MTU
// 64: UDT combined overhead (IP+UDP+UDT)
// 8/16: nonce/tag
// const MAX_MESSAGE_SIZE: u32 = 1280 - 64 - 8 - 16;
const MAX_MESSAGE_SIZE: u32 = 131072;

#[derive(Debug)]
enum Resource<S: Serve> {
    Path(S::PathResource),
    Open(S::OpenResource)
}

struct ResourceManager<S: Serve> {
    resources: RwLock<BTreeMap<u32, Resource<S>>>,
    handler: Arc<S>,
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
        TMessage::Tversion(..) | TMessage::Tflush(..) | TMessage::Treads(..) => {
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
            let resource = resources.get(&fid).ok_or_else(|| rerror("fid invalid"))?;
            
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
                let count = count.min((maxlen - IOHDRSZ) as u32);
                let mut data = resource.read(offset, count).await?;
                data.truncate(maxlen - IOHDRSZ);
                Ok(Rread { data }.into())
            } else {
                Err(rerror("fid not open for read"))
            }
        },
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

fn dispatch_reads<S: Serve>(
    resource_mgr: &ResourceManager<S>,
    Treads { fid, mut offset, mut count }: Treads,
    maxlen: usize
) -> impl Stream<Item = Result<RMessage, Rerror>> + '_ {
    try_stream! {
        let resources = resource_mgr.resources.read().await;

        let resource = resources.get(&fid).ok_or_else(|| rerror("fid invalid"))?;
        
        if let Resource::Open(resource) = resource {
            while count > 0 {
                let n = count.min((maxlen - IOHDRSZ) as u32);
                let mut data = resource.read(offset, n).await?;
                data.truncate(maxlen - IOHDRSZ);

                offset += data.len() as u64;
                count -= data.len() as u32;

                yield Rreads { offset, data }.into();
            }
            yield Rreads { offset, data: Bytes::new() }.into();
        } else {
            Err(rerror("fid not open for read"))?;
        }
    }
}

pub async fn handle_client<T: NpTransport, S: Serve>(
    peer: T,
    handler: Arc<S>
) -> io::Result<()> {
    let resource_mgr = ResourceManager {
        resources: RwLock::default(),
        handler: handler.clone(),
    };

    let mut inflight = PinArray::new(MAX_IN_FLIGHT);

    let mut initialized = None;
    let mut next_session = None;

    let bufpool = Pool::new(|| vec![0u8; MAX_MESSAGE_SIZE as usize]);

    loop {
        if inflight.is_empty() {
            if let Some(Tversion { msize, version }) = next_session.take() {
                // in-flight requests have been completely flushed out
                resource_mgr.resources.write().await.clear();

                if msize < 256 {
                    peer.send(&rerror(
                        "Tversion: message size too small"
                    ).serialize(!0).unwrap()).await?;
                } else {
                    let msize = msize.min(MAX_MESSAGE_SIZE).min(peer.max_msize());
                    let version: &'static str = if version == "9P2000" { "9P2000" } else { "unknown" };
                    peer.send(&Rversion { msize, version: ByteString::from_static(version) }.serialize(!0).unwrap()).await?;
    
                    initialized = Some(msize);
                }
            }
        }

        // 2025-03-31: I have realized that I reinvented StreamExt::buffer_unordered
        // from first principles. Luckily, that method doesn't actually work directly
        // with what I need to do because of the flush stuff.
        let mut buffer = bufpool.get();

        tokio::select! {
            biased;
            incoming_n = peer.recv(&mut buffer), if !inflight.is_full() && next_session.is_none() => {
                let incoming_n = incoming_n?;
                let incoming = mem::replace(&mut buffer, bufpool.get());

                let mut incoming = Bytes::from_owner(incoming);
                incoming.truncate(incoming_n);

                let des = deserialize_t(incoming);

                if initialized.is_none() && !matches!(des, Ok((_, TMessage::Tversion(_)))) {
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
                                    inflight.push(Tagged::new(tag, ready(rerror(
                                        "Tversion: invalid tag"
                                    ).into()).into_stream().left_stream())).unwrap();
                                }
                            },
                            TMessage::Tflush(Tflush { oldtag }) => {
                                let found;
                                // mucking around with lifetimes
                                {
                                    if let Some(fut) = inflight.iter_mut().find(|h| h.tag() == oldtag) {
                                        found = true;
                                        fut.flush(tag);
                                    } else {
                                        found = false;
                                    }
                                }

                                if !found {
                                    inflight.push(Tagged::new(tag, ready(Rflush.into()).into_stream().left_stream()))
                                        .expect("inflight is full");
                                }
                            },
                            TMessage::Treads(req) => {
                                inflight.push(Tagged::new(tag,
                                    dispatch_reads(
                                        &resource_mgr,
                                        req,
                                        initialized.unwrap() as usize
                                    )
                                        .map(|resp| resp.unwrap_or_else(RMessage::from))
                                        .right_stream().right_stream()
                                )).unwrap();
                            },
                            req => {
                                inflight.push(Tagged::new(tag, 
                                    dispatch(
                                        &resource_mgr,
                                        req,
                                        initialized.unwrap() as usize
                                    )
                                        .map(|resp| resp.unwrap_or_else(RMessage::from))
                                        .into_stream().left_stream().right_stream()
                                )).unwrap();
                            }
                        }
                    },
                    Err(e) => {
                        if let Some(tag) = e.tag() {
                            inflight.push(Tagged::new(tag, ready(Rerror { ename: e.to_string().into() }.into()).into_stream().left_stream())).unwrap();
                        }
                    }
                }
            },
            Some((tag, resp)) = inflight.next() => {
                let serialized = resp
                    .serialize(tag)
                    .unwrap_or_else(|e| Rerror::from(e).serialize(tag).unwrap());

                peer.send(&serialized).await?;
            },
            else => break
        }
    }

    peer.flush().await?;

    Ok(())
}