use std::{future::Future, mem, pin::{pin, Pin}, sync::Arc, task::{Context, Poll}};

use bytes::Bytes;
use futures::{io, stream::FuturesUnordered, SinkExt, StreamExt};
use tokio::{io::{AsyncRead, AsyncWrite}, task::{Id, JoinError, JoinHandle}};
use tokio_util::codec::LengthDelimitedCodec;
use npwire::*;
use util::noise::{NoiseStream, Side};

use super::{Serve, Fid};

const MAX_IN_FLIGHT: usize = 16;
const MAX_MESSAGE_SIZE: u32 = 16384;

struct TaggedJoinHandle<T> {
    tag: u16,
    flushes: Vec<u16>,
    hdl: JoinHandle<T>
}

impl<T> Future for TaggedJoinHandle<T> {
    type Output = (u16, Vec<u16>, Result<T, JoinError>);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.hdl).poll(cx).map(|v| (
            self.tag,
            mem::take(&mut self.flushes),
            v
        ))
    }
}

async fn dispatch<S: Serve<Fid> + 'static>(
    handler: Arc<S>,
    connection_id: Id,
    request: TMessage,
    tag: u16
) -> Result<Bytes, Rerror> {
    match request {
        TMessage::Tversion(..) | TMessage::Tflush(..) => {
            unimplemented!()
        },
        TMessage::Tauth(Tauth { afid, uname, aname }) => {
            let aqid = handler.auth(Fid::new(connection_id, afid), &uname, &aname).await?;
            Ok(Rauth { aqid }.serialize(tag)?)
        },
        TMessage::Tattach(Tattach { fid, afid, uname, aname }) => {
            let qid = handler.attach(Fid::new(connection_id, fid), Fid::new(connection_id, afid), &uname, &aname).await?;
            Ok(Rattach { qid }.serialize(tag)?)
        },
        TMessage::Twalk(Twalk { fid, newfid, wname }) => {
            let wname = wname.iter().map(|s| &s[..]).collect();
            let wqid = handler.walk(Fid::new(connection_id, fid), Fid::new(connection_id, newfid), wname).await?;
            Ok(Rwalk { wqid: wqid.into_iter().collect() }.serialize(tag)?)
        },
        TMessage::Topen(Topen { fid, mode }) => {
            let (qid, iounit) = handler.open(Fid::new(connection_id, fid), mode).await?;
            Ok(Ropen { qid, iounit }.serialize(tag)?)
        },
        TMessage::Tcreate(Tcreate { fid, name, perm, mode }) => {
            let (qid, iounit) = handler.create(Fid::new(connection_id, fid), &name, perm, mode).await?;
            Ok(Rcreate { qid, iounit }.serialize(tag)?)
        },
        TMessage::Tread(Tread { fid, offset, count }) => {
            let fid = Fid::new(connection_id, fid);

            let data = handler.read(fid, offset, count).await?;
            Ok(Rread { data }.serialize(tag)?)
        },
        TMessage::Twrite(Twrite { fid, offset, data }) => {
            let count = handler.write(Fid::new(connection_id, fid), offset, &data[..]).await?;
            Ok(Rwrite { count }.serialize(tag)?)
        },
        TMessage::Tclunk(Tclunk { fid }) => {
            handler.clunk(Fid::new(connection_id, fid)).await?;
            Ok(Rclunk.serialize(tag)?)
        },
        TMessage::Tremove(Tremove { fid }) => {
            handler.remove(Fid::new(connection_id, fid)).await?;
            Ok(Rremove.serialize(tag)?)
        },
        TMessage::Tstat(Tstat { fid }) => {
            let stat = handler.stat(Fid::new(connection_id, fid)).await?;
            Ok(Rstat { stat }.serialize(tag)?)
        },
        TMessage::Twstat(Twstat { fid, stat }) => {
            handler.wstat(Fid::new(connection_id, fid), stat).await?;
            Ok(Rwstat.serialize(tag)?)
        }
    }
}

pub async fn handle_client<S: Serve<Fid> + 'static>(
    peer: impl AsyncRead + AsyncWrite,
    id: Id,
    handler: Arc<S>
) -> io::Result<()> {
    let peer = pin!(peer);
    let peer = NoiseStream::new_init(peer, &super::PRIVATE_KEY, Side::Responder).await?;
    let mut framed = LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_type::<u32>()
        .length_adjustment(-4)
        .new_framed(peer);

    let mut inflight = FuturesUnordered::<TaggedJoinHandle<Result<Bytes, Rerror>>>::new();

    loop {
        tokio::select! {
            biased;
            Some((tag, flushes, resp)) = inflight.next() => {
                let resp = resp
                    .unwrap_or_else(|e| Err(e.into()))
                    .or_else(|e| e.serialize(tag))
                    .unwrap_or_else(|e| Rerror::from(e).serialize(tag).unwrap());

                framed.send(resp).await?;

                for flush in flushes {
                    framed.send(Rflush.serialize(flush).unwrap()).await?;
                }
            },
            Some(incoming) = framed.next(), if inflight.len() < MAX_IN_FLIGHT => {
                let incoming = incoming?;

                let des = deserialize_t(incoming.freeze());
                match des {
                    Ok((tag, req)) => {
                        match req {
                            TMessage::Tversion(Tversion { msize, version }) => {
                                let msize = msize.min(MAX_MESSAGE_SIZE);
                                let version = if version == "9P2000" { "9P2000" } else { "unknown" };
                                framed.codec_mut().set_max_frame_length((msize - 4) as usize);
                                framed.send(Rversion { msize, version: version.into() }.serialize(tag).unwrap()).await?;
                            },
                            TMessage::Tflush(Tflush { oldtag }) => {
                                if let Some(flushes) = inflight.iter_mut().find_map(|h| (h.tag == oldtag).then_some(&mut h.flushes)) {
                                    flushes.push(tag);
                                } else {
                                    framed.send(Rflush.serialize(tag).unwrap()).await?;
                                }
                            },
                            req => {
                                inflight.push(TaggedJoinHandle {
                                    tag,
                                    flushes: Vec::new(),
                                    hdl: tokio::spawn(dispatch(
                                        handler.clone(),
                                        id,
                                        req,
                                        tag
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
            else => break
        }
    }

    Ok(())
}