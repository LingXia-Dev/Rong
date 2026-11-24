//! What ICU4X 2.4's `DateRangeFormatter` needs checked before it formats.
//!
//! Its range patterns come from CLDR's interval formats and are written
//! with the names its inner `DateTimeFormatter` loaded for the single-date
//! patterns. The two disagree in many locales: en `dateStyle: 'long'`
//! ranges use `MMM` where the date uses `MMMM`, zh-Hans `'medium'` uses
//! `MMM` where the date uses `M`, every `'full'` range uses `E` where the
//! date uses `EEEE`. A field whose names were not loaded fails inside
//! ICU4X: a `debug_assert!` panic in debug builds, the month code (`M11`)
//! or `{E}` in the text in release builds.
//!
//! [`Watch`] is the provider the range formatter is built through. It
//! empties every range pattern whose names the single-date patterns did not
//! load, so ICU4X takes its own fallback there (both dates in full, joined
//! by the locale's range glue), and it records whether a decimal formatter
//! was loaded: ICU4X 2.4 skips it for some patterns that need it (pt
//! `{year, month: 'long'}`), which fails the same way.

use crate::data::Provider;
use icu::datetime::provider::packed_pattern::GenericPackedPatterns;
use icu::datetime::provider::pattern::runtime::Pattern;
use icu::datetime::provider::range_patterns::*;
use icu_provider::prelude::*;
use std::cell::Cell;
use writeable::Writeable;
use zerovec::VarZeroVec;

/// A UTS 35 pattern's fields, as (symbol, length), outside quotes.
pub(crate) fn pattern_runs(pattern: &str) -> Vec<(char, usize)> {
    let mut runs: Vec<(char, usize)> = Vec::new();
    let mut quoted = false;
    let mut last: Option<char> = None;
    for c in pattern.chars() {
        if c == '\'' {
            quoted = !quoted;
            last = None;
            continue;
        }
        if quoted || !c.is_ascii_alphabetic() {
            last = None;
            continue;
        }
        match runs.last_mut() {
            Some((symbol, n)) if last == Some(c) && *symbol == c => *n += 1,
            _ => runs.push((c, 1)),
        }
        last = Some(c);
    }
    runs
}

/// The names a field is written with, as ICU4X loads them: `None` for a
/// field written with digits alone. Months always look up names (a numeric
/// month can carry a locale's pattern, such as ko's `{0}월`).
fn names(symbol: char, length: usize) -> Option<(char, usize)> {
    match symbol {
        'M' | 'L' => Some((symbol, length.max(2))),
        'e' | 'c' if length >= 3 => Some((symbol, length)),
        'G' | 'E' | 'a' | 'b' | 'B' | 'U' => Some((symbol, length.max(3))),
        'z' | 'Z' | 'O' | 'v' | 'V' | 'x' | 'X' => Some((symbol, length)),
        _ => None,
    }
}

/// The names `patterns` need, without repeats.
pub(crate) fn names_of<'a>(patterns: impl IntoIterator<Item = &'a str>) -> Vec<(char, usize)> {
    let mut out: Vec<(char, usize)> = Vec::new();
    for pattern in patterns {
        for (symbol, length) in pattern_runs(pattern) {
            if let Some(need) = names(symbol, length)
                && !out.contains(&need)
            {
                out.push(need);
            }
        }
    }
    out
}

/// The provider a range formatter is built through; see the module docs.
pub(crate) struct Watch<'a> {
    inner: Provider<'a>,
    /// The names the single-date patterns load.
    loaded: Vec<(char, usize)>,
    decimal: Cell<bool>,
    emptied: Cell<usize>,
}

impl<'a> Watch<'a> {
    pub(crate) fn new(inner: Provider<'a>, loaded: Vec<(char, usize)>) -> Self {
        Self {
            inner,
            loaded,
            decimal: Cell::new(false),
            emptied: Cell::new(0),
        }
    }

    /// Whether a decimal formatter was loaded.
    pub(crate) fn decimal(&self) -> bool {
        self.decimal.get()
    }

