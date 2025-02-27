use std::{cmp::Reverse, collections::BinaryHeap, sync::{Arc, Mutex, Weak}};

#[derive(Debug, Default)]
pub struct FidPoolInner {
    next: u32,
    free_list: BinaryHeap<Reverse<u32>>
}

#[derive(Debug, Default)]
pub struct FidPool {
    inner: Arc<Mutex<FidPoolInner>>
}

#[derive(Debug)]
pub struct FidHandle {
    fid: u32,
    parent: Weak<Mutex<FidPoolInner>>
}

impl FidPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Option<FidHandle> {
        let mut inner = self.inner.lock().ok()?;

        let fid = if let Some(Reverse(fid)) = inner.free_list.pop() {
            fid
        } else {
            let old_next = inner.next;
            inner.next = inner.next.checked_add(1)?;
            old_next
        };

        Some(FidHandle { fid, parent: Arc::downgrade(&self.inner) })
    }
}

impl FidHandle {
    pub fn fid(&self) -> u32 {
        self.fid
    }
}

impl Drop for FidHandle {
    fn drop(&mut self) {
        let Some(parent) = self.parent.upgrade() else { return };
        let Ok(mut parent) = parent.lock() else { return };
        parent.free_list.push(Reverse(self.fid));
    }
}