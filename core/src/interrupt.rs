//! Cross-engine hard interruption.
//!
//! An [`InterruptHandle`] is a `Send + Sync` handle bound to one runtime. While
//! it is interrupted, the runtime aborts JavaScript execution: engines with
//! native preemption (QuickJS, JavaScriptCore) break even non-yielding code
//! such as `while (true) {}` with an uncatchable error, and every engine
//! rejects newly submitted evaluations until the request is cleared.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const SUPPORT_UNKNOWN: u8 = 0;
const SUPPORT_COOPERATIVE: u8 = 1;
const SUPPORT_PREEMPTIVE: u8 = 2;

/// The interruption behavior provided by a runtime's JavaScript engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InterruptMode {
    /// No runtime has been bound to the handle yet.
    #[default]
    Unbound,
    /// New evaluations are rejected, but already-running synchronous
    /// JavaScript cannot be preempted.
    Cooperative,
    /// Already-running JavaScript, including non-yielding code, is preempted.
    Preemptive,
}

impl InterruptMode {
    /// Whether already-running, non-yielding JavaScript can be preempted.
    pub const fn is_preemptive(self) -> bool {
        matches!(self, Self::Preemptive)
    }
}

#[derive(Debug, Default)]
struct InterruptRequests {
    persistent: bool,
    scoped: usize,
}

/// Thread-safe request to abort JavaScript execution on one runtime.
///
/// Obtain it from `JSRuntime::interrupt_handle()` or `Worker::interrupt_handle()`.
/// A persistent request made with [`interrupt`](Self::interrupt) remains active
/// until [`clear`](Self::clear). For independently owned requests, prefer
/// [`interrupt_scoped`](Self::interrupt_scoped), which cannot be cleared by
/// another scoped caller.
///
/// The handle remains safe to use after the runtime is gone; its local request
/// state still changes, but there is no longer a runtime to affect.
#[derive(Clone, Debug)]
pub struct InterruptHandle {
    flag: Arc<AtomicBool>,
    support: Arc<AtomicU8>,
    requests: Arc<Mutex<InterruptRequests>>,
}

impl Default for InterruptHandle {
    fn default() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            support: Arc::new(AtomicU8::new(SUPPORT_UNKNOWN)),
            requests: Arc::new(Mutex::new(InterruptRequests::default())),
        }
    }
}

impl InterruptHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Request interruption until [`clear`](Self::clear) is called.
    ///
    /// Running JavaScript is aborted on preemptive engines and new evaluations
    /// are rejected on every engine. Repeated calls are idempotent.
    pub fn interrupt(&self) {
        let mut requests = self.requests();
        requests.persistent = true;
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Clear this handle's persistent interruption request.
    ///
    /// Scoped requests remain active until their [`InterruptGuard`] is dropped.
    /// Clearing does not resume an aborted script; it only permits subsequent
    /// JavaScript execution.
    pub fn clear(&self) {
        let mut requests = self.requests();
        requests.persistent = false;
        self.flag.store(requests.scoped != 0, Ordering::SeqCst);
    }

    /// Request interruption for the lifetime of the returned guard.
    ///
    /// Scoped requests compose safely: dropping one guard does not clear a
    /// persistent request or another live guard's request.
    #[must_use = "dropping the guard immediately releases this interruption request"]
    pub fn interrupt_scoped(&self) -> InterruptGuard {
        let mut requests = self.requests();
        requests.scoped = requests
            .scoped
            .checked_add(1)
            .expect("too many concurrent interruption requests");
        self.flag.store(true, Ordering::SeqCst);
        drop(requests);

        InterruptGuard {
            flag: self.flag.clone(),
            requests: self.requests.clone(),
            active: true,
        }
    }

    /// Whether any persistent or scoped interruption request is active.
    pub fn is_interrupted(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// The interruption behavior of the bound engine.
    pub fn mode(&self) -> InterruptMode {
        match self.support.load(Ordering::SeqCst) {
            SUPPORT_PREEMPTIVE => InterruptMode::Preemptive,
            SUPPORT_COOPERATIVE => InterruptMode::Cooperative,
            _ => InterruptMode::Unbound,
        }
    }

    fn requests(&self) -> MutexGuard<'_, InterruptRequests> {
        self.requests.lock().unwrap_or_else(|err| err.into_inner())
    }

    /// The shared flag an engine's native interrupt hook polls.
    pub(crate) fn flag(&self) -> &Arc<AtomicBool> {
        &self.flag
    }

    /// Record whether the engine installed a preempting hook.
    pub(crate) fn bind_engine(&self, preemption: bool) {
        let value = if preemption {
            SUPPORT_PREEMPTIVE
        } else {
            SUPPORT_COOPERATIVE
        };
        self.support.store(value, Ordering::SeqCst);
    }
}

/// An independently owned interruption request.
///
/// The request is released on drop. Call [`clear`](Self::clear) to release it
/// explicitly before the end of its lexical scope.
#[derive(Debug)]
pub struct InterruptGuard {
    flag: Arc<AtomicBool>,
    requests: Arc<Mutex<InterruptRequests>>,
    active: bool,
}

impl InterruptGuard {
    /// Release this scoped request now.
    pub fn clear(mut self) {
        self.release();
    }

    fn release(&mut self) {
        if !self.active {
            return;
        }

        let mut requests = self.requests.lock().unwrap_or_else(|err| err.into_inner());
        debug_assert!(requests.scoped > 0);
        requests.scoped = requests.scoped.saturating_sub(1);
        self.flag.store(
            requests.persistent || requests.scoped != 0,
            Ordering::SeqCst,
        );
        self.active = false;
    }
}

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        self.release();
    }
}
