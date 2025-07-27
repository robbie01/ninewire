use std::{cell::UnsafeCell, hash::{BuildHasher, Hash, RandomState}, mem::{self, ManuallyDrop}, num::NonZeroUsize, ops::Deref, ptr, sync::Arc};

use arc_swap::ArcSwap;
use rpds::HashTrieMapSync;
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockState {
    Unlocked,
    Exclusive,
    Shared(NonZeroUsize)
}

impl LockState {
    fn decrement_shared_count(&mut self) {
        match *self {
            Self::Unlocked => panic!("already unlocked"),
            Self::Exclusive => panic!("locked exclusively"),
            Self::Shared(n) => match NonZeroUsize::new(usize::from(n) - 1) {
                None => *self = Self::Unlocked,
                Some(n) => *self = Self::Shared(n)
            }
        }
    }

    fn increment_shared_count(&mut self) {
        match *self {
            Self::Unlocked => *self = Self::Shared(NonZeroUsize::new(1).unwrap()),
            Self::Exclusive => panic!("locked exclusively"),
            Self::Shared(n) => *self = Self::Shared(n.checked_add(1).expect("shared overflow"))
        }
    }

    fn unlock_exclusive(&mut self) {
        match *self {
            Self::Unlocked => panic!("already unlocked"),
            Self::Exclusive => *self = Self::Unlocked,
            Self::Shared(_) => panic!("locked shared")
        }
    }
}

struct InnerValue<V> {
    writer_waiting: bool,
    state: LockState,
    value: Arc<UnsafeCell<Option<V>>>
}

impl<V> Clone for InnerValue<V> {
    fn clone(&self) -> Self {
        Self {
            writer_waiting: self.writer_waiting,
            state: self.state,
            value: self.value.clone()
        }
    }
}

pub struct AsyncMap<K, V, H: BuildHasher = RandomState> {
    inner: ArcSwap<HashTrieMapSync<K, InnerValue<V>, H>>,
    notify: Notify
}

pub struct EntryGuard<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> {
    parent: &'a AsyncMap<K, V, H>,
    key: K,
    entry: Arc<UnsafeCell<Option<V>>>
}

impl<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> Deref for EntryGuard<'a, K, V, H> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        let opt = unsafe { &*self.entry.get()};
        opt.as_ref().expect("EntryGuard was created for None entry")
    }
}

impl<'a, K: Clone + Eq + Hash + Clone, V, H: BuildHasher + Clone> Drop for EntryGuard<'a, K, V, H> {
    fn drop(&mut self) {
        let mut writer_parked = None;

        self.parent.inner.rcu(|prev| {
            let mut next = prev.clone();
            {
                let next = Arc::make_mut(&mut next);
                let ent = next.get_mut(&self.key).expect("EntryGuard was created for nonexistent entry");
                ent.state.decrement_shared_count();
                writer_parked = Some(ent.writer_waiting);
            }
            next
        });

        let writer_parked = writer_parked.expect("rcu did not run");
        if writer_parked {
            self.parent.notify.notify_waiters();
        }
    }
}

pub struct VacantGuard<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> {
    parent: &'a AsyncMap<K, V, H>,
    key: K,
    entry: Arc<UnsafeCell<Option<V>>>
}

pub struct OccupiedGuard<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> {
    parent: &'a AsyncMap<K, V, H>,
    key: K,
    entry: Arc<UnsafeCell<Option<V>>>
}

pub enum EntryMut<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> {
    Vacant(VacantGuard<'a, K, V, H>),
    Occupied(OccupiedGuard<'a, K, V, H>)
}

impl<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> VacantGuard<'a, K, V, H> {
    pub fn insert(self, value: V) -> OccupiedGuard<'a, K, V, H> {
        unsafe {
            assert!((*self.entry.get()).is_none(), "VacantGuard was created for Some entry");
            *self.entry.get() = Some(value);
        }
        let mut prev = ManuallyDrop::new(self);
        unsafe { 
            OccupiedGuard {
                parent: ptr::read(&mut prev.parent),
                key: ptr::read(&mut prev.key),
                entry: ptr::read(&mut prev.entry)
            }
        }
    }

    pub fn key(&self) -> &K {
        &self.key
    }
}

