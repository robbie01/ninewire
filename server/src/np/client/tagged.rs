use std::{pin::Pin, task::{Context, Poll, ready}};

use futures::{Stream, stream::FusedStream};
use npwire::{RMessage, Rflush};
use pin_project::pin_project;

#[pin_project]
pub struct Tagged<S: ?Sized> {
    tag: u16,
    flushed_by: Option<u16>,
    terminated: bool,
    #[pin]
    inner: S
}

impl<S: ?Sized> Tagged<S> {
    pub fn new(tag: u16, inner: S) -> Self where S: Sized {
        Self {
            tag,
            flushed_by: None,
            terminated: false,
            inner
        }
    }

    pub fn tag(&self) -> u16 {
        self.tag
    }

    pub fn flush(self: Pin<&mut Self>, tag: u16) {
        let me = self.project();

        *me.flushed_by = Some(tag)
    }
}

impl<S: Stream<Item = RMessage> + ?Sized> Stream for Tagged<S> {
    type Item = (u16, RMessage);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = self.project();

        if *me.terminated {
            return Poll::Ready(None)
        }

        let next = ready!(me.inner.poll_next(cx));

        Poll::Ready(if let Some(m) = next {
            Some((*me.tag, m))
        } else if let Some(tag) = me.flushed_by.take() {
            // wake one last time to ensure caller receives EoS
            cx.waker().wake_by_ref();

            *me.terminated = true;
            Some((tag, Rflush.into()))
        } else {
            *me.terminated = true;
            None
        })
    }
}

impl<S: ?Sized> FusedStream for Tagged<S> where Tagged<S>: Stream {
    fn is_terminated(&self) -> bool {
        self.terminated
    }
}