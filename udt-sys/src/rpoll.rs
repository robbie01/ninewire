use std::sync::Arc;

use bitflags::bitflags;
use crossbeam_utils::atomic::AtomicCell;
use static_assertions::const_assert;
use tokio::sync::Notify;
use util::polymur;

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Event: u32 {
        const IN = 1;
        const OUT = 4;
    }
}

const_assert!(AtomicCell::<Event>::is_lock_free());

#[derive(Debug, Default)]
pub struct SocketData {
    readable: Arc<Notify>,
    writable: Arc<Notify>
}

#[derive(Debug, Default)]
pub struct RPoll {
    evts: scc::HashMap<super::Socket, SocketData, polymur::RandomState>
}

impl RPoll {
    pub fn update_events(&self, socket: super::Socket, events: Event, value: bool) {
        let ent = self.evts.entry(socket).or_default();
        if value {
            if events.contains(Event::IN) {
                ent.readable.notify_waiters();
            }
            if events.contains(Event::OUT) {
                ent.writable.notify_waiters();
            }
        }
    }

    pub(crate) fn update_events_cxx(&self, socket: super::Socket, events: u32, value: bool) {
        self.update_events(socket, Event::from_bits_retain(events), value);
    }

    pub(crate) fn remove_usock(&self, socket: super::Socket) {
        self.evts.remove(&socket);
    }

    pub fn readable(&self, socket: super::Socket) -> Option<impl Future<Output = ()>> {
        self.evts.read(&socket, |_, ent| ent.readable.clone().notified_owned())
    }

    pub fn writable(&self, socket: super::Socket) -> Option<impl Future<Output = ()>> {
        self.evts.read(&socket, |_, ent| ent.writable.clone().notified_owned())
    }
}