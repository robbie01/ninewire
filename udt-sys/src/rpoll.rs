use std::task::Waker;

use bitflags::bitflags;

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Event: u32 {
        const IN = 1;
        const OUT = 4;
        const ERR = 8;
    }
}

#[derive(Debug, Default)]
pub struct RPoll {
    evts: scc::HashMap<super::Socket, (Event, Vec<(Event, Waker)>)>
}

impl RPoll {
    pub fn update_events(&self, socket: super::Socket, events: Event, value: bool) {
        // println!("+ {socket:?} {events:?} {value:?}");
        let mut ent = self.evts.entry(socket).or_default();
        if value {
            ent.0 = ent.0.union(events);
        } else {
            ent.0 = ent.0.difference(events);
        }
        let status = ent.0;
        // println!("UPDATE: {socket:?} {status:?}");
        for (_, waker) in ent.1.extract_if(.., |&mut (interest, _)| interest.intersects(status)) {
            waker.wake();
        }
    }

    pub(crate) fn update_events_cxx(&self, socket: super::Socket, events: u32, value: bool) {
        self.update_events(socket, Event::from_bits_retain(events), value);
    }

    pub(crate) fn remove_usock(&self, socket: super::Socket) {
        self.evts.remove(&socket);
    }

    pub fn query(&self, socket: super::Socket) -> Event {
        self.evts.read(&socket, |_, &(v, _)| v).unwrap_or_default()
    }

    pub fn with_lock<R>(&self, socket: super::Socket, f: impl FnOnce(&mut Event) -> R) -> R {
        let mut ent = self.evts.entry(socket).or_default();
        f(&mut ent.0)
    }

    pub fn register(&self, socket: super::Socket, interest: Event, waker: &Waker) {
        let mut ent = self.evts.entry(socket).or_default();
        
        if ent.0.intersects(interest) {
            waker.wake_by_ref();
        } else {
            let mut registered = false;
            for (existing_interest, existing_waker) in ent.1.iter_mut() {
                if existing_waker.will_wake(waker) {
                    *existing_interest = existing_interest.union(interest);
                    registered = true;
                    break;
                }
            }
            if !registered {
                ent.1.push((interest, waker.clone()));
            }
        }
    }
}