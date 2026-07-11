use crate::AbortSignal;
use rong::{function::*, *};

#[js_class]
pub struct AbortController {
    abort_signal: AbortSignal,
}

#[js_class]
impl AbortController {
    #[js_method(constructor)]
    fn new(ctx: JSContext) -> JSResult<Self> {
        Ok(Self {
            abort_signal: AbortSignal::new(&ctx),
        })
    }

    #[js_method(getter)]
    fn signal(&self) -> AbortSignal {
        self.abort_signal.clone()
    }

    #[js_method]
    fn abort(&self, ctx: JSContext, reason: Optional<JSValue>) -> JSResult<()> {
        let abort = &self.abort_signal;
        if abort.aborted() {
            //only once
            return Ok(());
        }
        abort.set_reason(reason);

        let obj = Class::lookup::<AbortSignal>(&ctx)?.instance(abort.clone());
        AbortSignal::broadcast_abort(&ctx, This(obj))?;
        Ok(())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        self.abort_signal.gc_mark_with(mark_fn);
    }
}
