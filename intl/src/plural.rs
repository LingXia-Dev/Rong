//! PluralRuleSelect, PluralRuleSelectRange and the categories a locale uses.

use crate::data::IntlData;
use crate::number::{self, DigitOptions, IntlMV};
use fixed_decimal::UnsignedDecimal;
use icu::decimal::CompactDecimalFormatter;
use icu::locale::Locale;
use icu::plurals::{PluralCategory, PluralOperands, PluralRules, PluralRulesWithRanges};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// [[Notation]], as plural selection sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Notation {
    Standard,
    Scientific,
    Engineering,
    CompactShort,
    CompactLong,
}

impl Notation {
    pub(crate) fn parse(notation: &str, compact_display: &str) -> Option<Self> {
        Some(match (notation, compact_display) {
            ("standard", _) => Self::Standard,
            ("scientific", _) => Self::Scientific,
            ("engineering", _) => Self::Engineering,
            ("compact", "long") => Self::CompactLong,
            ("compact", _) => Self::CompactShort,
            _ => return None,
        })
    }
}

pub(crate) type Rules = Rc<PluralRulesWithRanges<PluralRules>>;
type Compact = Option<Rc<CompactDecimalFormatter>>;

/// ICU4X objects for one context, built on first use per locale.
#[derive(Default)]
pub(crate) struct Cache {
    rules: RefCell<HashMap<(String, bool), Option<Rules>>>,
    compact: RefCell<HashMap<(String, bool), Compact>>,
}

impl Cache {
    /// The locale is a canonical tag, either one the blob supports or the
    /// default, and loading falls back through its parents to `und`. So
    /// `None` means the blob has no plural data for this type at all (an
    /// app blob built without the plural markers): reported once, and
    /// selection answers `other`.
    pub(crate) fn rules(&self, data: &IntlData, locale: &str, ordinal: bool) -> Option<Rules> {
        let key = (locale.to_string(), ordinal);
        if let Some(found) = self.rules.borrow().get(&key) {
            return found.clone();
        }
        let built = locale.parse::<Locale>().ok().and_then(|l| {
            let prefs = (&l).into();
            let provider = data.provider();
            if ordinal {
                PluralRulesWithRanges::try_new_ordinal_unstable(&provider, prefs)
            } else {
                PluralRulesWithRanges::try_new_cardinal_unstable(&provider, prefs)
            }
            .ok()
            .map(Rc::new)
        });
        if built.is_none() {
            crate::report(&format!(
                "rong_intl: the data blob has no {} plural rules or ranges for {locale} \
                 or its fallbacks; Intl.PluralRules answers \"other\"",
                if ordinal { "ordinal" } else { "cardinal" }
            ));
        }
        self.rules.borrow_mut().insert(key, built.clone());
        built
    }

    /// The compact formatter, which knows where the locale puts its compact
    /// exponents. `None` when the blob has no compact data for the locale.
    fn compact(&self, data: &IntlData, locale: &str, long: bool) -> Compact {
        let key = (locale.to_string(), long);
        if let Some(found) = self.compact.borrow().get(&key) {
            return found.clone();
        }
        let built = locale.parse::<Locale>().ok().and_then(|l| {
            let prefs = (&l).into();
            let provider = data.provider();
            let options = Default::default();
            if long {
                CompactDecimalFormatter::try_new_long_unstable(&provider, prefs, options)
            } else {
                CompactDecimalFormatter::try_new_short_unstable(&provider, prefs, options)
            }
            .ok()
            .map(Rc::new)
        });
        self.compact.borrow_mut().insert(key, built.clone());
        built
    }
}

/// One Intl.PluralRules' resolved slots, as Rust reads them.
pub(crate) struct Options {
    pub locale: String,
    pub ordinal: bool,
    pub notation: Notation,
    pub digits: DigitOptions,
}

