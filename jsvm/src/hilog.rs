//! Reports through HiLog, the platform log: a device does not collect stderr.

use std::ffi::{CString, c_char, c_int, c_uint};

const LOG_APP: c_int = 0;
const LOG_WARN: c_int = 5;
const LOG_ERROR: c_int = 6;
const LOG_FATAL: c_int = 7;
const DOMAIN: c_uint = 0x524E;
const TAG: &std::ffi::CStr = c"Rong";

#[link(name = "hilog_ndk.z")]
unsafe extern "C" {
    fn OH_LOG_Print(
        log_type: c_int,
        level: c_int,
        domain: c_uint,
        tag: *const c_char,
        fmt: *const c_char,
        ...
    ) -> c_int;
}

fn print(level: c_int, message: &str) {
    let message =
        CString::new(message.replace('\0', "\u{fffd}")).expect("interior NULs were replaced");
    // `%{public}s`: HiLog redacts plain `%s` arguments in release builds.
    unsafe {
        OH_LOG_Print(
            LOG_APP,
            level,
            DOMAIN,
            TAG.as_ptr(),
            c"%{public}s".as_ptr(),
            message.as_ptr(),
        );
    }
}

pub(crate) fn warn(message: &str) {
    print(LOG_WARN, message);
}

pub(crate) fn error(message: &str) {
    print(LOG_ERROR, message);
}

pub(crate) fn fatal(message: &str) {
    print(LOG_FATAL, message);
}
