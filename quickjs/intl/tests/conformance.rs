//! The conformance suite in `tests/conformance/`: each file runs in a fresh
//! context, on QuickJS with rong_quickjs_intl and on JavaScriptCore's own
//! Intl as the reference. Every case names the ECMA-402 clause it checks;
//! the log carries a count per file and clause. The checks JavaScript alone
//! cannot make (collection after drop, several contexts) are here.

use rong_test::*;
use std::collections::BTreeMap;
use std::io::Write;

const BLOB: &[u8] = include_bytes!("data/intl-test.postcard");
const HARNESS: &str = include_str!("conformance/harness.js");
const FILES: &[(&str, &str)] = &[
    ("intl_object.js", include_str!("conformance/intl_object.js")),
    (
        "plural_rules.js",
        include_str!("conformance/plural_rules.js"),
    ),
    (
        "number_format.js",
        include_str!("conformance/number_format.js"),
    ),
    (
        "date_time_format.js",
        include_str!("conformance/date_time_format.js"),
    ),
    ("robustness.js", include_str!("conformance/robustness.js")),
    ("regressions.js", include_str!("conformance/regressions.js")),
];

fn log(line: &str) {
    let _ = writeln!(
        std::io::stderr(),
        "[conformance] {} | {line}",
        RongJS::name()
    );
}

/// rong's Intl on an engine without one; whether it was installed.
fn install_intl(ctx: &JSContext, locale: Option<&str>) -> Result<bool, String> {
    let native: bool = ctx
        .eval(Source::from_bytes("typeof globalThis.Intl !== 'undefined'"))
        .map_err(|e| e.to_string())?;
    if native {
        return Ok(false);
    }
    let data =
        rong_quickjs_intl::IntlData::try_from_static_blob(BLOB).map_err(|e| e.to_string())?;
    let settings = rong_quickjs_intl::IntlSettings {
        locale: locale.map(str::to_string),
        ..Default::default()
    };
    rong_quickjs_intl::configure(ctx, data, settings);
    rong_quickjs_intl::init::<RongJS>(ctx).map_err(|e| e.to_string())?;
    Ok(true)
}

fn eval<T: FromJSValue<JSEngineValue>>(ctx: &JSContext, source: &str) -> Result<T, String> {
    ctx.eval(Source::from_bytes(source))
        .map_err(|e| e.to_string())
}

