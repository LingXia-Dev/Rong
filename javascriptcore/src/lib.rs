mod class;
mod context;
// Only Apple's system framework is known to schedule JSC's timers on the
// thread's CFRunLoop. Source/JSCOnly builds, even on Apple targets, keep the
// engine default.
#[cfg(all(target_vendor = "apple", not(jsc_source)))]
mod run_loop;
mod runtime;
mod value;

mod jsc {
    // Native low-level bindings
    pub use rong_jscore_sys::*;

    pub(crate) trait IntoAttributes {
        fn into_attributes(self) -> u32;
    }

    impl IntoAttributes for u32 {
        fn into_attributes(self) -> u32 {
            self
        }
    }

    impl IntoAttributes for i32 {
        fn into_attributes(self) -> u32 {
            self as u32
        }
    }

    pub(crate) fn attr<T: IntoAttributes>(value: T) -> u32 {
        value.into_attributes()
    }
}

pub use context::JSCContext;
pub use runtime::{JSCRuntime, JavaScriptCore};
pub use value::JSCValue;
