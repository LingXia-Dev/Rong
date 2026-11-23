//! Size probe: the first-cut Intl components linked into one library, with
//! data read from a blob at run time. Without the `blob` feature it is the
//! empty baseline the size is measured against.

#[cfg(feature = "blob")]
mod probe {
    use core::fmt::Write;
    use icu::calendar::Date;
    use icu::collator::{Collator, options::CollatorOptions};
    use icu::datetime::DateTimeFormatter;
    use icu::datetime::fieldsets::builder::{DateFields, FieldSetBuilder};
    use icu::datetime::options::{Length, TimePrecision};
    use icu::decimal::DecimalFormatter;
    use icu::decimal::input::Decimal;
    use icu::experimental::relativetime::{RelativeTimeFormatter, RelativeTimeFormatterOptions};
    use icu::list::{ListFormatter, options::ListFormatterOptions};
    use icu::locale::fallback::LocaleFallbacker;
    use icu::locale::{Locale, LocaleCanonicalizer, LocaleExpander};
    use icu::plurals::PluralRules;
    use icu::time::DateTime;
    use icu::time::Time;
    use icu_provider::buf::AsDeserializingBufferProvider;
    use icu_provider_adapters::fallback::LocaleFallbackProvider;
    use icu_provider_blob::BlobDataProvider;

    pub fn run(blob: &[u8], locale: &str) -> String {
        let mut out = String::new();
        let p = match BlobDataProvider::try_new_from_blob(blob.to_vec().into_boxed_slice()) {
            Ok(p) => p,
            Err(e) => return format!("blob {e:?}"),
        };
        let Ok(mut loc) = locale.parse::<Locale>() else {
            return out;
        };
        let fallbacker = match LocaleFallbacker::try_new_unstable(&p.as_deserializing()) {
            Ok(f) => f,
            Err(e) => return format!("fallbacker {e:?}"),
        };
        let fp = LocaleFallbackProvider::new(p.as_deserializing(), fallbacker);
        let mut note = |name: &str, r: Result<String, String>| {
            let _ = match r {
                Ok(text) => write!(out, "{text} | "),
                Err(e) => write!(out, "{name} failed: {e} | "),
            };
        };
        let err = |e: &dyn core::fmt::Debug| format!("{e:?}");
        note(
            "canonicalizer",
            LocaleCanonicalizer::try_new_extended_unstable(&fp)
                .map(|c| {
                    c.canonicalize(&mut loc);
                    loc.to_string()
                })
                .map_err(|e| err(&e)),
        );
        note(
            "expander",
            LocaleExpander::try_new_extended_unstable(&fp)
                .map(|e| {
                    let mut id = loc.id.clone();
                    e.maximize(&mut id);
                    id.to_string()
                })
                .map_err(|e| err(&e)),
        );
        let mut b = FieldSetBuilder::new();
        b.date_fields = Some(DateFields::YMDE);
        b.length = Some(Length::Medium);
        b.time_precision = Some(TimePrecision::Minute);
        note(
            "datetime",
            b.build_composite_datetime()
                .map_err(|e| err(&e))
                .and_then(|fs| {
                    DateTimeFormatter::try_new_unstable(&fp, (&loc).into(), fs).map_err(|e| err(&e))
                })
                .map(|f| {
                    let date = Date::try_new_iso(2026, 11, 22).unwrap();
                    let time = Time::try_new(8, 5, 0, 0).unwrap();
                    f.format(&DateTime { date, time }).to_string()
                }),
        );
        note(
            "decimal",
            DecimalFormatter::try_new_unstable(&fp, (&loc).into(), Default::default())
                .map(|f| f.format(&Decimal::from(12345)).to_string())
                .map_err(|e| err(&e)),
        );
        note(
            "plurals",
            PluralRules::try_new_cardinal_unstable(&fp, (&loc).into())
                .map(|r| format!("{:?}", r.category_for(3u32)))
                .map_err(|e| err(&e)),
        );
        note(
            "relativetime",
            RelativeTimeFormatter::try_new_long_day_unstable(
                &fp,
                (&loc).into(),
                RelativeTimeFormatterOptions::default(),
            )
            .map(|r| r.format(Decimal::from(-2)).to_string())
            .map_err(|e| err(&e)),
        );
        note(
            "list",
            ListFormatter::try_new_and_unstable(
                &fp,
                (&loc).into(),
                ListFormatterOptions::default(),
            )
            .map(|l| l.format(["a", "b", "c"].into_iter()).to_string())
            .map_err(|e| err(&e)),
        );
        note(
            "collator",
            Collator::try_new_unstable(&fp, (&loc).into(), CollatorOptions::default())
                .map(|c| format!("{:?}", c.as_borrowed().compare("ä", "z")))
                .map_err(|e| err(&e)),
        );
        out
    }
}

/// Formats with every first-cut component; returns the bytes written.
///
/// # Safety
/// The pointers must be valid for the given lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn intl_probe(
    blob: *const u8,
    blob_len: usize,
    locale: *const u8,
    locale_len: usize,
    out: *mut u8,
    cap: usize,
) -> usize {
    #[cfg(feature = "blob")]
    {
        let (blob, locale) = unsafe {
            (
                core::slice::from_raw_parts(blob, blob_len),
                core::slice::from_raw_parts(locale, locale_len),
            )
        };
        let text = probe::run(blob, core::str::from_utf8(locale).unwrap_or("und"));
        let n = text.len().min(cap);
        unsafe { core::ptr::copy_nonoverlapping(text.as_ptr(), out, n) };
        n
    }
    #[cfg(not(feature = "blob"))]
    {
        let _ = (blob, blob_len, locale, locale_len, out, cap);
        0
    }
}
