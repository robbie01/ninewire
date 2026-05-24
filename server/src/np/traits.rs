use std::{fmt::Display, future::Future, pin::Pin, task::{Context, Poll}};

use bytes::Bytes;

use npwire::{Qid, Stat};
use pin_project::pin_project;

pub trait IsCancelSafe {
    fn is_cancel_safe(&self) -> bool;
}

#[pin_project]
pub struct CancelSafe<T: ?Sized>(#[pin] T);

#[pin_project]
pub struct CancelUnsafe<T: ?Sized>(#[pin] T);

pub fn cancel_safe<T>(v: T) -> CancelSafe<T> {
    CancelSafe(v)
}

pub fn cancel_unsafe<T>(v: T) -> CancelUnsafe<T> {
    CancelUnsafe(v)
}

impl<T: Future + ?Sized> Future for CancelSafe<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll(cx)
    }
}

impl<T: Future + ?Sized> Future for CancelUnsafe<T> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll(cx)
    }
}

impl<T: ?Sized> IsCancelSafe for CancelSafe<T> {
    fn is_cancel_safe(&self) -> bool {
        true
    }
}

impl<T: ?Sized> IsCancelSafe for CancelUnsafe<T> {
    fn is_cancel_safe(&self) -> bool {
        false
    }
}

pub trait Resource: Send {
    type Error: Display;

    fn qid(&self) -> Qid;
    fn remove(self) -> impl Future<Output = Result<(), Self::Error>> + IsCancelSafe + Send;
    fn stat(&self) -> impl Future<Output = Result<Stat, Self::Error>> + IsCancelSafe + Send;
    fn wstat(&self, stat: Stat) -> impl Future<Output = Result<(), Self::Error>> + IsCancelSafe + Send;
}

pub trait PathResource: Resource + Sized + Send + Sync {
    type OpenResource: OpenResource;

    fn walk(&self, wname: &[&str]) -> impl Future<Output = Result<(Vec<Qid>, Option<Self>), Self::Error>> + IsCancelSafe + Send;
    fn open(&self, mode: u8) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + IsCancelSafe + Send;
    fn create(&self, name: &str, perm: u32, mode: u8) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + IsCancelSafe + Send;
}

pub trait OpenResource: Resource + Send + Sync {
    fn read(&self, offset: u64, count: u32) -> impl Future<Output = Result<Bytes, Self::Error>> + IsCancelSafe + Send;
    fn write(&self, offset: u64, data: &[u8]) -> impl Future<Output = Result<u32, Self::Error>> + IsCancelSafe + Send;
}

pub trait Serve: Send + Sync + 'static {
    type Error: Display;

    type PathResource: PathResource<Error = Self::Error, OpenResource = Self::OpenResource>;
    type OpenResource: OpenResource<Error = Self::Error>;

    fn auth(&self, uname: &str, aname: &str) -> impl Future<Output = Result<Self::OpenResource, Self::Error>> + IsCancelSafe + Send;
    fn attach(&self, ares: Option<&Self::OpenResource>, uname: &str, aname: &str) -> impl Future<Output = Result<Self::PathResource, Self::Error>> + IsCancelSafe + Send;
}