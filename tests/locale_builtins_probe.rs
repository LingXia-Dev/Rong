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

#[test]
fn locale_builtins() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let engine = format!("{} {}", RongJS::name(), RongJS::version());
    let lines: String = ctx
        .eval(Source::from_bytes(PROBE))
        .map_err(|e| e.to_string())?;
    let mut report = vec![format!("host zone | {}", host_zone())];
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
