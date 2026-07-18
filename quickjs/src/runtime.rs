use crate::{QJSContext, QJSValue, qjs};
use rong_core::{JSEngine, JSRuntimeImpl};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

thread_local! {
    static RUNTIME_REGISTRY: RefCell<HashMap<usize, Weak<QJSRuntimeInner>>> =
        RefCell::new(HashMap::new());
}

pub(crate) struct QJSRuntimeInner {
    pub(crate) rt: *mut qjs::JSRuntime,
    /// Keeps the flag the installed interrupt handler polls alive for the
    /// whole runtime lifetime (its address is the handler's opaque pointer).
    interrupt_flag: RefCell<Option<Arc<AtomicBool>>>,
}

/// QuickJS polls this handler inside the interpreter (and regex) loops; a
/// nonzero return aborts execution with an uncatchable `InternalError:
/// interrupted`, which breaks `while (true) {}`.
unsafe extern "C" fn interrupt_handler(
    _rt: *mut qjs::JSRuntime,
    opaque: *mut std::os::raw::c_void,
) -> std::os::raw::c_int {
    let flag = unsafe { &*(opaque as *const AtomicBool) };
    flag.load(Ordering::Relaxed) as std::os::raw::c_int
}

impl Drop for QJSRuntimeInner {
    fn drop(&mut self) {
        // Remove before freeing to avoid upgrading a dead runtime.
        let key = self.rt as usize;
        RUNTIME_REGISTRY.with(|m| {
            m.borrow_mut().remove(&key);
        });

        unsafe {
            // Best-effort cleanup: drain jobs and run GC before freeing the runtime.
            // Keep the interrupt handler installed while draining: an active
            // interruption request must still be able to abort a non-yielding
            // pending job during teardown.
            let mut ctx = std::ptr::null_mut();
            let mut iterations: usize = 0;
            const MAX_JOB_DRAIN: usize = 10_000;
            while qjs::JS_IsJobPending(self.rt) && iterations < MAX_JOB_DRAIN {
                let rc = qjs::JS_ExecutePendingJob(self.rt, &mut ctx);
                if rc < 0 {
                    break;
                }
                iterations += 1;
            }
            qjs::JS_RunGC(self.rt);
            qjs::JS_RunGC(self.rt);

            // The Arc remains alive in `interrupt_flag` through the drain, so
            // its opaque pointer is valid until the handler is uninstalled.
            if self.interrupt_flag.get_mut().is_some() {
                qjs::JS_SetInterruptHandler(self.rt, None, std::ptr::null_mut());
            }
            qjs::JS_FreeRuntime(self.rt);
        }
    }
}

#[derive(Clone)]
pub struct QJSRuntime {
    pub(crate) inner: Rc<QJSRuntimeInner>,
}

impl JSRuntimeImpl for QJSRuntime {
    type RawRuntime = *mut qjs::JSRuntime;
    type Context = QJSContext;

    // new QuickJS JS Runtime
    fn new() -> Self {
        let rt = unsafe { qjs::JS_NewRuntime() };
        let inner = Rc::new(QJSRuntimeInner {
            rt,
            interrupt_flag: RefCell::new(None),
        });
        let key = rt as usize;
        RUNTIME_REGISTRY.with(|m| {
            m.borrow_mut().insert(key, Rc::downgrade(&inner));
        });

        //0x200: dump every object free
        //0x4000: dump leaked objects and strings in JS_FreeRuntime
        //more flags, pls refer to quickjs.c
        #[cfg(debug_assertions)]
        if let Ok(flags) = std::env::var("DUMPFLAGS") {
            let flags = if flags.starts_with("0x") {
                u64::from_str_radix(flags.trim_start_matches("0x"), 16).unwrap_or(0x4000 | 0x200)
            } else {
                0x4000 | 0x200
            };
            println!("Dump flags: 0x{:x}", flags);
            unsafe {
                qjs::JS_SetDumpFlags(rt, flags);
            }
        }
        Self { inner }
    }

    fn to_raw(&self) -> Self::RawRuntime {
        self.inner.rt
    }

    fn run_pending_jobs(&self) -> i32 {
        unsafe {
            let mut ctx = std::ptr::null_mut();
            while qjs::JS_IsJobPending(self.inner.rt) {
                qjs::JS_ExecutePendingJob(self.inner.rt, &mut ctx);
            }
        }
        0
    }

    fn run_gc(&self) {
        unsafe {
            #[cfg(debug_assertions)]
            println!("run gc");
            qjs::JS_RunGC(self.inner.rt);
        }
    }

    fn install_interrupt(&self, flag: Arc<AtomicBool>) -> bool {
        let opaque = Arc::as_ptr(&flag) as *mut std::os::raw::c_void;
        // The Arc lives in the inner runtime until drop, so the opaque
        // pointer outlives the installed handler.
        *self.inner.interrupt_flag.borrow_mut() = Some(flag);
        unsafe {
            qjs::JS_SetInterruptHandler(self.inner.rt, Some(interrupt_handler), opaque);
        }
        true
    }
}

pub(crate) fn runtime_guard_from_ctx(ctx: *mut qjs::JSContext) -> Option<Rc<QJSRuntimeInner>> {
    if ctx.is_null() {
        return None;
    }
    // SAFETY: `ctx` is expected to be a valid QuickJS context pointer.
    let rt = unsafe { qjs::JS_GetRuntime(ctx) };
    let key = rt as usize;
    RUNTIME_REGISTRY.with(|m| m.borrow().get(&key).and_then(|w| w.upgrade()))
}

pub struct QuickJS;

impl JSEngine for QuickJS {
    type Value = QJSValue;
    type Context = QJSContext;
    type Runtime = QJSRuntime;

    fn name() -> &'static str {
        "QuickJS-NG"
    }

    fn version() -> String {
        unsafe {
            let c_str = qjs::JS_GetVersion();
            std::ffi::CStr::from_ptr(c_str)
                .to_str()
                .map(|s| s.to_string())
                .unwrap()
        }
    }
}