    /// How many range patterns were emptied.
    pub(crate) fn emptied(&self) -> usize {
        self.emptied.get()
    }

    fn covered(&self, pattern: &str) -> bool {
        names_of([pattern])
            .iter()
            .all(|need| self.loaded.contains(need))
    }

    /// The range patterns with every entry that needs names the single-date
    /// patterns did not load replaced by one with no patterns, which ICU4X
    /// answers with its fallback. Entries keep their places, so the header
    /// that indexes them by length and variant still holds.
    fn checked(&self, packed: &PackedRangePatterns) -> Option<PackedRangePatterns<'static>> {
        let mut emptied = 0;
        let entries: Vec<PatternsByGreatestDifference> = packed
            .elements
            .iter()
            .map(|entry| {
                let usable = entry.patterns.iter().all(|pattern| {
                    let pattern: Pattern = zerofrom::ZeroFrom::zero_from(pattern);
                    self.covered(&pattern.write_to_string())
                });
                if usable {
                    zerofrom::ZeroFrom::zero_from(entry)
                } else {
                    emptied += 1;
                    PatternsByGreatestDifference {
                        header: GreatestDifferenceHeader(0),
                        patterns: VarZeroVec::new(),
                    }
                }
            })
            .collect();
        if emptied == 0 {
            return None;
        }
        self.emptied.set(self.emptied.get() + emptied);
        Some(GenericPackedPatterns {
            header: packed.header,
            elements: VarZeroVec::from(&entries),
        })
    }
}

/// Checks the payload if it is one of `markers`.
macro_rules! check_range_patterns {
    ($watch:ident, $payload:expr, $($marker:ident),+ $(,)?) => {
        $(
            if let Ok(payload) = $payload.dynamic_cast_mut::<$marker>() {
                if let Some(checked) = $watch.checked(payload.get()) {
                    *payload = DataPayload::from_owned(checked);
                }
            }
        )+
    };
}

impl<M: DataMarker> DataProvider<M> for Watch<'_>
where
    for<'p> Provider<'p>: DataProvider<M>,
{
    fn load(&self, req: DataRequest) -> Result<DataResponse<M>, DataError> {
        let mut response = self.inner.load(req)?;
        if M::INFO.id == icu_decimal::provider::DecimalSymbolsV1::INFO.id {
            self.decimal.set(true);
        }
        let payload = &mut response.payload;
        check_range_patterns!(
            self,
            payload,
            DatetimePatternsRangeTimeV1,
            DatetimePatternsRangeDateBuddhistV1,
            DatetimePatternsRangeDateChineseV1,
            DatetimePatternsRangeDateCopticV1,
            DatetimePatternsRangeDateDangiV1,
            DatetimePatternsRangeDateEthiopianV1,
            DatetimePatternsRangeDateGregorianV1,
            DatetimePatternsRangeDateHebrewV1,
            DatetimePatternsRangeDateIndianV1,
            DatetimePatternsRangeDateHijriV1,
            DatetimePatternsRangeDateJapaneseV1,
            DatetimePatternsRangeDatePersianV1,
            DatetimePatternsRangeDateRocV1,
        );
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_skip_quoted_text_and_split_symbols() {
        assert_eq!(
            pattern_runs("EEEE, d 'de' MMMM 'de' y"),
            vec![('E', 4), ('d', 1), ('M', 4), ('y', 1)]
        );
        assert_eq!(pattern_runs("h:mm a"), vec![('h', 1), ('m', 2), ('a', 1)]);
        assert_eq!(pattern_runs("d''MMM"), vec![('d', 1), ('M', 3)]);
    }

    #[test]
    fn widths_that_share_names_are_one_need() {
        assert_eq!(names_of(["E", "EEE"]), vec![('E', 3)]);
        assert_eq!(names_of(["M", "MM"]), vec![('M', 2)]);
        assert_eq!(names_of(["MMM d", "MMMM"]), vec![('M', 3), ('M', 4)]);
        assert_eq!(names_of(["y/MM/dd HH:mm:ss"]), vec![('M', 2)]);
    }
}
