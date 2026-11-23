//! Intl.RelativeTimeFormat's PartitionRelativeTimePattern over ICU4X's
//! relative time data; the number goes through Intl.NumberFormat's parts.

use crate::data::IntlData;
use crate::number::IntlMV;
use crate::number_format::Part;
use fixed_decimal::UnsignedDecimal;
use icu::plurals::{PluralOperands, PluralRules};
use icu_experimental::relativetime::provider::*;
use icu_pattern::PatternItem;
use icu_provider::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// One unit's patterns in one style.
type Patterns = DataPayload<DatetimeRelativeSecondLongV1>;

/// A relative time part: [[Type]], [[Value]] and, for the number's parts,
/// the [[Unit]].
pub(crate) type RelativePart = (&'static str, String, Option<&'static str>);

/// ICU4X data per context, loaded on first use.
#[derive(Default)]
pub(crate) struct Cache {
    patterns: RefCell<HashMap<(String, &'static str, &'static str), Rc<Patterns>>>,
}

/// The singular units, as ECMA-402's SingularRelativeTimeUnit returns them.
const UNITS: [&str; 8] = [
    "second", "minute", "hour", "day", "week", "month", "quarter", "year",
];
const STYLES: [&str; 3] = ["long", "short", "narrow"];

fn load_patterns(
    data: &IntlData,
    locale: &str,
    unit: &str,
    style: &str,
) -> Result<Patterns, String> {
    let parsed = locale
        .parse::<icu::locale::Locale>()
        .map_err(|_| format!("{locale} is not a locale"))?;
    let data_locale = DataLocale::from(&parsed);
    let provider = data.provider();
    let request = DataRequest {
        id: DataIdentifierBorrowed::for_locale(&data_locale),
        ..Default::default()
    };
    macro_rules! by_unit {
        ($(($u:literal, $s:literal) => $marker:ty),* $(,)?) => {
            match (unit, style) {
                $(($u, $s) => DataProvider::<$marker>::load(&provider, request)
                    .map(|r| r.payload.cast()),)*
                _ => return Err(format!("{unit} is not a relative time unit")),
            }
        };
    }
    by_unit!(
        ("second", "long") => DatetimeRelativeSecondLongV1,
        ("second", "short") => DatetimeRelativeSecondShortV1,
        ("second", "narrow") => DatetimeRelativeSecondNarrowV1,
        ("minute", "long") => DatetimeRelativeMinuteLongV1,
        ("minute", "short") => DatetimeRelativeMinuteShortV1,
        ("minute", "narrow") => DatetimeRelativeMinuteNarrowV1,
        ("hour", "long") => DatetimeRelativeHourLongV1,
        ("hour", "short") => DatetimeRelativeHourShortV1,
        ("hour", "narrow") => DatetimeRelativeHourNarrowV1,
        ("day", "long") => DatetimeRelativeDayLongV1,
        ("day", "short") => DatetimeRelativeDayShortV1,
        ("day", "narrow") => DatetimeRelativeDayNarrowV1,
        ("week", "long") => DatetimeRelativeWeekLongV1,
        ("week", "short") => DatetimeRelativeWeekShortV1,
        ("week", "narrow") => DatetimeRelativeWeekNarrowV1,
        ("month", "long") => DatetimeRelativeMonthLongV1,
        ("month", "short") => DatetimeRelativeMonthShortV1,
        ("month", "narrow") => DatetimeRelativeMonthNarrowV1,
        ("quarter", "long") => DatetimeRelativeQuarterLongV1,
        ("quarter", "short") => DatetimeRelativeQuarterShortV1,
        ("quarter", "narrow") => DatetimeRelativeQuarterNarrowV1,
        ("year", "long") => DatetimeRelativeYearLongV1,
        ("year", "short") => DatetimeRelativeYearShortV1,
        ("year", "narrow") => DatetimeRelativeYearNarrowV1,
    )
    .map_err(|_| {
        format!(
            "the data blob has no {style} relative {unit} patterns for {locale} or its fallbacks"
        )
    })
}

impl Cache {
    fn get(
        &self,
        data: &IntlData,
        locale: &str,
        unit: &str,
        style: &str,
    ) -> Result<Rc<Patterns>, String> {
        let unit = UNITS
            .iter()
            .find(|u| **u == unit)
            .ok_or_else(|| format!("{unit} is not a relative time unit"))?;
        let style = STYLES
            .iter()
            .find(|s| **s == style)
            .ok_or_else(|| format!("{style} is not a relative time style"))?;
        let key = (locale.to_string(), *unit, *style);
        if let Some(found) = self.patterns.borrow().get(&key) {
            return Ok(found.clone());
        }
        let built = Rc::new(load_patterns(data, locale, unit, style)?);
        self.patterns.borrow_mut().insert(key, built.clone());
        Ok(built)
    }
}

/// PartitionRelativeTimePattern for a finite value. `number` is
/// PartitionNumberPattern with the plural operands of the value it shows.
#[allow(clippy::too_many_arguments, reason = "the slots of one formatter")]
pub(crate) fn format_to_parts(
    cache: &Cache,
    data: &IntlData,
    rules: Option<&PluralRules>,
    locale: &str,
    style: &str,
    numeric_auto: bool,
    unit: &str,
    value: &IntlMV,
    number: impl Fn(&IntlMV) -> (Vec<Part>, Option<PluralOperands>),
) -> Result<Vec<RelativePart>, String> {
    let patterns = cache.get(data, locale, unit, style)?;
    let patterns = patterns.get();
    let unit = UNITS
        .iter()
        .copied()
        .find(|u| *u == unit)
        .unwrap_or("second");
    let (past, magnitude) = match value {
        IntlMV::NegativeZero => (true, UnsignedDecimal::default()),
        IntlMV::Finite { negative, value } => (*negative, value.clone()),
        _ => return Err(String::from("relative time values are finite")),
    };
    // numeric "auto": the unit's own name for an offset ("yesterday"); ToString(-0) is "0".
    if numeric_auto
        && let Some(offset) = integer_offset(&magnitude, past)
        && let Some(name) = patterns.get_relative(offset)
    {
        return Ok(vec![("literal", name.to_string(), None)]);
    }
    let (parts, operands) = number(&IntlMV::Finite {
        negative: false,
        value: magnitude,
    });
    let set = if past {
        &patterns.past
    } else {
        &patterns.future
    };
    let pattern = match (operands, rules) {
        (Some(operands), Some(rules)) => set.get(operands, rules),
        _ => set.elements.get_default().1,
    };
    let mut out = Vec::new();
    for item in pattern.iter() {
        match item {
            PatternItem::Literal(text) if !text.is_empty() => {
                out.push(("literal", text.to_string(), None))
            }
            PatternItem::Literal(_) => {}
            PatternItem::Placeholder(_) => out.extend(
                parts
                    .iter()
                    .map(|(kind, text)| (*kind, text.clone(), Some(unit))),
            ),
        }
    }
    Ok(out)
}

/// The value as an i8 offset, when it is an integer in range.
fn integer_offset(magnitude: &UnsignedDecimal, negative: bool) -> Option<i8> {
    if magnitude.is_zero() {
        return Some(0);
    }
    if magnitude.nonzero_magnitude_end() < 0 || magnitude.nonzero_magnitude_start() > 2 {
        return None;
    }
    let mut whole = 0i16;
    for position in (0..=magnitude.nonzero_magnitude_start()).rev() {
        whole = whole * 10 + i16::from(magnitude.digit_at(position));
    }
    let signed = if negative { -whole } else { whole };
    i8::try_from(signed).ok()
}
