use std::{cell::Cell, marker::PhantomData, sync::Arc};

use rsevents::{AutoResetEvent, Awaitable as _, EventState};

pub struct Unparker {
    event: Option<Arc<AutoResetEvent>>
}

// Similar to crossbeam::sync::Parker, but uses an AutoResetEvent instead
pub struct Parker {
    unparker: Unparker,
    _phantom: PhantomData<Cell<()>>
}

impl Parker {
    pub fn new() -> Self {
        Self {
            unparker: Unparker {
                event: Some(Arc::new(AutoResetEvent::new(EventState::Unset)))
            },
            _phantom: PhantomData
        }
    }

    pub fn unparker(&self) -> &Unparker {
        &self.unparker
    }

    pub fn park(&self) {
        self.unparker.event.as_ref().unwrap().wait()
    }
}

impl Unparker {
    pub fn noop() -> Self {
        Self { event: None }
    }

    pub fn unpark(self) {
        self.unpark_by_ref();
    }

    pub fn unpark_by_ref(&self) {
        if let Some(ref event) = self.event {
            event.set();
        }
    }

    pub fn will_unpark(&self, other: &Unparker) -> bool {
        match (&self.event, &other.event) {
            (Some(_), None) => false,
            (None, Some(_)) => false,
            (None, None) => true,
            (Some(left), Some(right)) => Arc::ptr_eq(left, right)
        }
    }
}

impl Clone for Unparker {
    fn clone(&self) -> Self {
        Self { event: self.event.clone() }
    }

    fn clone_from(&mut self, source: &Self) {
        if !self.will_unpark(source) {
            *self = source.clone()
        }
    }
}