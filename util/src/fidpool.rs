use std::{collections::BTreeSet, ptr, sync::{Arc, Mutex, Weak}};

#[derive(Debug, Default)]
pub struct FidPoolInner {
    next: u32,
    free_list: BTreeSet<u32>
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

        let fid = if let Some(fid) = inner.free_list.pop_first() {
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

    pub fn is_nofid(&self) -> bool {
        self.fid == !0
    }

    pub fn is_of(&self, pool: &FidPool) -> bool {
        ptr::addr_eq(
            self.parent.as_ptr(),
            Arc::as_ptr(&pool.inner)
        )
    }
}

impl Default for FidHandle {
    fn default() -> Self {
        Self {
            fid: !0,
            parent: Weak::new()
        }
    }
}

impl Drop for FidHandle {
    fn drop(&mut self) {
        let Some(parent) = self.parent.upgrade() else { return };
        let mut parent = parent.lock().unwrap();
        assert!(self.fid < parent.next);
        assert!(parent.free_list.insert(self.fid));
    }
}