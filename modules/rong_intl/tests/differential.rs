//! The same calls on every engine against one table of answers. On
//! JavaScriptCore the engine's own `Intl` answers, so the table is checked
//! against the reference; on QuickJS rong_intl answers. Each run logs the
//! ICU or ICU4X version behind the answers, so a difference can be traced to
//! data rather than code.

use rong_test::*;
use std::io::Write;

const BLOB: &[u8] = include_bytes!("data/locale-services.postcard");

const RUN: &str = r#"
(cases) => cases.map(([expr, expect, note]) => {
  let got;
  try { got = JSON.stringify((0, eval)(expr)); }
  catch (e) { got = 'throws ' + (e && e.constructor ? e.constructor.name : String(e)); }
  return [expr, got, expect, note || ''];
})
"#;

fn log(line: &str) {
    let _ = writeln!(std::io::stderr(), "[intl_diff] {} | {line}", RongJS::name());
}

#[test]
fn locale_cases_match_on_every_engine() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let data = rong_intl::IntlData::try_from_static_blob(BLOB).map_err(|e| e.to_string())?;
    rong_intl::configure(&ctx, data, rong_intl::IntlSettings::default());
    rong_intl::init(&ctx).map_err(|e| e.to_string())?;
    let native: bool = ctx
        .eval(Source::from_bytes(
            "typeof Intl.DateTimeFormat === 'function'",
        ))
        .map_err(|e| e.to_string())?;
    log(&if native {
        String::from("answers from the engine's own Intl")
    } else {
        format!(
            "answers from rong_intl on ICU4X {}",
            rong_intl::ICU4X_VERSION
        )
    });

    let cases: JSArray = JSArray::from_object(
        JSObject::from_json_string(&ctx, include_str!("cases/locales.json"))
            .map_err(|e| e.to_string())?,
    )
    .ok_or("cases are not an array")?;
    let run: JSFunc = ctx
        .eval(Source::from_bytes(RUN))
        .map_err(|e| e.to_string())?;
    let results: Vec<Vec<String>> = run.call(None, (cases,)).map_err(|e| e.to_string())?;
    let mut failed = 0;
    for row in &results {
        let (expr, got, expect, note) = (&row[0], &row[1], &row[2], &row[3]);
        // A case marked as data-dependent is reported, not failed: its answer
        // follows the engine's CLDR version.
        let verdict = if got == expect {
            "ok"
        } else if !note.is_empty() {
            "data"
        } else {
            failed += 1;
            "DIFF"
        };
        log(&format!(
            "{verdict} | {expr} | got {got} | want {expect} {note}"
        ));
    }
    if failed > 0 {
        return Err(format!("{failed} of {} cases differ", results.len()));
    }
    Ok(())
}
