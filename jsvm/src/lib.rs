//! The JSVM backend for rong: HarmonyOS's JSVM-API (`libjsvm`, a V8), not
//! ArkTS's own runtime.
//!
//! # Handle scopes
//!
//! Each backend operation runs in a JSVM handle scope, so the handles it makes
//! are freed when it returns; values kept in Rust hold persistent references
//! and survive. Only the outermost operation on a context opens a scope: inside
//! a native function called from JS that Rust started, every operation joins
//! the running one, and its handles stay until that native call returns. A
//! native function doing a large batch of work should run each step in
//! `JSContext::handle_scope`, which always opens its own scope.

#[cfg(target_env = "ohos")]
mod class;
#[cfg(target_env = "ohos")]
mod context;
#[cfg(target_env = "ohos")]
mod hilog;
#[cfg(all(target_env = "ohos", feature = "intl"))]
pub mod intl;
#[cfg(target_env = "ohos")]
mod runtime;
#[cfg(target_env = "ohos")]
mod scope;
#[cfg(target_env = "ohos")]
mod value;

#[cfg(target_env = "ohos")]
mod jsvm {
    // Native low-level bindings for HarmonyOS JSVM
    pub use rong_jsvm_sys::*;
}

#[cfg(target_env = "ohos")]
pub use context::JSVMContext;
#[cfg(target_env = "ohos")]
pub use runtime::{JSVM, JSVMRuntime};
#[cfg(target_env = "ohos")]
pub use value::JSVMValue;

/// Writes one warning through the backend's HiLog reporter, so a device test
/// can confirm the fatal, out-of-memory and refused-reference reports reach it.
#[cfg(target_env = "ohos")]
#[doc(hidden)]
pub fn hilog_probe(message: &str) {
    hilog::warn(message);
}

#[cfg(not(target_env = "ohos"))]
pub struct JSVM;
#[cfg(not(target_env = "ohos"))]
pub struct JSVMRuntime;
#[cfg(not(target_env = "ohos"))]
pub struct JSVMContext;
#[cfg(not(target_env = "ohos"))]
pub struct JSVMValue;
