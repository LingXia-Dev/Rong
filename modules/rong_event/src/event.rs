use rong::{function::*, *};

/// Event constructor options
#[derive(FromJSObject, Default)]
pub struct EventOptions {
    #[js_default]
    pub bubbles: bool,
    #[js_default]
    pub cancelable: bool,
    #[js_default]
    pub composed: bool,
}

/// Represents an event object
#[js_class]
#[derive(Default)]
pub struct Event {
    pub(crate) type_: String,
    bubbles: bool,
    cancelable: bool,
    // whether the event will trigger listeners outside of a shadow root
    composed: bool,
}

#[js_class]
impl Event {
    #[js_method(constructor)]
    pub(crate) fn new(type_: String, options: Optional<EventOptions>) -> Self {
        let opts = options.0.unwrap_or_default();
        Self {
            type_,
            bubbles: opts.bubbles,
            cancelable: opts.cancelable,
            composed: opts.composed,
        }
    }

    // Returns the type of the event
    #[js_method(getter, rename = "type")]
    pub(crate) fn type_(&self) -> String {
        self.type_.clone()
    }

    // Returns whether the event bubbles
    #[js_method(getter)]
    pub(crate) fn bubbles(&self) -> bool {
        self.bubbles
    }

    // Returns whether the event can be canceled
    #[js_method(getter)]
    pub(crate) fn cancelable(&self) -> bool {
        self.cancelable
    }

    // Returns whether the event will trigger listeners outside of a shadow root
    #[js_method(getter)]
    pub(crate) fn composed(&self) -> bool {
        self.composed
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}
