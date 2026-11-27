//! Cranelift baseline JIT compiler for QuickJS-NG.
//!
//! This crate is internal to `rong_quickjs`. Public consumers should enable the
//! `quickjs-jit` feature on `rong` and use `rong::quickjs::jit`.

mod compiler;
mod decoder;

use rong_quickjs_sys as sys;
use std::ffi::c_void;

/// Configuration for the QuickJS baseline JIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JitConfig {
    /// Number of calls before a bytecode function is compiled to native code.
    ///
    /// A threshold of `1` compiles on the first call; the default is `16`.
    pub hot_threshold: u32,
    /// Maximum bytecode length (in bytes) eligible for JIT compilation.
    ///
    /// Larger functions stay in the QuickJS interpreter to bound compile time.
    pub max_bytecode_bytes: usize,
}

impl Default for JitConfig {
    fn default() -> Self {
        Self {
            hot_threshold: 16,
            max_bytecode_bytes: 16 * 1024,
        }
    }
}

/// Runtime statistics for a JIT-enabled QuickJS runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JitStats {
    /// Whether the JIT backend is supported on this CPU/OS target.
    pub supported: bool,
    /// Number of JS functions successfully compiled to native code.
    pub compiled_functions: u64,
    /// Number of JS functions skipped/rejected (run in QuickJS interpreter).
    pub rejected_functions: u64,
    /// Number of invocations executed in JIT-compiled native code.
    pub jit_calls: u64,
    /// Number of invocations that fell back to the QuickJS interpreter.
    pub interpreter_fallbacks: u64,
}

/// Returns `true` if the Cranelift JIT backend is supported on this target.
pub const fn is_supported() -> bool {
    cfg!(all(
        any(target_arch = "x86_64", target_arch = "aarch64"),
        not(target_os = "ios")
    ))
}

#[cfg(all(
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(target_os = "ios")
))]
mod engine {
    use super::*;
    use crate::compiler::{CompiledFn, CraneliftBackend};
    use crate::decoder::decode_bytecode;

    /// Sentinel stored in `JSFunctionBytecode::jit_data` for functions that
    /// cannot be compiled so we do not retry compilation on subsequent calls.
    const JIT_REJECTED: *mut c_void = std::ptr::dangling_mut::<c_void>();

    struct CompiledEntry {
        func: CompiledFn,
        realm: *mut sys::JSContext,
    }

    pub(crate) struct JitEngine {
        config: JitConfig,
        backend: CraneliftBackend,
        stats: JitStats,
        enabled: bool,
    }

    impl JitEngine {
        fn new(config: JitConfig) -> Result<Self, String> {
            let val_size = unsafe { sys::QJS_JitValueSize() };
            if val_size != 16 {
                return Err(format!(
                    "unsupported QuickJS JSValue layout (size={val_size}, expected 16)"
                ));
            }
            let backend = CraneliftBackend::new()?;
            Ok(Self {
                config,
                backend,
                stats: JitStats {
                    supported: true,
                    ..JitStats::default()
                },
                enabled: true,
            })
        }
    }

    unsafe extern "C" fn jit_call_hook(
        caller_ctx: *mut sys::JSContext,
        opaque: *mut c_void,
        bc: *mut c_void,
        func_obj: *const sys::JSValue,
        this_obj: *const sys::JSValue,
        argc: std::ffi::c_int,
        argv: *const sys::JSValue,
        _flags: std::ffi::c_int,
        ret_out: *mut sys::JSValue,
    ) -> std::ffi::c_int {
        if opaque.is_null() {
            return 0;
        }
        let engine = unsafe { &mut *(opaque as *mut JitEngine) };
        if !engine.enabled {
            engine.stats.interpreter_fallbacks += 1;
            return 0;
        }

        let data = unsafe { sys::QJS_JitGetBytecodeData(bc) };
        if data == JIT_REJECTED {
            engine.stats.interpreter_fallbacks += 1;
            return 0;
        }

        let entry_ptr = if !data.is_null() {
            data as *const CompiledEntry
        } else {
            let count = unsafe { sys::QJS_JitIncCallCount(bc) };
            if count < engine.config.hot_threshold.max(1) {
                engine.stats.interpreter_fallbacks += 1;
                return 0;
            }

            let mut info = std::mem::MaybeUninit::<sys::QJSJitBytecodeInfo>::zeroed();
            unsafe { sys::QJS_JitGetBytecodeInfo(bc, info.as_mut_ptr()) };
            let info = unsafe { info.assume_init() };

            let decoded = match decode_bytecode(&info, engine.config.max_bytecode_bytes) {
                Ok(d) => d,
                Err(_) => {
                    unsafe { sys::QJS_JitSetBytecodeData(bc, JIT_REJECTED) };
                    engine.stats.rejected_functions += 1;
                    engine.stats.interpreter_fallbacks += 1;
                    return 0;
                }
            };

            match engine.backend.compile(&decoded, &info) {
                Ok(func) => {
                    let entry = Box::into_raw(Box::new(CompiledEntry {
                        func,
                        realm: info.realm,
                    }));
                    unsafe { sys::QJS_JitSetBytecodeData(bc, entry as *mut c_void) };
                    engine.stats.compiled_functions += 1;
                    entry as *const CompiledEntry
                }
                Err(_) => {
                    unsafe { sys::QJS_JitSetBytecodeData(bc, JIT_REJECTED) };
                    engine.stats.rejected_functions += 1;
                    engine.stats.interpreter_fallbacks += 1;
                    return 0;
                }
            }
        };

        let entry = unsafe { &*entry_ptr };
        let exec_ctx = if entry.realm.is_null() {
            caller_ctx
        } else {
            entry.realm
        };
        engine.stats.jit_calls += 1;
        let rc = unsafe { (entry.func)(exec_ctx, func_obj, this_obj, argc, argv, ret_out) };
        if rc == 0 {
            1 // handled by JIT
        } else {
            engine.stats.interpreter_fallbacks += 1;
            0 // bailed out -> fall back to interpreter
        }
    }

