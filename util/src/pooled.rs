use std::{ops::{Deref, DerefMut}, sync::{Arc, Weak}};

use scc::Bag;

#[derive(Debug)]
pub struct Pool<F: ?Sized, T> {
    bag: Arc<Bag<T>>,
    f: F
}

#[derive(Debug, Clone)]
pub struct Pooled<T> {
    bag: Weak<Bag<T>>,
    v: Option<T>
}

impl<F: ?Sized, T> Pool<F, T> {
    pub fn new(f: F) -> Self where F: Sized {
        Self {
            bag: Default::default(),
            f
        }
    }

    pub fn get(&self) -> Pooled<T> where F: Fn() -> T {
        Pooled {
            bag: Arc::downgrade(&self.bag),
            v: Some(self.bag.pop().unwrap_or_else(&self.f))
        }
    }
}

impl<T> Drop for Pooled<T> {
    fn drop(&mut self) {
        if let Some(bag) = self.bag.upgrade() {
            bag.push(self.v.take().unwrap());
        }
    }
}

impl<T> Deref for Pooled<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.v.as_ref().unwrap()
    }
}

impl<T> DerefMut for Pooled<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.v.as_mut().unwrap()
    }
}

impl<U: ?Sized, T: AsRef<U>> AsRef<U> for Pooled<T> {
    fn as_ref(&self) -> &U {
        (**self).as_ref()
    }
}