#[test]
fn every_case_passes_on_this_engine() -> Result<(), String> {
    let mut failures = Vec::new();
    let mut total = 0;
    for (file, source) in FILES {
        let rt = RongJS::runtime();
        let ctx = rt.context();
        let ours = install_intl(&ctx, Some("en-US"))?;
        let engine = if ours { "rong" } else { "reference" };
        eval::<()>(&ctx, &format!("var ENGINE = {engine:?};"))?;
        eval::<()>(&ctx, HARNESS)?;
        if let Err(e) = eval::<()>(&ctx, source) {
            failures.push(format!("{file}: the file threw: {e}"));
            continue;
        }
        let rows: Vec<Vec<String>> = eval(&ctx, "__results")?;
        // clause -> (passed, run, skipped)
        let mut by_clause: BTreeMap<String, (u32, u32, u32)> = BTreeMap::new();
        for row in &rows {
            let (clause, name, outcome) = (&row[0], &row[1], &row[2]);
            let counts = by_clause.entry(clause.clone()).or_default();
            if outcome == "skip" {
                counts.2 += 1;
                log(&format!("skip | {file} | {clause} | {name}"));
                continue;
            }
            counts.1 += 1;
            if outcome.is_empty() {
                counts.0 += 1;
            } else {
                failures.push(format!("{file} | {clause} | {name}: {outcome}"));
            }
        }
        total += rows.len();
        for (clause, (passed, run, skipped)) in &by_clause {
            log(&format!(
                "{engine} | {file} | {clause} | {passed}/{run} passed, {skipped} skipped"
            ));
        }
    }
    log(&format!("{total} cases, {} failed", failures.len()));
    for failure in &failures {
        log(&format!("FAIL | {failure}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("{} of {total} cases failed", failures.len()))
    }
}

/// Formatters a script drops are collected: nothing in rong's Intl keeps
/// them alive. QuickJS collects on request; JavaScriptCore collects on its
/// own schedule, so the check runs where collection can be forced.
#[test]
fn dropped_formatters_are_collected() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    if !install_intl(&ctx, None)? {
        log("collection: skipped on the reference, which collects on its own schedule");
        return Ok(());
    }
    eval::<()>(
        &ctx,
        "var refs = [];
         (function () {
           for (let i = 0; i < 2000; i++) {
             const nf = new Intl.NumberFormat(['en', 'fr', 'ar'][i % 3], { minimumFractionDigits: i % 4 });
             nf.format(i);
             const bound = nf.format;
             const pr = new Intl.PluralRules('en', { type: i % 2 ? 'ordinal' : 'cardinal' });
             pr.select(i);
             refs.push(new WeakRef(nf), new WeakRef(bound), new WeakRef(pr));
           }
         })();",
    )?;
    rt.run_pending_jobs();
    rt.run_gc();
    rt.run_gc();
    let alive: u32 = eval(&ctx, "refs.filter((r) => r.deref() !== undefined).length")?;
    log(&format!(
        "collection: {alive} of 6000 formatters alive after collection"
    ));
    if alive != 0 {
        return Err(format!("{alive} dropped formatters were not collected"));
    }
    Ok(())
}

/// Two contexts in one runtime and a second runtime each have their own
/// Intl, settings and caches; a formatter keeps working after another
/// context is dropped.
#[test]
fn contexts_are_independent() -> Result<(), String> {
    let rt = RongJS::runtime();
    let first = rt.context();
    if !install_intl(&first, Some("fr"))? {
        log("contexts: skipped on the reference, whose Intl is the engine's");
        return Ok(());
    }
    let second = rt.context();
    install_intl(&second, Some("de"))?;
    eval::<()>(
        &first,
        "var nf = new Intl.NumberFormat(); var pr = new Intl.PluralRules();",
    )?;
    eval::<()>(&second, "var nf = new Intl.NumberFormat();")?;
    let a: String = eval(
        &first,
        "nf.resolvedOptions().locale + ' ' + nf.format(1234.5)",
    )?;
    let b: String = eval(
        &second,
        "nf.resolvedOptions().locale + ' ' + nf.format(1234.5)",
    )?;
    assert_eq!(a, "fr 1\u{202f}234,5");
    assert_eq!(b, "de 1.234,5");
    let other = RongJS::runtime();
    let third = other.context();
    install_intl(&third, Some("ja"))?;
    let c: String = eval(&third, "new Intl.NumberFormat().resolvedOptions().locale")?;
    assert_eq!(c, "ja");
    drop(second);
    rt.run_gc();
    let again: String = eval(&first, "nf.format(5) + ' ' + pr.select(1)")?;
    assert_eq!(again, "5 one");
    // A formatter's methods called with a receiver from another context.
    let cross = rt.context();
    install_intl(&cross, Some("en-US"))?;
    eval::<()>(&cross, "var nfx = new Intl.NumberFormat('en');")?;
    let foreign: JSObject = eval(&cross, "nfx")?;
    first
        .global()
        .set("foreign", foreign)
        .map_err(|e| e.to_string())?;
    let verdict: String = eval(
        &first,
        "try { Intl.NumberFormat.prototype.formatToParts.call(foreign, 1); 'accepted' } \
         catch (e) { e instanceof TypeError ? 'TypeError' : 'other ' + e }",
    )?;
    log(&format!(
        "contexts: another context's formatter as receiver: {verdict}"
    ));
    assert_eq!(verdict, "TypeError");
    Ok(())
}
