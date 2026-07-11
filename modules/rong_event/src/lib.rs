//! Event handling implementation combining Web and Node.js patterns
//!
//! This module provides event handling functionality inspired by both Web APIs and Node.js,
//! with a unified implementation through the `EventEmitter` structure.
//!
//! ## Web Standard APIs:
//! - [`Event`]: The base event object
//! - [`CustomEvent`]: For custom events with additional data
//! - [`EventTarget`]: Interface for objects that can receive events
//!
//! ## Node.js APIs:
//! - [`EventEmitter`]: Node.js style event emitter with extended functionality
//! - [`Emitter`] and [`EmitterExt`]: Traits for implementing custom event emitters
//! - [`EmitError`]: Trait for converting Rust errors to JavaScript error events
//!
//! # Event Handling Patterns
//!
//! ## Web Style (EventTarget):
//! - Event listeners are called in the order they were registered
//! - Events are dispatched through the `dispatchEvent` method
//! - Basic event listener management with `addEventListener` and `removeEventListener`
//! - Note: The capture phase is currently not implemented
//!
//! ## Node.js Style (EventEmitter):
//! - Supports both string and symbol event types
//! - Configurable maximum number of listeners per event (default: 10)
//! - Rich API for event handling:
//!   - `on`/`addListener`: Add a listener
//!   - `once`: Add a one-time listener
//!   - `off`/`removeListener`: Remove a listener
//!   - `removeAllListeners`: Remove all listeners
//!   - `prependListener`: Add listener to the beginning
//!   - `prependOnceListener`: Add one-time listener to the beginning
//!   - `emit`: Trigger an event
//!   - `eventNames`: Get all registered event types
//!   - `setMaxListeners`/`getMaxListeners`: Configure listener limits
//!
//! ## Error Handling:
//! The module provides a seamless way to handle Rust errors in JavaScript:
//! - Automatic conversion of Rust errors to JavaScript error events
//! - Node.js-style error event emission
//! - Fallback to throwing errors when no error listeners are present
//!
//! # Event Object Properties
//!
//! Event objects have the following read-only properties:
//!
//! - `type`: The event type string
//! - `bubbles`: Whether the event bubbles up through the DOM
//! - `cancelable`: Whether the event can be canceled
//! - `composed`: Whether the event will trigger listeners outside of a shadow root
//!
//! CustomEvent objects additionally have:
//!
//! - `detail`: Custom data passed when creating the event
//!
//! # Implementation Details
//!
//! The module uses a unified `EventEmitter` structure, which provides:
//! - Thread-safe event listener management through `Mutex`
//! - Support for both string and symbol event types via `EventKey`
//! - Automatic cleanup of one-time listeners
//! - Configurable listener limits per event type
//! - Order-preserving listener execution
//! - Integrated error handling with event emission

mod custom_event;
mod event;
mod event_emitter;
mod event_target;

pub use custom_event::CustomEvent;
pub use event::Event;
pub use event_emitter::{EmitError, Emitter, EmitterExt, EventEmitter, EventKey};
pub use event_target::EventTarget;

use rong::*;

/// Register event-related classes with the JavaScript engine
pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<EventEmitter>()?;
    EventEmitter::add_node_event_target_prototype(ctx)?;
    EventEmitter::add_web_event_target_prototype(ctx)?;

    ctx.register_class::<Event>()?;
    ctx.register_class::<CustomEvent>()?;

    ctx.register_class::<EventTarget>()?;
    EventTarget::add_web_event_target_prototype(ctx)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_emit_error() {
        async_run!(|ctx: JSContext| async move {
            // Create a test error emitter
            #[js_class]
            struct TestEmitter {
                events: EventEmitter,
            }

            #[js_class]
            impl TestEmitter {
                #[js_method(constructor)]
                fn new() -> Self {
                    Self {
                        events: EventEmitter::new(),
                    }
                }

                #[js_method]
                fn do_operation(&self, this: This<JSObject>, ctx: JSContext) -> JSResult<bool> {
                    let result: Result<(), String> = Err("Test error message".to_string());
                    result.emit_error::<Self>(this, &ctx, "do_operation")
                }

                #[js_method(gc_mark)]
                fn gc_mark_with<F>(&self, mark_fn: F)
                where
                    F: FnMut(&JSValue),
                {
                    self.events.gc_mark_with(mark_fn);
                }
            }

            impl Emitter for TestEmitter {
                fn get_event_emitter(&self) -> EventEmitter {
                    self.events.clone()
                }
            }

            ctx.global().set(
                "print",
                JSFunc::new(&ctx, |msg: String| println!("{}", msg)),
            )?;

            // Register the test emitter
            ctx.register_class::<TestEmitter>()?;
            TestEmitter::add_node_event_target_prototype(&ctx)?;

            let result = ctx.eval::<bool>(Source::from_bytes(
                r#"
                let errorCaught = false;
                const testEmitter=new TestEmitter();
                testEmitter.on('error', (err) => {
                    print("Got:"+err);
                    errorCaught = err === "Test error message";
                });
                testEmitter.do_operation();
                errorCaught;
            "#,
            ))?;

            assert!(result, "Error event should have been caught by listener");

            Ok(())
        });
    }

    #[test]
    fn test_event() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            rong_assert::init(&ctx)?;
            rong_timer::init(&ctx)?;
            rong_console::init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "event.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }
}
