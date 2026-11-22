#![cfg(target_env = "ohos")]
//! Asks the device's JSVM whether a persistent reference holds a primitive
//! past the native call that created it. `ArkJSValue` relies on it.

use rong_arkjs_sys as sys;
use rong_core::JSContextImpl;
use rong_test::*;
use std::cell::Cell;
use std::ffi::CStr;
use std::ptr;
use std::rc::Rc;

const PROBE_TEXT: &str = "rong reference probe";
const PROBE_NUMBER: f64 = 4242.5;

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

#[test]
fn a_reference_holds_a_primitive_past_its_native_call() -> Result<(), String> {
    let info = vm_info();
    log::info!("[arkjs_reference_probe] JSVM {info}");

    let rt = RongJS::runtime();
    let ctx = rt.context();
    let env: sys::JSVM_Env = *ctx.as_ref().as_raw();

    let string_ref: Rc<Cell<Option<sys::JSVM_Ref>>> = Rc::default();
    let number_ref: Rc<Cell<Option<sys::JSVM_Ref>>> = Rc::default();
    let statuses: Rc<Cell<(u32, u32)>> = Rc::default();
    let (keep_string, keep_number, keep_statuses) =
        (string_ref.clone(), number_ref.clone(), statuses.clone());

    let probe = JSFunc::new(&ctx, move || unsafe {
        let mut text: sys::JSVM_Value = ptr::null_mut();
        sys::OH_JSVM_CreateStringUtf8(env, PROBE_TEXT.as_ptr().cast(), PROBE_TEXT.len(), &mut text);
        let mut number: sys::JSVM_Value = ptr::null_mut();
        sys::OH_JSVM_CreateDouble(env, PROBE_NUMBER, &mut number);

        let mut r: sys::JSVM_Ref = ptr::null_mut();
        let s1 = sys::OH_JSVM_CreateReference(env, text, 1, &mut r);
        if s1 == sys::JSVM_Status_JSVM_OK {
            keep_string.set(Some(r));
        }
        let mut r: sys::JSVM_Ref = ptr::null_mut();
        let s2 = sys::OH_JSVM_CreateReference(env, number, 1, &mut r);
        if s2 == sys::JSVM_Status_JSVM_OK {
            keep_number.set(Some(r));
        }
        keep_statuses.set((s1 as u32, s2 as u32));
    })
    .map_err(|e| format!("{e}"))?;
    ctx.global()
        .set("probe", probe)
        .map_err(|e| format!("{e}"))?;

    // Fill the handle slots the probe call used with other values.
    ctx.eval::<()>(Source::from_bytes(
        "probe(); for (let i = 0; i < 256; i++) { String(i) + 'x'; [i, i + 0.5]; }",
    ))
    .map_err(|e| format!("{e}"))?;

    let (s1, s2) = statuses.get();
    let mut report =
        format!("JSVM {info}; string reference status {s1}; number reference status {s2}");
    let mut ok = true;
    unsafe {
        match string_ref.get() {
            Some(r) => {
                let mut v: sys::JSVM_Value = ptr::null_mut();
                sys::OH_JSVM_GetReferenceValue(env, r, &mut v);
                let read = if v.is_null() {
                    None
                } else {
                    read_string(env, v)
                };
                ok &= read.as_deref() == Some(PROBE_TEXT);
                report.push_str(&format!("; string read back {read:?}"));
                sys::OH_JSVM_DeleteReference(env, r);
            }
            None => ok = false,
        }
        match number_ref.get() {
            Some(r) => {
                let mut v: sys::JSVM_Value = ptr::null_mut();
                sys::OH_JSVM_GetReferenceValue(env, r, &mut v);
                let mut n = f64::NAN;
                if !v.is_null() {
                    sys::OH_JSVM_GetValueDouble(env, v, &mut n);
                }
                ok &= n == PROBE_NUMBER;
                report.push_str(&format!("; number read back {n}"));
                sys::OH_JSVM_DeleteReference(env, r);
            }
            None => ok = false,
        }
    }
    log::info!("[arkjs_reference_probe] {report}");
    rong_arkjs::hilog_probe("[arkjs_reference_probe] backend HiLog reporter reached");
    if ok { Ok(()) } else { Err(report) }
}
