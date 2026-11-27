//! Cranelift baseline JIT compiler for QuickJS. Turned on by the `jit` feature
//! (`rong`'s `quickjs-jit`); reached as `rong::quickjs::jit`.
//!
//! Call [`init`] with a QuickJS context to enable JIT compilation of hot
//! bytecode functions on its owning runtime. Without the feature, QuickJS runs
//! purely interpreted with zero JIT hook overhead.
//!
//! Compiled native code stays allocated on the owning runtime until that
//! runtime is dropped; keep `hot_threshold > 1` (the default is `16`) if a
//! workload dynamically creates and discards many short-lived functions.

use crate::{QJSContext, qjs};
use rong_core::{HostError, JSContext, JSResult};

pub use rong_quickjs_jit::{JitConfig, JitStats, is_supported};

fn raw_runtime(ctx: &JSContext<QJSContext>) -> *mut qjs::JSRuntime {
    unsafe { qjs::JS_GetRuntime(ctx.as_ref().ctx) }
}

/// Enables the Cranelift baseline JIT on the runtime owning `ctx` with the
/// given `config`.
///
/// Returns `Ok(true)` when the JIT backend is active on this target, or
/// `Ok(false)` on targets where JIT compilation is unavailable (such as iOS),
/// where execution falls back cleanly to the QuickJS interpreter.
pub fn init(ctx: &JSContext<QJSContext>, config: JitConfig) -> JSResult<bool> {
    let raw = raw_runtime(ctx);
    unsafe { rong_quickjs_jit::init_raw(raw, config) }.map_err(|msg| {
        HostError::new(
            rong_core::error::E_INTERNAL,
            format!("quickjs jit init failed: {msg}"),
        )
        .into()
    })
}

/// Enables or disables JIT execution on the runtime owning `ctx` after
/// [`init`].
///
/// Returns `false` if [`init`] was not called on this runtime (or on
/// unsupported targets).
pub fn set_enabled(ctx: &JSContext<QJSContext>, enabled: bool) -> bool {
    let raw = raw_runtime(ctx);
    unsafe { rong_quickjs_jit::set_enabled_raw(raw, enabled) }
}

/// Returns a snapshot of JIT compilation and execution statistics for the
/// runtime owning `ctx`, if [`init`] was called.
pub fn stats(ctx: &JSContext<QJSContext>) -> Option<JitStats> {
    let raw = raw_runtime(ctx);
    unsafe { rong_quickjs_jit::stats_raw(raw) }
}
