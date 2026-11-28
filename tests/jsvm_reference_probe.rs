#![cfg(target_env = "ohos")]
//! Asks the device's JSVM what `JSVMValue` relies on: that a persistent
//! reference holds every kind of primitive past the native call that made it,
//! and that closing a handle scope leaves a pending exception readable.

use rong_core::JSContextImpl;
use rong_jsvm_sys as sys;
use rong_test::*;
use std::cell::RefCell;
use std::ffi::CStr;
use std::ptr;
use std::rc::Rc;

const PROBE_TEXT: &str = "rong reference probe";
const PROBE_NUMBER: f64 = 4242.5;
const PROBE_BIGINT: i64 = -9_007_199_254_740_993;
const THROWN_TEXT: &str = "thrown inside a closed scope";

fn vm_info() -> String {
    unsafe {
        let mut info: sys::JSVM_VMInfo = std::mem::zeroed();
        if sys::OH_JSVM_GetVMInfo(&mut info) != sys::JSVM_Status_JSVM_OK {
            return "unavailable".into();
        }
        let text = |p: *const std::os::raw::c_char| {
            if p.is_null() {
                String::new()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        };
        format!(
            "apiVersion={} engine={} version={}",
            info.apiVersion,
            text(info.engine),
            text(info.version)
        )
    }
}

fn read_string(env: sys::JSVM_Env, value: sys::JSVM_Value) -> Option<String> {
    unsafe {
        let mut len = 0usize;
        if sys::OH_JSVM_GetValueStringUtf8(env, value, ptr::null_mut(), 0, &mut len)
            != sys::JSVM_Status_JSVM_OK
        {
            return None;
        }
        let mut buf = vec![0u8; len + 1];
        if sys::OH_JSVM_GetValueStringUtf8(env, value, buf.as_mut_ptr().cast(), len + 1, &mut len)
            != sys::JSVM_Status_JSVM_OK
        {
            return None;
        }
        buf.truncate(len);
        String::from_utf8(buf).ok()
    }
}

fn type_of(env: sys::JSVM_Env, value: sys::JSVM_Value) -> sys::JSVM_ValueType {
    let mut t = sys::JSVM_ValueType_JSVM_UNDEFINED;
    unsafe { sys::OH_JSVM_Typeof(env, value, &mut t) };
    t
}

fn create_string(env: sys::JSVM_Env, text: &str) -> sys::JSVM_Value {
    let mut v = ptr::null_mut();
    unsafe { sys::OH_JSVM_CreateStringUtf8(env, text.as_ptr().cast(), text.len(), &mut v) };
    v
}

/// Per kind: the CreateReference status and the reference, if one was made.
type Made = Rc<RefCell<Vec<(u32, Option<sys::JSVM_Ref>)>>>;

/// One primitive kind: how to make it, and how to tell it survived.
struct Kind {
    name: &'static str,
    make: fn(sys::JSVM_Env) -> sys::JSVM_Value,
    check: fn(sys::JSVM_Env, sys::JSVM_Value) -> String,
}

fn kinds() -> Vec<Kind> {
    vec![
        Kind {
            name: "string",
            make: |env| create_string(env, PROBE_TEXT),
            check: |env, v| format!("{:?}", read_string(env, v)),
        },
        Kind {
            name: "number",
            make: |env| {
                let mut v = ptr::null_mut();
                unsafe { sys::OH_JSVM_CreateDouble(env, PROBE_NUMBER, &mut v) };
                v
            },
            check: |env, v| {
                let mut n = f64::NAN;
                unsafe { sys::OH_JSVM_GetValueDouble(env, v, &mut n) };
                format!("{n}")
            },
        },
        Kind {
            name: "boolean",
            make: |env| {
                let mut v = ptr::null_mut();
                unsafe { sys::OH_JSVM_GetBoolean(env, true, &mut v) };
                v
            },
            check: |env, v| {
                let mut b = false;
                let s = unsafe { sys::OH_JSVM_GetValueBool(env, v, &mut b) };
                format!("{b} (status {s})")
            },
        },
        Kind {
            name: "undefined",
            make: |env| {
                let mut v = ptr::null_mut();
                unsafe { sys::OH_JSVM_GetUndefined(env, &mut v) };
                v
            },
            check: |env, v| format!("type {}", type_of(env, v)),
        },
        Kind {
            name: "null",
            make: |env| {
                let mut v = ptr::null_mut();
                unsafe { sys::OH_JSVM_GetNull(env, &mut v) };
                v
            },
            check: |env, v| format!("type {}", type_of(env, v)),
        },
        Kind {
            name: "symbol",
            make: |env| {
                let mut v = ptr::null_mut();
                let desc = create_string(env, PROBE_TEXT);
                unsafe { sys::OH_JSVM_CreateSymbol(env, desc, &mut v) };
                v
            },
            check: |env, v| format!("type {}", type_of(env, v)),
        },
        Kind {
            name: "bigint",
            make: |env| {
                let mut v = ptr::null_mut();
                unsafe { sys::OH_JSVM_CreateBigintInt64(env, PROBE_BIGINT, &mut v) };
                v
            },
            check: |env, v| {
                let (mut n, mut lossless) = (0i64, false);
                unsafe { sys::OH_JSVM_GetValueBigintInt64(env, v, &mut n, &mut lossless) };
                format!("{n}")
            },
        },
    ]
}

fn expected(name: &str) -> String {
    match name {
        "string" => format!("{:?}", Some(PROBE_TEXT)),
        "number" => format!("{PROBE_NUMBER}"),
        "boolean" => format!("true (status {})", sys::JSVM_Status_JSVM_OK),
        "undefined" => format!("type {}", sys::JSVM_ValueType_JSVM_UNDEFINED),
        "null" => format!("type {}", sys::JSVM_ValueType_JSVM_NULL),
        "symbol" => format!("type {}", sys::JSVM_ValueType_JSVM_SYMBOL),
        "bigint" => format!("{PROBE_BIGINT}"),
        _ => unreachable!(),
    }
}

#[test]
fn a_reference_holds_every_primitive_past_its_native_call() -> Result<(), String> {
    let info = vm_info();
    log::info!("[jsvm_reference_probe] JSVM {info}");

    let rt = RongJS::runtime();
    let ctx = rt.context();
    let env: sys::JSVM_Env = *ctx.as_ref().as_raw();

    let made: Made = Rc::default();
    let keep = made.clone();
    let probe = JSFunc::new(&ctx, move || {
        for kind in kinds() {
            let value = (kind.make)(env);
            let mut r: sys::JSVM_Ref = ptr::null_mut();
            let status = unsafe { sys::OH_JSVM_CreateReference(env, value, 1, &mut r) };
            let ok = status == sys::JSVM_Status_JSVM_OK && !r.is_null();
            keep.borrow_mut().push((status as u32, ok.then_some(r)));
        }
    })
    .map_err(|e| format!("{e}"))?;
    ctx.global()
        .set("probe", probe)
        .map_err(|e| format!("{e}"))?;

    // Fill the handle slots the probe call used with other values.
    ctx.eval::<()>(Source::from_bytes(
        "probe(); for (let i = 0; i < 256; i++) { String(i) + 'x'; [i, i + 0.5]; Symbol(i); BigInt(i); }",
    ))
    .map_err(|e| format!("{e}"))?;

    let mut report = format!("JSVM {info}");
    let mut ok = true;
    let made = made.borrow();
    for (kind, (status, r)) in kinds().iter().zip(made.iter()) {
        let read = match r {
            Some(r) => unsafe {
                let mut v: sys::JSVM_Value = ptr::null_mut();
                let s = sys::OH_JSVM_GetReferenceValue(env, *r, &mut v);
                let read = if s == sys::JSVM_Status_JSVM_OK && !v.is_null() {
                    (kind.check)(env, v)
                } else {
                    format!("unreadable (status {s})")
                };
                sys::OH_JSVM_DeleteReference(env, *r);
                read
            },
            None => "no reference".to_string(),
        };
        ok &= read == expected(kind.name);
        report.push_str(&format!(
            "; {}: reference status {status}, read back {read}",
            kind.name
        ));
    }
    if made.len() != kinds().len() {
        ok = false;
        report.push_str("; probe did not run");
    }
    log::info!("[jsvm_reference_probe] {report}");
    rong_jsvm::hilog_probe("[jsvm_reference_probe] backend HiLog reporter reached");
    if ok { Ok(()) } else { Err(report) }
}

/// The backend closes an operation's scope while a thrown exception may still
/// be pending. JSVM must allow the close, and the exception must still be
/// there to read afterwards.
#[test]
fn closing_a_scope_keeps_its_pending_exception() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let env: sys::JSVM_Env = *ctx.as_ref().as_raw();
    unsafe {
        let mut scope: sys::JSVM_HandleScope = ptr::null_mut();
        let open = sys::OH_JSVM_OpenHandleScope(env, &mut scope);
        if open != sys::JSVM_Status_JSVM_OK || scope.is_null() {
            return Err(format!("open {open} gave no scope to close"));
        }
        let message = create_string(env, THROWN_TEXT);
        let mut error = ptr::null_mut();
        sys::OH_JSVM_CreateError(env, ptr::null_mut(), message, &mut error);
        let thrown = sys::OH_JSVM_Throw(env, error);
        let close = sys::OH_JSVM_CloseHandleScope(env, scope);

        let mut pending = false;
        sys::OH_JSVM_IsExceptionPending(env, &mut pending);
        let mut exception: sys::JSVM_Value = ptr::null_mut();
        let take = sys::OH_JSVM_GetAndClearLastException(env, &mut exception);
        let mut text = None;
        if !exception.is_null() {
            let mut msg = ptr::null_mut();
            if sys::OH_JSVM_GetNamedProperty(env, exception, c"message".as_ptr(), &mut msg)
                == sys::JSVM_Status_JSVM_OK
            {
                text = read_string(env, msg);
            }
        }
        let report = format!(
            "open {open}, throw {thrown}, close {close}, pending after close {pending}, \
             take {take}, message {text:?}"
        );
        log::info!("[jsvm_reference_probe] {report}");
        let ok =
            close == sys::JSVM_Status_JSVM_OK && pending && text.as_deref() == Some(THROWN_TEXT);
        if ok { Ok(()) } else { Err(report) }
    }
}
