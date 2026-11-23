//! A data blob without what a constructor needs is reported once through
//! the reporter, not answered silently. Own test binary: the reporter is
//! process-wide and set once.

use rong_test::*;
use std::sync::{Arc, Mutex};

/// Locale services only: no plural data at all.
const NO_PLURALS: &[u8] = include_bytes!("data/locale-services.postcard");

#[test]
fn missing_plural_data_is_reported_once() -> Result<(), String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let native: bool = ctx
        .eval(Source::from_bytes("typeof globalThis.Intl !== 'undefined'"))
        .map_err(|e| e.to_string())?;
    if native {
        // The engine's own Intl has its own data.
        return Ok(());
    }
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = lines.clone();
    assert!(rong_quickjs_intl::set_reporter(move |line| {
        sink.lock().unwrap().push(line.to_string())
    }));
    let data =
        rong_quickjs_intl::IntlData::try_from_static_blob(NO_PLURALS).map_err(|e| e.to_string())?;
    rong_quickjs_intl::configure(&ctx, data, rong_quickjs_intl::IntlSettings::default());
    rong_quickjs_intl::init::<RongJS>(&ctx).map_err(|e| e.to_string())?;

    let got: String = ctx
        .eval(Source::from_bytes(
            "const pr = new Intl.PluralRules('en'); [pr.select(1), pr.select(2)].join()",
        ))
        .map_err(|e| e.to_string())?;
    assert_eq!(got, "other,other");
    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("no cardinal plural rules"),
        "{}",
        lines[0]
    );
    Ok(())
}