impl<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> OccupiedGuard<'a, K, V, H> {
    pub fn get(&self) -> &V {
        let opt = unsafe { &*self.entry.get()};
        opt.as_ref().expect("OccupiedGuard was created for None entry")
    }

    pub fn get_mut(&mut self) -> &mut V {
        let opt = unsafe { &mut *self.entry.get()};
        opt.as_mut().expect("OccupiedGuard was created for None entry")
    }

    pub fn insert(&mut self, value: V) -> V {
        mem::replace(self.get_mut(), value)
    }

    pub fn key(&self) -> &K {
        &self.key
    }

    pub fn remove(self) -> V {
        let value = unsafe { &mut *self.entry.get() }.take().expect("OccupiedGuard was created for None entry");
        let mut prev = ManuallyDrop::new(self);
        unsafe {
            drop(VacantGuard {
                parent: ptr::read(&mut prev.parent),
                key: ptr::read(&mut prev.key),
                entry: ptr::read(&mut prev.entry)
            });
        }
        value
    }
}

impl<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> Drop for VacantGuard<'a, K, V, H> {
    fn drop(&mut self) {
        self.parent.inner.rcu(|prev | {
            let mut next = prev.clone();
            {
                let next = Arc::make_mut(&mut next);
                let ent = next.get(&self.key).expect("VacantGuard was created for nonexistent entry");
                assert!(Arc::ptr_eq(&self.entry, &ent.value), "inconsistent Arcs");
                assert!(unsafe { &*ent.value.get() }.is_none(), "VacantGuard was created for Some entry");
                next.remove_mut(&self.key);
            }
            next
        });
        self.parent.notify.notify_waiters();
    }
}

impl<'a, K: Clone + Eq + Hash, V, H: BuildHasher + Clone> Drop for OccupiedGuard<'a, K, V, H> {
    fn drop(&mut self) {
        self.parent.inner.rcu(|prev | {
            let mut next = prev.clone();
            {
                let next = Arc::make_mut(&mut next);
                let ent = next.get_mut(&self.key).expect("OccupiedGuard was created for nonexistent entry");
                assert!(Arc::ptr_eq(&self.entry, &ent.value), "inconsistent Arcs");
                assert!(unsafe { &*ent.value.get() }.is_some(), "OccupiedGuard was created for None entry");
                ent.state.unlock_exclusive();
            }
            next
        });
        self.parent.notify.notify_waiters();
    }
}

impl<K: Clone + Eq + Hash, V, H: BuildHasher + Clone> AsyncMap<K, V, H> {
    pub async fn get(&self, key: K) -> Option<EntryGuard<'_, K, V, H>> {
        let mut notified = self.notify.notified();
        let mut cur = self.inner.load();
        loop {
            let mut next = (*cur).clone();
            let value = {
                let next = Arc::make_mut(&mut next);
                let Some(ent) = next.get_mut(&key) else { return None };

                if ent.state == LockState::Exclusive || ent.writer_waiting {
                    drop(cur);
                    notified.await;
                    notified = self.notify.notified();
                    cur = self.inner.load();
                    continue;
                }
                
                ent.state.increment_shared_count();
                ent.value.clone()
            };

            notified = self.notify.notified(); // reregister before getting new value
            let prev = self.inner.compare_and_swap(&*cur, next);
            let swapped = Arc::ptr_eq(&*cur, &*prev);
            if swapped {
                return Some(EntryGuard {
                    parent: self,
                    key,
                    entry: value
                });
            } else {
                cur = prev;
            }
        }
    }

    pub async fn entry_mut(&self, key: K) -> EntryMut<'_, K, V, H> {
        let mut notified = self.notify.notified();
        let mut cur = self.inner.load();

        loop {
            let mut next = (*cur).clone();
            let value = {
                let next = Arc::make_mut(&mut next);
                let ent = next.get_mut(&key);

                match ent {
                    Some(ent) => {
                        if ent.state != LockState::Unlocked {
                            if !ent.writer_waiting {
                                // todo
                            }

                            drop(cur);
                            notified.await;
                            notified = self.notify.notified();
                            cur = self.inner.load();
                            continue;
                        }

                        ent.state = LockState::Exclusive;
                        ent.value.clone()
                    }
                    None => {
                        let value = Arc::new(UnsafeCell::new(None));
                        next.insert_mut(key.clone(), InnerValue {
                            writer_waiting: false,
                            state: LockState::Exclusive,
                            value: value.clone()
                        });
                        value
                    }
                }
            };

            // Attempt to swap
            let prev = self.inner.compare_and_swap(&*cur, next);
            if Arc::ptr_eq(&*cur, &*prev) {
                return if unsafe { &*value.get() }.is_some() {
                    EntryMut::Occupied(OccupiedGuard {
                        parent: self,
                        key,
                        entry: value,
                    })
                } else {
                    EntryMut::Vacant(VacantGuard {
                        parent: self,
                        key,
                        entry: value,
                    })
                };
            } else {
                cur = prev;
            }
        }
    }
}