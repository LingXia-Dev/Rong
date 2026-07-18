//! Cross-engine hard interruption.
//!
//! An [`InterruptHandle`] is a `Send + Sync` handle bound to one runtime. While
//! it is interrupted, the runtime aborts JavaScript execution: engines with
//! native preemption (QuickJS, JavaScriptCore) break even non-yielding code
//! such as `while (true) {}` with an uncatchable error, and every engine
//! rejects newly submitted evaluations until the handle is cleared.
//!
//! Whether the bound engine can preempt running code is reported by
//! [`InterruptHandle::engine_preemption`] once a runtime has been created for
//! the handle.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

// 0 = unknown (no runtime bound yet)
const SUPPORT_NO: u8 = 1;
const SUPPORT_YES: u8 = 2;

/// Thread-safe request to abort JavaScript execution on one runtime.
///
/// Obtain it from `JSRuntime::interrupt_handle()` or `Worker::interrupt_handle()`.
/// The handle stays valid after the runtime is gone; interrupting then is a
/// no-op.
#[derive(Clone, Debug, Default)]
pub struct InterruptHandle {
    flag: Arc<AtomicBool>,
    support: Arc<AtomicU8>,
}

impl InterruptHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Request interruption: running JavaScript is aborted (on engines with
    /// preemption) and new evaluations are rejected until [`clear`](Self::clear).
    pub fn interrupt(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Allow JavaScript execution again.
    pub fn clear(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }

    pub fn is_interrupted(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Whether the bound engine preempts non-yielding JavaScript.
    ///
    /// `None` until a runtime has been created for this handle. `Some(false)`
    /// means only the cooperative layer applies: running synchronous code
    /// cannot be broken, but new evaluations are rejected while interrupted.
    pub fn engine_preemption(&self) -> Option<bool> {
        match self.support.load(Ordering::SeqCst) {
            SUPPORT_YES => Some(true),
            SUPPORT_NO => Some(false),
            _ => None,
        }
    }

    /// The shared flag an engine's native interrupt hook polls.
    pub(crate) fn flag(&self) -> &Arc<AtomicBool> {
        &self.flag
    }

    /// Record whether the engine installed a preempting hook.
    pub(crate) fn bind_engine(&self, preemption: bool) {
        let value = if preemption { SUPPORT_YES } else { SUPPORT_NO };
        self.support.store(value, Ordering::SeqCst);
    }
}
