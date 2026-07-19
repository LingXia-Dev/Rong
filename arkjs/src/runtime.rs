#![allow(clippy::missing_const_for_thread_local)]

use crate::{ArkJSContext, ArkJSValue, arkjs};
use rong_core::{JSEngine, JSRuntimeImpl};
use std::cell::RefCell;
use std::ffi::CStr;
use std::ptr;
use std::rc::Rc;
use std::sync::Once;

/// Stored info about an unhandled promise rejection.
/// When `Promise.reject(v)` is created without a handler, ArkJS fires the
/// reject handler with the promise and its rejection value.  We capture these
/// so `register_promise_handlers` can deliver them directly (bypassing the
/// broken microtask system for already-settled promises).
struct UnhandledRejection {
    env: arkjs::JSVM_Env,
    promise_ref: arkjs::JSVM_Ref,
    value_ref: arkjs::JSVM_Ref,
}

thread_local! {
    static UNHANDLED_REJECTIONS: RefCell<Vec<UnhandledRejection>> = const { RefCell::new(Vec::new()) };
}

/// Store a rejection.  Called from the promise_reject_handler.
fn store_unhandled_rejection(
    env: arkjs::JSVM_Env,
    promise: arkjs::JSVM_Value,
    value: arkjs::JSVM_Value,
) {
    unsafe {
        let mut promise_ref: arkjs::JSVM_Ref = ptr::null_mut();
        let mut value_ref: arkjs::JSVM_Ref = ptr::null_mut();
        // Create strong references so the values survive beyond the callback
        let s1 = arkjs::OH_JSVM_CreateReference(env, promise, 1, &mut promise_ref);
        let s2 = arkjs::OH_JSVM_CreateReference(env, value, 1, &mut value_ref);
        if s1 == arkjs::JSVM_Status_JSVM_OK && s2 == arkjs::JSVM_Status_JSVM_OK {
            UNHANDLED_REJECTIONS.with(|v| {
                v.borrow_mut().push(UnhandledRejection {
                    env,
                    promise_ref,
                    value_ref,
                });
            });
        } else {
            if !promise_ref.is_null() {
                arkjs::OH_JSVM_DeleteReference(env, promise_ref);
            }
            if !value_ref.is_null() {
                arkjs::OH_JSVM_DeleteReference(env, value_ref);
            }
        }
    }
}

fn remove_unhandled_rejection_entry(
    env: arkjs::JSVM_Env,
    promise: arkjs::JSVM_Value,
) -> Option<UnhandledRejection> {
    UNHANDLED_REJECTIONS.with(|v| {
        let mut rejections = v.borrow_mut();
        let mut found_idx = None;
        for (i, entry) in rejections.iter().enumerate() {
            if entry.env != env {
                continue;
            }
            unsafe {
                let mut stored_promise: arkjs::JSVM_Value = ptr::null_mut();
                let status =
                    arkjs::OH_JSVM_GetReferenceValue(env, entry.promise_ref, &mut stored_promise);
                if status == arkjs::JSVM_Status_JSVM_OK && !stored_promise.is_null() {
                    let mut is_equal = false;
                    let status =
                        arkjs::OH_JSVM_StrictEquals(env, promise, stored_promise, &mut is_equal);
                    if status == arkjs::JSVM_Status_JSVM_OK && is_equal {
                        found_idx = Some(i);
                        break;
                    }
                }
            }
        }

        found_idx.map(|idx| rejections.remove(idx))
    })
}

fn delete_unhandled_rejection_entry(
    env: arkjs::JSVM_Env,
    entry: UnhandledRejection,
) -> Option<arkjs::JSVM_Value> {
    unsafe {
        let mut value: arkjs::JSVM_Value = ptr::null_mut();
        let status = arkjs::OH_JSVM_GetReferenceValue(env, entry.value_ref, &mut value);
        arkjs::OH_JSVM_DeleteReference(env, entry.promise_ref);
        arkjs::OH_JSVM_DeleteReference(env, entry.value_ref);
        if status == arkjs::JSVM_Status_JSVM_OK && !value.is_null() {
            Some(value)
        } else {
            None
        }
    }
}

