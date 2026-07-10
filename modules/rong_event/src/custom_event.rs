use rong::{function::*, *};

use super::Event;

/// CustomEvent constructor options
#[derive(FromJSObject, Default)]
pub struct CustomEventOptions {
    #[js_default]
    pub bubbles: bool,
    #[js_default]
    pub cancelable: bool,
    #[js_default]
    pub composed: bool,
    pub detail: Option<JSValue>,
}

/// Represents a custom event object
#[js_class]
pub struct CustomEvent {
    detail: Option<JSValue>,
    event: Event,
}

#[js_class]
impl CustomEvent {
    #[js_method(constructor)]
    fn new(type_: String, options: Optional<CustomEventOptions>) -> Self {
        let opts = options.0.unwrap_or_default();

        // Create Event with the same options
        use super::event::EventOptions;
        let event_opts = EventOptions {
            bubbles: opts.bubbles,
            cancelable: opts.cancelable,
            composed: opts.composed,
        };

        Self {
            detail: opts.detail,
            event: Event::new(type_, Optional(Some(event_opts))),
        }
    }

    // Returns the custom data
    #[js_method(getter)]
    fn detail(&self) -> Option<JSValue> {
        self.detail.clone()
    }

    // Delegate Event methods
    #[js_method(getter, rename = "type")]
    fn type_(&self) -> String {
        self.event.type_()
    }

    #[js_method(getter)]
    fn bubbles(&self) -> bool {
        self.event.bubbles()
    }

    #[js_method(getter)]
    fn cancelable(&self) -> bool {
        self.event.cancelable()
    }

    #[js_method(getter)]
    fn composed(&self) -> bool {
        self.event.composed()
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        if let Some(detail) = self.detail.as_ref() {
            mark_fn(detail);
        }
    }
}
