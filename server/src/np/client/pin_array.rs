#![allow(unsafe_code)]

use std::{fmt::Debug, iter, pin::Pin, task::{Context, Poll}};

use futures::Stream;

#[derive(Debug)]
pub struct PinArray<T>(Box<[Option<T>]>);

pub struct Full<T: ?Sized>(pub T);

impl<T: ?Sized> Debug for Full<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("array is full")
    }
}

impl<T> PinArray<T> {
    pub fn new(capacity: usize) -> Self {
        Self(iter::repeat_with(|| None).take(capacity).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    pub fn is_full(&self) -> bool {
        self.0.iter().all(Option::is_some)
    }

    pub fn push(&mut self, value: T) -> Result<(), Full<T>> {
        let mut value = Some(value);

        for item in &mut self.0 {
            if item.is_none() {
                *item = value.take();
                break;
            }
        }

        match value {
            None => Ok(()),
            Some(value) => Err(Full(value))
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = Pin<&mut T>> {
        self.0.iter_mut()
            .filter_map(|v| v.as_mut().map(|v| unsafe { Pin::new_unchecked(v) }))
    }
}

impl<T: Stream> Stream for PinArray<T> {
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        let mut empty = true;

        for o in &mut this.0 {
            if let Some(item) = o {
                let item = unsafe { Pin::new_unchecked(item) };

                match item.poll_next(cx) {
                    Poll::Ready(Some(v)) => return Poll::Ready(Some(v)),
                    Poll::Ready(None) => {
                        *o = None;
                    },
                    Poll::Pending => empty = false
                }
            }
        }

        if empty {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}