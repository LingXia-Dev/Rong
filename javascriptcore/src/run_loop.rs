//! JavaScriptCore's timed work on Apple's system framework.
//!
//! JSC schedules its garbage collection timers, its incremental sweeper and
//! its other deferred work as CFRunLoop timers on the thread that created the
//! VM. Rong's workers never run a CFRunLoop, so without this those timers
//! never fire: collections come only from allocation, mostly as eden
//! collections, and memory that only a full collection frees (retired
//! contexts, say) is never reclaimed.
//!
//! Only public CoreFoundation API is used.

use std::os::raw::c_void;
use std::time::Duration;

type CFStringRef = *const c_void;
type CFRunLoopRef = *mut c_void;
type CFAbsoluteTime = f64;
type CFTimeInterval = f64;
type Boolean = u8;
type CFRunLoopRunResult = i32;

const RUN_FINISHED: CFRunLoopRunResult = 1;
const RUN_STOPPED: CFRunLoopRunResult = 2;
const RUN_HANDLED_SOURCE: CFRunLoopRunResult = 4;

/// Most run loop passes per call. One pass services at most one source, so a
/// few are needed when several timers are due together; the bound keeps a
/// timer that re-arms itself for "now" from holding the worker.
const MAX_PASSES: usize = 8;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    #[allow(non_upper_case_globals)]
    static kCFRunLoopDefaultMode: CFStringRef;

    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopRunInMode(
        mode: CFStringRef,
        seconds: CFTimeInterval,
        return_after_source_handled: Boolean,
    ) -> CFRunLoopRunResult;
    fn CFRunLoopGetNextTimerFireDate(run_loop: CFRunLoopRef, mode: CFStringRef) -> CFAbsoluteTime;
    fn CFAbsoluteTimeGetCurrent() -> CFAbsoluteTime;
}

/// Services what is due on this thread's run loop in the default mode,
/// without waiting, and returns the time until its next timer fires, or
/// `Duration::MAX` when none is armed.
///
/// Call it on the VM's thread with no JavaScript on the stack. JSC's timer
/// callbacks take the VM's API lock themselves.
pub(crate) fn run_due_timers() -> Duration {
    // SAFETY: plain CoreFoundation calls on the current thread's run loop.
    // `kCFRunLoopDefaultMode` is a constant CFString owned by CoreFoundation.
    unsafe {
        let run_loop = CFRunLoopGetCurrent();
        let mode = kCFRunLoopDefaultMode;
        for _ in 0..MAX_PASSES {
            match CFRunLoopRunInMode(mode, 0.0, 1) {
                RUN_HANDLED_SOURCE => continue,
                // Nothing is scheduled in this mode, or the loop was stopped.
                RUN_FINISHED | RUN_STOPPED => break,
                // Timed out: go again only while a timer is still due.
                _ => {
                    let next = CFRunLoopGetNextTimerFireDate(run_loop, mode);
                    if next == 0.0 || next > CFAbsoluteTimeGetCurrent() {
                        break;
                    }
                }
            }
        }

        let next = CFRunLoopGetNextTimerFireDate(run_loop, mode);
        if next == 0.0 {
            return Duration::MAX;
        }
        Duration::try_from_secs_f64((next - CFAbsoluteTimeGetCurrent()).max(0.0))
            .unwrap_or(Duration::MAX)
    }
}
