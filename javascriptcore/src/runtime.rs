use crate::{JSCContext, JSCValue, jsc};
use rong_core::{JSEngine, JSRuntimeImpl};
#[cfg(jsc_interrupt)]
use std::cell::Cell;
#[cfg(jsc_interrupt)]
use std::sync::Arc;
#[cfg(jsc_interrupt)]
use std::sync::atomic::{AtomicBool, Ordering};

/// How often JSC's watchdog consults the callback while JavaScript runs.
#[cfg(jsc_interrupt)]
const WATCHDOG_QUANTUM_SECONDS: f64 = 0.1;

/// Callback context for the execution-time-limit watchdog. Heap-allocated per
/// runtime and freed on drop, after the limit is cleared.
#[cfg(jsc_interrupt)]
struct WatchdogState {
    group: *const jsc::OpaqueJSContextGroup,
    flag: Arc<AtomicBool>,
}

pub struct JSCRuntime {
    raw: *const jsc::OpaqueJSContextGroup,
    /// The installed watchdog's callback context (`*mut WatchdogState`), or
    /// null when no interrupt flag was installed.
    #[cfg(jsc_interrupt)]
    watchdog_state: Cell<*mut WatchdogState>,
}

/// Invoked on the JS thread each time the execution time limit elapses.
/// Returning `true` terminates the script with an uncatchable termination
/// exception, which breaks `while (true) {}`.
///
/// The system framework does NOT re-arm the limit when the callback returns
/// `false` (observed: it fires exactly once per arming), so the callback
/// re-arms itself — `JSContextRefPrivate.h` documents that re-arming from
/// within the callback is safe.
#[cfg(jsc_interrupt)]
unsafe extern "C" fn should_terminate(
    _ctx: jsc::JSContextRef,
    context: *mut std::os::raw::c_void,
) -> bool {
    let state = unsafe { &*(context as *const WatchdogState) };
    if state.flag.load(Ordering::Relaxed) {
        return true;
    }
    unsafe {
        jsc::JSContextGroupSetExecutionTimeLimit(
            state.group,
            WATCHDOG_QUANTUM_SECONDS,
            Some(should_terminate),
            context,
        );
    }
    false
}

impl JSRuntimeImpl for JSCRuntime {
    type RawRuntime = *const jsc::OpaqueJSContextGroup;
    type Context = JSCContext;

    fn new() -> Self {
        // On the source/JSCOnly backend, force JSC's one-time global init before
        // the first JSC API call. `JSContextGroupCreate` is the earliest VM touch
        // (earlier than `JSCContext::new`), so the guard belongs here. Idempotent
        // and thread-safe; a no-op on the system framework, which runs this from
        // the dylib's static initializers.
        #[cfg(jsc_source)]
        jsc::ensure_initialized();
        Self {
            raw: unsafe { jsc::JSContextGroupCreate() },
            #[cfg(jsc_interrupt)]
            watchdog_state: Cell::new(std::ptr::null_mut()),
        }
    }

    fn to_raw(&self) -> Self::RawRuntime {
        self.raw
    }

    // JavaScriptCore  GC works on Conext level, not runtime
    fn run_gc(&self) {}

    /// Source builds always install preemption. System-framework builds remain
    /// cooperative-only unless `interrupt-spi` is explicitly enabled.
    #[cfg(jsc_interrupt)]
    fn install_interrupt(&self, flag: Arc<AtomicBool>) -> bool {
        // The state lives until the runtime drops, so the context pointer
        // outlives the installed watchdog. Arming before any script enters
        // the group satisfies the SPI's "set before executing" contract.
        let state = Box::into_raw(Box::new(WatchdogState {
            group: self.raw,
            flag,
        }));
        self.watchdog_state.set(state);
        unsafe {
            jsc::JSContextGroupSetExecutionTimeLimit(
                self.raw,
                WATCHDOG_QUANTUM_SECONDS,
                Some(should_terminate),
                state as *mut std::os::raw::c_void,
            );
        }
        true
    }
}

impl Drop for JSCRuntime {
    fn drop(&mut self) {
        unsafe {
            // Uninstall the watchdog before freeing its callback context. No
            // JavaScript runs on this VM anymore, so no fire is in flight.
            #[cfg(jsc_interrupt)]
            {
                let state = self.watchdog_state.replace(std::ptr::null_mut());
                if !state.is_null() {
                    jsc::JSContextGroupClearExecutionTimeLimit(self.raw);
                    drop(Box::from_raw(state));
                }
            }
            jsc::JSContextGroupRelease(self.raw);
        }
    }
}

pub struct JavaScriptCore;

impl JSEngine for JavaScriptCore {
    type Value = JSCValue;
    type Context = JSCContext;
    type Runtime = JSCRuntime;

    fn name() -> &'static str {
        "JavaScriptCore"
    }

    fn version() -> String {
        #[cfg(jsc_source)]
        {
            option_env!("RONG_JSC_WEBKIT_REVISION")
                .map(|revision| format!("source:{revision}"))
                .unwrap_or_else(|| String::from("source"))
        }
        #[cfg(not(jsc_source))]
        {
            String::from("framework")
        }
    }
}