/// Try to find and remove a stored rejection for the given promise.
/// Returns the rejection value if found.
pub(crate) fn take_unhandled_rejection(
    env: arkjs::JSVM_Env,
    promise: arkjs::JSVM_Value,
) -> Option<arkjs::JSVM_Value> {
    remove_unhandled_rejection_entry(env, promise)
        .and_then(|entry| delete_unhandled_rejection_entry(env, entry))
}

fn discard_unhandled_rejection(env: arkjs::JSVM_Env, promise: arkjs::JSVM_Value) {
    if let Some(entry) = remove_unhandled_rejection_entry(env, promise) {
        let _ = delete_unhandled_rejection_entry(env, entry);
    }
}

pub(crate) fn clear_unhandled_rejections(env: arkjs::JSVM_Env) {
    UNHANDLED_REJECTIONS.with(|v| {
        let mut rejections = v.borrow_mut();
        let mut idx = 0;
        while idx < rejections.len() {
            if rejections[idx].env != env {
                idx += 1;
                continue;
            }

            let entry = rejections.remove(idx);
            unsafe {
                arkjs::OH_JSVM_DeleteReference(env, entry.promise_ref);
                arkjs::OH_JSVM_DeleteReference(env, entry.value_ref);
            }
        }
    });
}

/// Handler invoked by the VM on fatal errors (e.g. unreachable code).
unsafe extern "C" fn fatal_error_handler(
    location: *const std::os::raw::c_char,
    message: *const std::os::raw::c_char,
) {
    unsafe {
        let loc = if location.is_null() {
            "<unknown>"
        } else {
            CStr::from_ptr(location)
                .to_str()
                .unwrap_or("<invalid utf8>")
        };
        let msg = if message.is_null() {
            "<no message>"
        } else {
            CStr::from_ptr(message).to_str().unwrap_or("<invalid utf8>")
        };
        eprintln!("[ArkJS FATAL] {}: {}", loc, msg);
    }
}

/// Handler invoked by the VM when an out-of-memory condition occurs.
unsafe extern "C" fn oom_error_handler(
    location: *const std::os::raw::c_char,
    detail: *const std::os::raw::c_char,
    is_heap_oom: bool,
) {
    unsafe {
        let loc = if location.is_null() {
            "<unknown>"
        } else {
            CStr::from_ptr(location)
                .to_str()
                .unwrap_or("<invalid utf8>")
        };
        let det = if detail.is_null() {
            "<no detail>"
        } else {
            CStr::from_ptr(detail).to_str().unwrap_or("<invalid utf8>")
        };
        eprintln!("[ArkJS OOM] {}: {} (heap_oom={})", loc, det, is_heap_oom);
    }
}

/// Handler invoked when promise rejection bookkeeping changes.
///
/// We capture `REJECT_WITH_NO_HANDLER` values so `register_promise_handlers`
/// can deliver them directly, and we drop the cached entry when ArkJS later
/// reports `ADD_HANDLER_AFTER_REJECTED`.
unsafe extern "C" fn promise_reject_handler(
    env: arkjs::JSVM_Env,
    reject_event: arkjs::JSVM_PromiseRejectEvent,
    reject_info: arkjs::JSVM_Value,
) {
    if reject_info.is_null() {
        return;
    }

    unsafe {
        let mut promise: arkjs::JSVM_Value = ptr::null_mut();
        let mut value: arkjs::JSVM_Value = ptr::null_mut();
        arkjs::OH_JSVM_GetNamedProperty(env, reject_info, c"promise".as_ptr() as _, &mut promise);
        arkjs::OH_JSVM_GetNamedProperty(env, reject_info, c"value".as_ptr() as _, &mut value);
        if promise.is_null() {
            return;
        }

        match reject_event {
            arkjs::JSVM_PromiseRejectEvent_JSVM_PROMISE_REJECT_WITH_NO_HANDLER => {
                if !value.is_null() {
                    store_unhandled_rejection(env, promise, value);
                }
            }
            arkjs::JSVM_PromiseRejectEvent_JSVM_PROMISE_ADD_HANDLER_AFTER_REJECTED => {
                discard_unhandled_rejection(env, promise);
            }
            _ => {}
        }
    }
}

static JSVM_INIT: Once = Once::new();

