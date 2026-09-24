use crate::AbortSignal;
use rong::{function::*, *};

#[js_class]
pub struct AbortController {
    /// The one JS object for this controller's signal. The signal's reason and
    /// listeners live in state every JS object of it shares and marks, so
    /// handing out a second object for the same signal made a GC cycle count
    /// them twice and free them early. `signal` returns this object every
    /// time, as the platform requires.
    signal: JSObject,
}

#[js_class]
impl AbortController {
    #[js_method(constructor)]
    fn new(ctx: JSContext) -> JSResult<Self> {
        let signal = Class::lookup::<AbortSignal>(&ctx)?.instance(AbortSignal::new(&ctx));
        Ok(Self { signal })
    }

    #[js_method(getter)]
    fn signal(&self) -> JSObject {
        self.signal.clone()
    }

    #[js_method]
    fn abort(&self, ctx: JSContext, reason: Optional<JSValue>) -> JSResult<()> {
        {
            let abort = self.signal.borrow::<AbortSignal>()?;
            if abort.aborted() {
                //only once
                return Ok(());
            }
            abort.set_reason(reason);
        }
        AbortSignal::broadcast_abort(&ctx, This(self.signal.clone()))
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        // The controller holds the signal object; the object marks its state.
        mark_fn(self.signal.as_js_value());
    }
}
