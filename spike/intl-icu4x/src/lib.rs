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
    use icu_provider_blob::BlobDataProvider;

    pub fn run(blob: &[u8], locale: &str) -> String {
        let mut out = String::new();
        let Ok(p) = BlobDataProvider::try_new_from_blob(blob.to_vec().into_boxed_slice()) else {
            return out;
        };
        let Ok(mut loc) = locale.parse::<Locale>() else {
            return out;
        };
        let _ = LocaleFallbacker::try_new_with_buffer_provider(&p);
        if let Ok(c) = LocaleCanonicalizer::try_new_extended_with_buffer_provider(&p) {
            c.canonicalize(&mut loc);
        }
        if let Ok(e) = LocaleExpander::try_new_extended_with_buffer_provider(&p) {
            e.maximize(&mut loc.id);
        }
        let prefs = (&loc).into();
        let mut b = FieldSetBuilder::new();
        b.date_fields = Some(DateFields::YMDE);
        b.length = Some(Length::Medium);
        b.time_precision = Some(TimePrecision::Minute);
        if let Ok(fs) = b.build_composite_datetime()
            && let Ok(f) = DateTimeFormatter::try_new_with_buffer_provider(&p, prefs, fs)
            && let (Ok(date), Ok(time)) =
                (Date::try_new_iso(2026, 11, 22), Time::try_new(8, 5, 0, 0))
        {
            let _ = write!(out, "{}", f.format(&DateTime { date, time }));
        }
        if let Ok(f) =
            DecimalFormatter::try_new_with_buffer_provider(&p, (&loc).into(), Default::default())
        {
            let _ = write!(out, "|{}", f.format(&Decimal::from(12345)));
        }
        if let Ok(r) = PluralRules::try_new_cardinal_with_buffer_provider(&p, (&loc).into()) {
            let _ = write!(out, "|{:?}", r.category_for(3u32));
        }
        if let Ok(r) = RelativeTimeFormatter::try_new_long_day_with_buffer_provider(
            &p,
            (&loc).into(),
            RelativeTimeFormatterOptions::default(),
        ) {
            let _ = write!(out, "|{}", r.format(Decimal::from(-2)));
        }
        if let Ok(l) = ListFormatter::try_new_and_with_buffer_provider(
            &p,
            (&loc).into(),
            ListFormatterOptions::default(),
        ) {
            let _ = write!(out, "|{}", l.format(["a", "b", "c"].into_iter()));
        }
        if let Ok(c) =
            Collator::try_new_with_buffer_provider(&p, (&loc).into(), CollatorOptions::default())
        {
            let _ = write!(out, "|{:?}", c.as_borrowed().compare("ä", "z"));
        }
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