/// ResolvePlural: the category and FormatNumericToString's unsigned string.
/// Non-finite values are `other` with a string no finite value formats to,
/// which is all ResolvePluralRange compares.
pub(crate) fn resolve(
    cache: &Cache,
    data: &IntlData,
    options: &Options,
    value: &IntlMV,
) -> (PluralCategory, String) {
    let (negative, magnitude) = match value {
        IntlMV::Finite { negative, value } => (*negative, value.clone()),
        IntlMV::NegativeZero => (true, UnsignedDecimal::default()),
        IntlMV::NotANumber => return (PluralCategory::Other, String::from("NaN")),
        IntlMV::PositiveInfinity => return (PluralCategory::Other, String::from("∞")),
        IntlMV::NegativeInfinity => return (PluralCategory::Other, String::from("-∞")),
    };
    let rounded = number::format_numeric_to_string(&options.digits, negative, &magnitude);
    let category = cache.rules(data, &options.locale, options.ordinal).map_or(
        PluralCategory::Other,
        |rules| {
            let operands = operands(cache, data, options, &rounded.value);
            rules.rules().category_for(operands)
        },
    );
    (category, rounded.string)
}

/// The operands of the rounded value, with the exponent the notation shows
/// (CLDR's `c`/`e`): in French 1.5 million is `many` in compact notation
/// ("1,5 M") but `other` in standard notation.
fn operands(
    cache: &Cache,
    data: &IntlData,
    options: &Options,
    value: &UnsignedDecimal,
) -> PluralOperands {
    let magnitude = if value.is_zero() {
        0
    } else {
        value.nonzero_magnitude_start()
    };
    let exponent = match options.notation {
        Notation::Standard => 0,
        Notation::Scientific => magnitude,
        Notation::Engineering => magnitude.div_euclid(3) * 3,
        Notation::CompactShort | Notation::CompactLong => cache
            .compact(
                data,
                &options.locale,
                options.notation == Notation::CompactLong,
            )
            .map_or(0, |f| {
                i16::from(f.compact_exponent_for_magnitude(magnitude))
            }),
    };
    // CLDR exponents are never negative: 0.0015 in scientific notation
    // selects like 0.0015.
    match u8::try_from(exponent) {
        Ok(exponent) if exponent > 0 => {
            let mut significand = value.clone();
            significand.multiply_pow10(-i16::from(exponent));
            PluralOperands::from_significand_and_exponent(&significand, exponent)
        }
        _ => PluralOperands::from(value),
    }
}

/// PluralRuleSelectRange.
pub(crate) fn select_range(
    cache: &Cache,
    data: &IntlData,
    locale: &str,
    ordinal: bool,
    start: PluralCategory,
    end: PluralCategory,
) -> PluralCategory {
    cache
        .rules(data, locale, ordinal)
        .map_or(end, |rules| rules.resolve_range(start, end))
}

/// The categories the locale's rules can return, in the standard's order.
pub(crate) fn categories(
    cache: &Cache,
    data: &IntlData,
    locale: &str,
    ordinal: bool,
) -> Vec<&'static str> {
    let mut found: Vec<PluralCategory> = cache
        .rules(data, locale, ordinal)
        .map(|rules| rules.rules().categories().collect())
        .unwrap_or_else(|| vec![PluralCategory::Other]);
    found.sort_by_key(|c| ORDER.iter().position(|o| o == c));
    found.into_iter().map(name).collect()
}

const ORDER: [PluralCategory; 6] = [
    PluralCategory::Zero,
    PluralCategory::One,
    PluralCategory::Two,
    PluralCategory::Few,
    PluralCategory::Many,
    PluralCategory::Other,
];

pub(crate) fn name(category: PluralCategory) -> &'static str {
    match category {
        PluralCategory::Zero => "zero",
        PluralCategory::One => "one",
        PluralCategory::Two => "two",
        PluralCategory::Few => "few",
        PluralCategory::Many => "many",
        PluralCategory::Other => "other",
    }
}
