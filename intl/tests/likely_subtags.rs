//! Likely subtags come from the blob: the extended set when it carries
//! `LocaleLikelySubtagsExtendedV1`, else the common one. Twin blobs, equal
//! but for that marker, pin what stays and what changes.

use rong_test::*;

/// en and haw dates and the locale services, common likely subtags only.
const COMMON: &[u8] = include_bytes!("data/likely-common.postcard");
/// The same plus `LocaleLikelySubtagsExtendedV1`.
const EXTENDED: &[u8] = include_bytes!("data/likely-extended.postcard");

/// Evaluates `source` with rong_intl on `blob`; `None` on an engine
/// with its own Intl.
fn eval(blob: &'static [u8], source: &str) -> Result<Option<String>, String> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let native: bool = ctx
        .eval(Source::from_bytes("typeof globalThis.Intl !== 'undefined'"))
        .map_err(|e| e.to_string())?;
    if native {
        return Ok(None);
    }
    let data = rong_intl::IntlData::try_from_static_blob(blob).map_err(|e| e.to_string())?;
    let settings = rong_intl::IntlSettings {
        locale: Some("en-US".to_string()),
        ..Default::default()
    };
    rong_intl::configure(&ctx, data, settings);
    rong_intl::init::<RongJS>(&ctx).map_err(|e| e.to_string())?;
    ctx.eval(Source::from_bytes(source))
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Both blobs' answers, or `None` on an engine with its own Intl.
fn both(source: &str) -> Result<Option<(String, String)>, String> {
    Ok(eval(COMMON, source)?.zip(eval(EXTENDED, source)?))
}

/// test262's complex-region-subtag-replacement cases, the aliases, and the
/// default hour cycle of a language the common data has: the same either way.
#[test]
fn common_languages_answer_the_same() -> Result<(), String> {
    let cases = [
        ("ru-SU", "ru-RU"),
        ("en-SU", "en-RU"),
        ("ru-810", "ru-RU"),
        ("en-810", "en-RU"),
        ("und-SU", "und-RU"),
        ("und-810", "und-RU"),
        ("und-Latn-SU", "und-Latn-RU"),
        ("und-Latn-810", "und-Latn-RU"),
        ("hy-SU", "hy-AM"),
        ("hy-810", "hy-AM"),
        ("und-Armn-SU", "und-Armn-AM"),
        ("und-Armn-810", "und-Armn-AM"),
        ("sr-CS", "sr-RS"),
        ("sr-Latn-CS", "sr-Latn-RS"),
        ("sr-Cyrl-CS", "sr-Cyrl-RS"),
        ("az-NT", "az-SA"),
        ("iw", "he"),
    ];
    let tags: Vec<String> = cases.iter().map(|(from, _)| format!("'{from}'")).collect();
    let want: Vec<&str> = cases.iter().map(|(_, to)| *to).collect();
    let source = format!(
        "[{}].map((t) => Intl.getCanonicalLocales(t)[0]).join() + ';' + \
         new Intl.DateTimeFormat('en', {{ hour: 'numeric' }}).resolvedOptions().hourCycle",
        tags.join(", ")
    );
    let Some((common, extended)) = both(&source)? else {
        return Ok(());
    };
    let expected = format!("{};h12", want.join(","));
    assert_eq!(common, extended);
    assert_eq!(common, expected);
    Ok(())
}

/// A language only the extended set maximizes: its split region's
/// replacement and its default hour cycle follow the data the blob carries.
#[test]
fn extended_only_languages_follow_the_blob() -> Result<(), String> {
    let source = "[Intl.getCanonicalLocales('ab-SU')[0], Intl.getCanonicalLocales('ji-SU')[0], \
         new Intl.DateTimeFormat('haw', { hour: 'numeric', timeZone: 'UTC' }).resolvedOptions().hourCycle, \
         new Intl.DateTimeFormat('haw', { hour: 'numeric', timeZone: 'UTC' }).format(Date.UTC(2026, 0, 1, 15))].join()";
    let Some((common, extended)) = both(source)? else {
        return Ok(());
    };
    // With it: ab is likely in Georgia, yi (ji's replacement) in Ukraine,
    // haw in the US (12-hour clock).
    assert_eq!(extended.replace('\u{202f}', " "), "ab-GE,yi-UA,h12,3 PM");
    // Without it: the first replacement for SU, and the world's hour cycle.
    assert_eq!(common, "ab-RU,yi-RU,h23,15");
    Ok(())
}
