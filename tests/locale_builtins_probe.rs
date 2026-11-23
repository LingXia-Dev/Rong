//! Not asserted: records what the builtins that depend on ICU or the host
//! return on this engine, one line per item in a fixed format, so the logs of
//! different engines line up side by side.

use rong::JSEngine;
use rong_test::*;

const PROBE: &str = r#"
(() => {
  const out = [];
  const item = (name, f) => {
    let v;
    try {
      const r = f();
      v = typeof r === 'string' ? JSON.stringify(r) : String(r);
    } catch (e) {
      v = 'throws ' + (e && e.name ? e.name : typeof e) + ': ' + (e && e.message);
    }
    out.push(name + ' | ' + v);
  };
  item('Intl', () => typeof globalThis.Intl);
  for (const c of ['DateTimeFormat', 'NumberFormat', 'PluralRules', 'RelativeTimeFormat',
                   'ListFormat', 'Collator', 'Segmenter', 'DisplayNames', 'Locale']) {
    item('Intl.' + c, () => typeof Intl[c]);
  }
  item('Intl.supportedValuesOf', () => typeof Intl.supportedValuesOf);
  item('Intl.supportedValuesOf("timeZone").length', () => Intl.supportedValuesOf('timeZone').length);
  item('Intl.DateTimeFormat().resolvedOptions().locale', () => Intl.DateTimeFormat().resolvedOptions().locale);
  item('Intl.DateTimeFormat().resolvedOptions().timeZone', () => Intl.DateTimeFormat().resolvedOptions().timeZone);
  item('Intl.DateTimeFormat().resolvedOptions()', () => JSON.stringify(Intl.DateTimeFormat().resolvedOptions()));
  item('Intl.NumberFormat().resolvedOptions()', () => JSON.stringify(Intl.NumberFormat().resolvedOptions()));
  // How the requested hour width meets each locale's own pattern.
  for (const loc of ['es', 'de', 'en-GB', 'en-US', 'he', 'ja']) {
    for (const hour of ['numeric', '2-digit']) {
      const opts = { hour, minute: '2-digit', timeZone: 'UTC' };
      item(`DateTimeFormat ${loc} hour:${hour} minute:2-digit 08:05`, () => {
        const f = new Intl.DateTimeFormat(loc, opts);
        return JSON.stringify(f.format(Date.UTC(2026, 10, 22, 8, 5))) + ' ' + f.resolvedOptions().hour;
      });
    }
    item(`DateTimeFormat ${loc} day:2-digit month:short hour:numeric`, () =>
      JSON.stringify(new Intl.DateTimeFormat(loc, { day: '2-digit', month: 'short', hour: 'numeric', minute: '2-digit', timeZone: 'UTC' }).format(Date.UTC(2025, 8, 5, 21, 7))));
  }
  item('new Date().getTimezoneOffset()', () => new Date().getTimezoneOffset());
  item('new Date(0).toString()', () => new Date(0).toString());
  item("new Date(0).toLocaleString('he-IL')", () => new Date(0).toLocaleString('he-IL'));
  item("new Date(0).toLocaleDateString('es')", () => new Date(0).toLocaleDateString('es'));
  item("new Date(0).toLocaleString()", () => new Date(0).toLocaleString());
  item("(1234.5).toLocaleString('de')", () => (1234.5).toLocaleString('de'));
  item("(1234.5).toLocaleString()", () => (1234.5).toLocaleString());
  item("'ä'.localeCompare('z', 'sv')", () => 'ä'.localeCompare('z', 'sv'));
  item("'ä'.localeCompare('z')", () => 'ä'.localeCompare('z'));
  item("'İ'.toLocaleLowerCase('tr')", () => 'İ'.toLocaleLowerCase('tr'));
  item("'I'.toLocaleLowerCase('tr')", () => 'I'.toLocaleLowerCase('tr'));
  item("'ß'.toUpperCase()", () => 'ß'.toUpperCase());
  item("'é'.normalize('NFD').length", () => 'é'.normalize('NFD').length);
  item("/\\p{L}/u.test('ж')", () => new RegExp('\\p{L}', 'u').test('ж'));
  item("/\\p{Script=Han}/u.test('漢')", () => new RegExp('\\p{Script=Han}', 'u').test('漢'));
  item('Temporal', () => typeof globalThis.Temporal);
  return out.join('\n');
})()
"#;

