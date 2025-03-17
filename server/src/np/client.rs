use std::{future::Future, pin::{pin, Pin}, sync::Arc, task::{Context, Poll}};

use futures::{io, stream::FuturesUnordered, SinkExt, StreamExt};
use pin_project::pin_project;
use tokio::{io::{AsyncRead, AsyncWrite}, task::Id};
use tokio_util::codec::LengthDelimitedCodec;
use npwire::*;
use util::noise::{NoiseStream, Side};

use super::{Serve, MuxFid};

const MAX_IN_FLIGHT: usize = 16;
const MAX_MESSAGE_SIZE: u32 = 65535 - 16;

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

async fn dispatch<S: Serve<Fid = MuxFid>>(
    handler: Arc<S>,
    connection_id: Id,
    request: TMessage,
    maxlen: usize
) -> Result<RMessage, Rerror> {
    match request {
        TMessage::Tversion(..) | TMessage::Tflush(..) => {
            unimplemented!()
        },
        TMessage::Tauth(Tauth { afid, uname, aname }) => {
            let aqid = handler.auth(MuxFid::new(connection_id, afid), &uname, &aname).await?;
            Ok(Rauth { aqid }.into())
        },
        TMessage::Tattach(Tattach { fid, afid, uname, aname }) => {
            let qid = handler.attach(MuxFid::new(connection_id, fid), MuxFid::new(connection_id, afid), &uname, &aname).await?;
            Ok(Rattach { qid }.into())
        },
        TMessage::Twalk(Twalk { fid, newfid, wname }) => {
            let wname = wname.iter().map(|s| &s[..]).collect::<Vec<_>>();
            let wqid = handler.walk(MuxFid::new(connection_id, fid), MuxFid::new(connection_id, newfid), &wname).await?;
            Ok(Rwalk { wqid: wqid.into_iter().collect() }.into())
        },
        TMessage::Topen(Topen { fid, mode }) => {
            let (qid, iounit) = handler.open(MuxFid::new(connection_id, fid), mode).await?;
            Ok(Ropen { qid, iounit }.into())
        },
        TMessage::Tcreate(Tcreate { fid, name, perm, mode }) => {
            let (qid, iounit) = handler.create(MuxFid::new(connection_id, fid), &name, perm, mode).await?;
            Ok(Rcreate { qid, iounit }.into())
        },
        TMessage::Tread(Tread { fid, offset, count }) => {
            // Bound count by the maximum frame size
            let maxcount = (maxlen - RREAD_OVERHEAD).try_into().unwrap_or(u32::MAX);
            let count = count.min(maxcount);

            let fid = MuxFid::new(connection_id, fid);

            let data = handler.read(fid, offset, count).await?;
            Ok(Rread { data }.into())
        },
        TMessage::Twrite(Twrite { fid, offset, data }) => {
            let count = handler.write(MuxFid::new(connection_id, fid), offset, &data[..]).await?;
            Ok(Rwrite { count }.into())
        },
        TMessage::Tclunk(Tclunk { fid }) => {
            handler.clunk(MuxFid::new(connection_id, fid)).await?;
            Ok(Rclunk.into())
        },
        TMessage::Tremove(Tremove { fid }) => {
            handler.remove(MuxFid::new(connection_id, fid)).await?;
            Ok(Rremove.into())
        },
        TMessage::Tstat(Tstat { fid }) => {
            let stat = handler.stat(MuxFid::new(connection_id, fid)).await?;
            Ok(Rstat { stat }.into())
        },
        TMessage::Twstat(Twstat { fid, stat }) => {
            handler.wstat(MuxFid::new(connection_id, fid), stat).await?;
            Ok(Rwstat.into())
        }
    }
}

pub async fn handle_client<S: Serve<Fid = MuxFid>>(
    peer: impl AsyncRead + AsyncWrite,
    id: Id,
    handler: Arc<S>
) -> io::Result<()> {
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

                handler.clunk_where(|fid| fid.connection_id == id).await;

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
                                    // https://9fans.github.io/plan9port/man/man9/flush.html
                                    // "it need respond only to the last flush"
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
                                        handler.clone(),
                                        id,
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