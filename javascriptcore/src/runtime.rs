use crate::{JSCContext, JSCValue, jsc};
#[cfg(jsc_interrupt)]
use rong_core::InterruptSignal;
use rong_core::{JSEngine, JSRuntimeImpl};
#[cfg(any(jsc_interrupt, jsc_source))]
use std::cell::Cell;

/// How often JSC's watchdog consults the callback while JavaScript runs.
#[cfg(jsc_interrupt)]
const WATCHDOG_QUANTUM_SECONDS: f64 = 0.1;

/// Callback context for the execution-time-limit watchdog. Heap-allocated per
/// runtime and freed on drop, after the limit is cleared.
#[cfg(jsc_interrupt)]
struct WatchdogState {
    group: *const jsc::OpaqueJSContextGroup,
    signal: InterruptSignal,
}

#[cfg(jsc_source)]
unsafe fn read_stat_u64(
    ctx: jsc::JSContextRef,
    obj: jsc::JSObjectRef,
    name: &std::ffi::CStr,
) -> Option<u64> {
    unsafe {
        let key = jsc::JSStringCreateWithUTF8CString(name.as_ptr());
        let mut exception: jsc::JSValueRef = std::ptr::null();
        let val = jsc::JSObjectGetProperty(ctx, obj, key, &mut exception);
        jsc::JSStringRelease(key);
        if !exception.is_null() || val.is_null() || !jsc::JSValueIsNumber(ctx, val) {
            return None;
        }
        let num = jsc::JSValueToNumber(ctx, val, &mut exception);
        if !exception.is_null() || !num.is_finite() || num < 0.0 {
            return None;
        }
        Some(num as u64)
    }
}

pub struct JSCRuntime {
    raw: *const jsc::OpaqueJSContextGroup,
    /// The installed watchdog's callback context (`*mut WatchdogState`), or
    /// null when no interrupt flag was installed.
    #[cfg(jsc_interrupt)]
    watchdog_state: Cell<*mut WatchdogState>,
    /// A context created on the first `memory_usage` call and kept until the
    /// runtime drops: JSC reports statistics through a context, and any context
    /// of the group reads the same VM-wide heap.
    #[cfg(jsc_source)]
    stats_context: Cell<*mut jsc::OpaqueJSContext>,
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
    if state.signal.should_abort() {
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
            #[cfg(jsc_source)]
            stats_context: Cell::new(std::ptr::null_mut()),
        }
    }

    fn to_raw(&self) -> Self::RawRuntime {
        self.raw
    }

    // JavaScriptCore has no public synchronous collection. It collects on
    // its own schedule, from timers that `run_engine_timers` services on
    // Apple's system framework.
    fn run_gc(&self) {}

    /// Source/JSCOnly builds query `JSGetMemoryUsageStatistics` (C ABI from
    /// `JSBasePrivate.h`, compiled inside `libJavaScriptCore` itself so there is
    /// no C++ struct-offset sensitivity across platforms/toolchains). Apple's
    /// system `JavaScriptCore.framework` has no public heap-statistics API
    /// (keeping default iOS/macOS builds free of private SPI symbols) and keeps
    /// the default `None`. Gathers VM-wide statistics: sample, not per call.
    #[cfg(jsc_source)]
    fn memory_usage(&self) -> Option<rong_core::MemoryUsage> {
        if self.raw.is_null() {
            return None;
        }
        // Created on first use instead of tracking the group's live contexts:
        // that bookkeeping would cost every context retain and release, a hot
        // path, to serve a rare sampling call.
        let ctx = match self.stats_context.get() {
            cached if !cached.is_null() => cached,
            _ => {
                let created =
                    unsafe { jsc::JSGlobalContextCreateInGroup(self.raw, std::ptr::null_mut()) };
                if created.is_null() {
                    return None;
                }
                self.stats_context.set(created);
                created
            }
        };

        unsafe {
            let stats_obj = jsc::JSGetMemoryUsageStatistics(ctx);
            if stats_obj.is_null() {
                return None;
            }
            let used_bytes = read_stat_u64(ctx, stats_obj, c"heapSize")?;
            let mut usage = rong_core::MemoryUsage::new(used_bytes);
            if let Some(cap) = read_stat_u64(ctx, stats_obj, c"heapCapacity") {
                usage = usage.with_allocated_bytes(cap);
            }
            // Already counted in heapSize; reported for breakdown only.
            if let Some(extra) = read_stat_u64(ctx, stats_obj, c"extraMemorySize") {
                usage = usage.with_external_bytes(extra);
            }
            if let Some(objs) = read_stat_u64(ctx, stats_obj, c"objectCount") {
                usage = usage.with_object_count(objs);
            }
            Some(usage)
        }
    }

    /// Apple's system framework runs JSC's collection timers, sweeper and
    /// deferred work on the VM thread's CFRunLoop. Always `Some`: timers get
    /// armed after the first call, so the caller must keep calling.
    ///
    /// Source/JSCOnly builds keep the default `None`: WebKit's JSCOnly port
    /// runs these timers on WTF's run loop, which the C API cannot drive, so
    /// retired contexts there wait for allocation-driven collections.
    #[cfg(all(target_vendor = "apple", not(jsc_source)))]
    fn run_engine_timers(&self) -> Option<std::time::Duration> {
        Some(crate::run_loop::run_due_timers())
    }

    /// Source builds always install preemption. System-framework builds remain
    /// cooperative-only unless `interrupt-spi` is explicitly enabled.
    #[cfg(jsc_interrupt)]
    fn install_interrupt(&self, signal: InterruptSignal) -> bool {
        // The state lives until the runtime drops, so the context pointer
        // outlives the installed watchdog. Arming before any script enters
        // the group satisfies the SPI's "set before executing" contract.
        let state = Box::into_raw(Box::new(WatchdogState {
            group: self.raw,
            signal,
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
            #[cfg(jsc_source)]
            {
                let stats_ctx = self.stats_context.replace(std::ptr::null_mut());
                if !stats_ctx.is_null() {
                    jsc::JSGlobalContextRelease(stats_ctx);
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