fn host_zone() -> String {
    let tz = std::env::var("TZ").unwrap_or_else(|_| "unset".into());
    #[cfg(unix)]
    let link = std::fs::read_link("/etc/localtime")
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unreadable".into());
    #[cfg(not(unix))]
    let link = String::from("n/a");
    format!("TZ={tz}; /etc/localtime -> {link}")
}

/// ICU's own versions, from whichever ICU the process has loaded: JavaScript
/// exposes none of them. ICU may export its C API under plain names or with
/// a `_<major>` suffix; a statically linked ICU with hidden symbols is not
/// found.
fn icu_versions() -> String {
    type Version = [u8; 4];
    let text = |v: Version| format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3]);
    let names = |base: &str| {
        let mut names = vec![base.to_string()];
        names.extend((50..100).map(|major| format!("{base}_{major}")));
        names
    };
    let mut found = Vec::new();
    for (label, base, takes_status) in [
        ("ICU", "u_getVersion", false),
        ("Unicode", "u_getUnicodeVersion", false),
        ("CLDR", "ulocdata_getCLDRVersion", true),
    ] {
        let symbol = names(base)
            .into_iter()
            .find_map(|name| lookup(&name).map(|p| (name, p)));
        let Some((name, ptr)) = symbol else {
            found.push(format!("{label} not found"));
            continue;
        };
        let mut v: Version = [0; 4];
        unsafe {
            if takes_status {
                let f: extern "C" fn(*mut u8, *mut i32) = std::mem::transmute(ptr);
                let mut status = 0i32;
                f(v.as_mut_ptr(), &mut status);
            } else {
                let f: extern "C" fn(*mut u8) = std::mem::transmute(ptr);
                f(v.as_mut_ptr());
            }
        }
        found.push(format!("{label} {} ({name})", text(v)));
    }
    found.join("; ")
}

#[cfg(unix)]
fn lookup(name: &str) -> Option<*mut std::ffi::c_void> {
    unsafe extern "C" {
        fn dlsym(
            handle: *mut std::ffi::c_void,
            symbol: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_void;
    }
    // RTLD_DEFAULT: every object loaded into the process.
    #[cfg(target_vendor = "apple")]
    let handle = -2isize as *mut std::ffi::c_void;
    #[cfg(not(target_vendor = "apple"))]
    let handle = std::ptr::null_mut();
    let name = std::ffi::CString::new(name).ok()?;
    let p = unsafe { dlsym(handle, name.as_ptr()) };
    (!p.is_null()).then_some(p)
}

#[cfg(windows)]
fn lookup(name: &str) -> Option<*mut std::ffi::c_void> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleA(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
        fn GetProcAddress(
            module: *mut std::ffi::c_void,
            name: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_void;
    }
    let symbol = std::ffi::CString::new(name).ok()?;
    let mut modules = vec!["icu.dll".to_string(), "icuuc.dll".to_string()];
    modules.extend((50..100).map(|major| format!("icuuc{major}.dll")));
    modules.into_iter().find_map(|m| {
        let m = std::ffi::CString::new(m).ok()?;
        let handle = unsafe { GetModuleHandleA(m.as_ptr()) };
        if handle.is_null() {
            return None;
        }
        let p = unsafe { GetProcAddress(handle, symbol.as_ptr()) };
        (!p.is_null()).then_some(p)
    })
}

#[test]
fn locale_builtins() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let engine = format!("{} {}", RongJS::name(), RongJS::version());
    let lines: String = ctx
        .eval(Source::from_bytes(PROBE))
        .map_err(|e| e.to_string())?;
    let mut report = vec![
        format!("host zone | {}", host_zone()),
        format!("ICU loaded in process | {}", icu_versions()),
    ];
    report.extend(lines.lines().map(str::to_string));
    for line in &report {
        let line = format!("[locale_probe] {engine} | {line}");
        #[cfg(target_env = "ohos")]
        log::info!("{line}");
        // Written to the stream directly: the test harness captures only the
        // print macros, and this log is the test's whole output.
        #[cfg(not(target_env = "ohos"))]
        {
            use std::io::Write;
            let _ = writeln!(std::io::stderr(), "{line}");
        }
    }
    Ok(())
}
