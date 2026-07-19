//! Drop-in replacement for `use rong_test::*` that binds to the ArkJS engine.
//!
//! Generated test modules have their `use rong_test::*` rewritten to
//! `use crate::prelude::*` by build.rs. Since `rong` is a real dependency
//! (with `arkjs` feature), most types come directly from `rong::*`.

pub use rong::function::{Constructor, Optional, Rest, This, ThisMut};
pub use rong::*;

pub use rong_macro;
pub use rong_test::UnitJSRunner;

pub fn thrown_js_value(ctx: &JSContext, err: &RongJSError) -> JSResult<JSValue> {
    err.thrown_value(ctx)
        .ok_or_else(|| HostError::new(error::E_INTERNAL, "Expected thrown JS value").into())
}

pub fn thrown_object(ctx: &JSContext, err: &RongJSError) -> JSResult<JSObject> {
    let thrown = thrown_js_value(ctx, err)?;
    thrown.into_object().ok_or_else(|| {
        HostError::new(error::E_INTERNAL, "Expected thrown value to be an object").into()
    })
}

pub fn thrown_object_prop<T>(ctx: &JSContext, err: &RongJSError, key: &str) -> JSResult<T>
where
    T: FromJSValue<JSEngineValue>,
{
    thrown_object(ctx, err)?.get(key)
}

pub fn thrown_error_message(ctx: &JSContext, err: &RongJSError) -> JSResult<String> {
    thrown_object_prop(ctx, err, "message")
}

pub fn thrown_error_stack(ctx: &JSContext, err: &RongJSError) -> JSResult<String> {
    thrown_object_prop(ctx, err, "stack")
}

pub fn run<F: FnOnce(&JSContext) -> JSResult<()>>(f: F) {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    f(&ctx).unwrap();
}

#[macro_export]
macro_rules! async_run {
    ($user_fn:expr) => {{
        use $crate::prelude::*;
        let rong = Rong::<RongJS>::builder().shared().build().unwrap();
        let call_closure = |runtime: JSRuntime, _receiver| {
            let ctx = runtime.context();
            $user_fn(ctx)
        };
        rong.call_blocking::<_, _, ()>(call_closure).unwrap();
    }};
    (async $user_fn:expr) => {{
        use $crate::prelude::*;
        let rong = Rong::<RongJS>::builder().shared().build().unwrap();
        let call_closure = |runtime: JSRuntime, _receiver| {
            let ctx = runtime.context();
            $user_fn(ctx)
        };
        rong.call_blocking::<_, _, ()>(call_closure).unwrap();
    }};
}