fn ensure_jsvm_initialized() {
    JSVM_INIT.call_once(|| {
        let init_options = arkjs::JSVM_InitOptions {
            externalReferences: ptr::null(),
            argc: ptr::null_mut(),
            argv: ptr::null_mut(),
            removeFlags: false,
        };

        unsafe {
            let status = arkjs::OH_JSVM_Init(&init_options);
            if status != arkjs::JSVM_Status_JSVM_OK {
                panic!("Failed to initialize Ark JS VM: {:?}", status);
            }
        }
    });
}

pub(crate) struct ArkJSRuntimeInner {
    pub(crate) raw: arkjs::JSVM_VM,
    vm_scope: arkjs::JSVM_VMScope,
}

impl Drop for ArkJSRuntimeInner {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                if !self.vm_scope.is_null() {
                    arkjs::OH_JSVM_CloseVMScope(self.raw, self.vm_scope);
                }
                arkjs::OH_JSVM_DestroyVM(self.raw);
            }
        }
    }
}

#[derive(Clone)]
pub struct ArkJSRuntime {
    pub(crate) inner: Rc<ArkJSRuntimeInner>,
}

impl JSRuntimeImpl for ArkJSRuntime {
    type RawRuntime = arkjs::JSVM_VM;
    type Context = ArkJSContext;

    fn new() -> Self {
        ensure_jsvm_initialized();

        let mut vm: arkjs::JSVM_VM = ptr::null_mut();
        let mut vm_scope: arkjs::JSVM_VMScope = ptr::null_mut();

        unsafe {
            let create_options = arkjs::JSVM_CreateVMOptions {
                maxOldGenerationSize: 0,
                maxYoungGenerationSize: 0,
                initialOldGenerationSize: 0,
                initialYoungGenerationSize: 0,
                snapshotBlobData: ptr::null(),
                snapshotBlobSize: 0,
                isForSnapshotting: false,
            };

            let status = arkjs::OH_JSVM_CreateVM(&create_options, &mut vm);
            if status != arkjs::JSVM_Status_JSVM_OK {
                panic!("Failed to create Ark JS VM: {:?}", status);
            }

            let status = arkjs::OH_JSVM_OpenVMScope(vm, &mut vm_scope);
            if status != arkjs::JSVM_Status_JSVM_OK {
                arkjs::OH_JSVM_DestroyVM(vm);
                panic!("Failed to open VM scope: {:?}", status);
            }

            // Register error handlers for diagnostics
            arkjs::OH_JSVM_SetHandlerForFatalError(vm, Some(fatal_error_handler));
            arkjs::OH_JSVM_SetHandlerForOOMError(vm, Some(oom_error_handler));
            arkjs::OH_JSVM_SetHandlerForPromiseReject(vm, Some(promise_reject_handler));
        }

        Self {
            inner: Rc::new(ArkJSRuntimeInner { raw: vm, vm_scope }),
        }
    }

    fn to_raw(&self) -> Self::RawRuntime {
        self.inner.raw
    }

    fn run_pending_jobs(&self) -> i32 {
        unsafe {
            let _ = arkjs::OH_JSVM_PerformMicrotaskCheckpoint(self.inner.raw);
        }
        0
    }

    fn run_gc(&self) {
        // Harmony Ark JS doesn't expose direct GC control
    }

    // Engine preemption is intentionally NOT implemented: the JSVM-API surface
    // bound here (ark_runtime/jsvm.h, OpenHarmony SDK 20) exposes no
    // execution-terminate or interrupt-request primitive (no
    // OH_JSVM_TerminateExecution or equivalent). Keeping the default
    // `install_interrupt` (returns false) leaves only rong's cooperative
    // layer: new evaluations are rejected while interrupted, but a
    // non-yielding loop cannot be broken. Revisit when the SDK exposes a
    // terminate API.
}

pub struct HarmonyArkJS;

impl JSEngine for HarmonyArkJS {
    type Value = ArkJSValue;
    type Context = ArkJSContext;
    type Runtime = ArkJSRuntime;

    fn name() -> &'static str {
        "HarmonyArkJS"
    }

    fn version() -> String {
        String::from("ArkJS 1.0.0")
    }
}
