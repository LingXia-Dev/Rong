//! A data blob without what a constructor or the host locale needs: the
//! answer stays a locale the blob supports, and the gap is reported once
//! through the reporter, never answered silently. Own test binary: the
//! reporter is process-wide and set once.

use rong_test::*;
use std::sync::{Mutex, OnceLock};

/// Locale services only: no plural data at all.
const NO_PLURALS: &[u8] = include_bytes!("data/locale-services.postcard");
/// 27 locales; no `sw`.
const TEST_BLOB: &[u8] = include_bytes!("data/intl-test.postcard");

fn reported() -> &'static Mutex<Vec<String>> {
    static LINES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    LINES.get_or_init(|| {
        rong_quickjs_intl::set_reporter(|line| reported().lock().unwrap().push(line.to_string()));
        Mutex::new(Vec::new())
    })
}

fn lines_with(text: &str) -> Vec<String> {
    reported()
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains(text))
        .cloned()
        .collect()
}

/// A context with rong_quickjs_intl, or `None` on an engine with its own Intl.
fn context(
    rt: &JSRuntime,
    data: rong_quickjs_intl::IntlData,
    locale: &str,
) -> Result<Option<JSContext>, String> {
    let ctx = rt.context();
    let native: bool = ctx
        .eval(Source::from_bytes("typeof globalThis.Intl !== 'undefined'"))
        .map_err(|e| e.to_string())?;
    if native {
        return Ok(None);
    }
    reported();
    let settings = rong_quickjs_intl::IntlSettings {
        locale: Some(locale.to_string()),
        ..Default::default()
    };
    rong_quickjs_intl::configure(&ctx, data, settings);
    rong_quickjs_intl::init::<RongJS>(&ctx).map_err(|e| e.to_string())?;
    Ok(Some(ctx))
}

fn eval(ctx: &JSContext, source: &str) -> Result<String, String> {
    ctx.eval(Source::from_bytes(source))
        .map_err(|e| e.to_string())
}

#[test]
fn missing_plural_data_is_reported_once() -> Result<(), String> {
    let rt = RongJS::runtime();
    let data =
        rong_quickjs_intl::IntlData::try_from_static_blob(NO_PLURALS).map_err(|e| e.to_string())?;
    let Some(ctx) = context(&rt, data, "en-US")? else {
        return Ok(());
    };
    let got = eval(
        &ctx,
        "const pr = new Intl.PluralRules('en'); \
         [pr.resolvedOptions().locale, pr.select(1), pr.select(2)].join()",
    )?;
    // No plural data for any locale: the root answers, and says so.
    assert_eq!(got, "und,other,other");
    let lines = lines_with("no cardinal plural rules");
    assert_eq!(lines.len(), 1, "{lines:?}");
    Ok(())
}

#[test]
fn a_host_locale_missing_from_the_blob_defaults_to_one_it_holds() -> Result<(), String> {
    let rt = RongJS::runtime();
    let data =
        rong_quickjs_intl::IntlData::try_from_static_blob(TEST_BLOB).map_err(|e| e.to_string())?;
    let Some(ctx) = context(&rt, data, "sw-KE")? else {
        return Ok(());
    };
    let locale = eval(&ctx, "new Intl.PluralRules().resolvedOptions().locale")?;
    assert_ne!(locale, "sw-KE");
    assert_ne!(locale, "und");
    let supported = eval(
        &ctx,
        &format!("Intl.PluralRules.supportedLocalesOf('{locale}').join()"),
    )?;
    assert_eq!(supported, locale);
    // Unsupported requests resolve to the same default; reported once.
    let again = eval(&ctx, "new Intl.PluralRules('xx').resolvedOptions().locale")?;
    assert_eq!(again, locale);
    let lines = lines_with("host locale sw-KE");
    assert_eq!(lines.len(), 1, "{lines:?}");

    // The blob's declared fallback wins over its first locale.
    let data = rong_quickjs_intl::IntlData::try_from_static_blob(TEST_BLOB)
        .and_then(|d| d.with_fallback_locale("fr"))
        .map_err(|e| e.to_string())?;
    let ctx = context(&rt, data, "sw-KE")?.ok_or("engine changed")?;
    let locale = eval(&ctx, "new Intl.PluralRules().resolvedOptions().locale")?;
    assert_eq!(locale, "fr");

    // A host locale the blob holds through fallback is used as such.
    let data =
        rong_quickjs_intl::IntlData::try_from_static_blob(TEST_BLOB).map_err(|e| e.to_string())?;
    let ctx = context(&rt, data, "fr-CA")?.ok_or("engine changed")?;
    let locale = eval(&ctx, "new Intl.PluralRules().resolvedOptions().locale")?;
    assert_eq!(locale, "fr-CA");
    Ok(())
}
