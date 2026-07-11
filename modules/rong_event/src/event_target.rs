use crate::event_emitter::{Emitter, EventEmitter};
use rong::*;

/// Represents an event target that can receive events and may have listeners for them.
#[js_class]
pub struct EventTarget {
    events: EventEmitter,
}

#[js_class]
impl EventTarget {
    #[js_method(constructor)]
    fn new() -> Self {
        Self {
            events: EventEmitter::new(),
        }
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        self.events.gc_mark_with(mark_fn);
    }
}

impl Default for EventTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter for EventTarget {
    fn get_event_emitter(&self) -> EventEmitter {
        self.events.clone()
    }
}