    unsafe extern "C" fn jit_free_bytecode_hook(
        _opaque: *mut c_void,
        _bc: *mut c_void,
        jit_data: *mut c_void,
    ) {
        if !jit_data.is_null() && jit_data != JIT_REJECTED {
            unsafe {
                drop(Box::from_raw(jit_data as *mut CompiledEntry));
            }
        }
    }

    unsafe extern "C" fn jit_free_runtime_hook(opaque: *mut c_void) {
        if !opaque.is_null() {
            unsafe {
                drop(Box::from_raw(opaque as *mut JitEngine));
            }
        }
    }

    /// Installs and enables the Cranelift baseline JIT on `rt`.
    ///
    /// If the JIT was already installed on `rt`, updates its config and
    /// re-enables it.
    ///
    /// # Safety
    /// `rt` must be a valid, live `*mut JSRuntime` pointer.
    pub unsafe fn init_raw(rt: *mut sys::JSRuntime, config: JitConfig) -> Result<bool, String> {
        let existing = unsafe { sys::QJS_GetJitOpaque(rt) };
        if !existing.is_null() {
            let engine = unsafe { &mut *(existing as *mut JitEngine) };
            engine.config = config;
            engine.enabled = true;
            return Ok(true);
        }

        let engine = Box::new(JitEngine::new(config)?);
        let opaque = Box::into_raw(engine) as *mut c_void;
        unsafe {
            sys::QJS_SetJitHooks(
                rt,
                opaque,
                Some(jit_call_hook),
                Some(jit_free_bytecode_hook),
                Some(jit_free_runtime_hook),
            );
        }
        Ok(true)
    }

    /// Disables JIT execution on `rt` (existing compiled functions are retained
    /// until the runtime is freed, and all calls fall back to the interpreter).
    ///
    /// # Safety
    /// `rt` must be a valid, live `*mut JSRuntime` pointer.
    pub unsafe fn set_enabled_raw(rt: *mut sys::JSRuntime, enabled: bool) -> bool {
        let existing = unsafe { sys::QJS_GetJitOpaque(rt) };
        if existing.is_null() {
            return false;
        }
        let engine = unsafe { &mut *(existing as *mut JitEngine) };
        engine.enabled = enabled;
        true
    }

    /// Returns a snapshot of JIT statistics for `rt`, if initialized.
    ///
    /// # Safety
    /// `rt` must be a valid, live `*mut JSRuntime` pointer.
    pub unsafe fn stats_raw(rt: *mut sys::JSRuntime) -> Option<JitStats> {
        let existing = unsafe { sys::QJS_GetJitOpaque(rt) };
        if existing.is_null() {
            return None;
        }
        let engine = unsafe { &*(existing as *const JitEngine) };
        Some(engine.stats)
    }
}

#[cfg(not(all(
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(target_os = "ios")
)))]
mod engine {
    use super::*;

    pub unsafe fn init_raw(_rt: *mut sys::JSRuntime, _config: JitConfig) -> Result<bool, String> {
        Ok(false)
    }

    pub unsafe fn set_enabled_raw(_rt: *mut sys::JSRuntime, _enabled: bool) -> bool {
        false
    }

    pub unsafe fn stats_raw(_rt: *mut sys::JSRuntime) -> Option<JitStats> {
        Some(JitStats {
            supported: false,
            ..JitStats::default()
        })
    }
}

pub use engine::{init_raw, set_enabled_raw, stats_raw};
