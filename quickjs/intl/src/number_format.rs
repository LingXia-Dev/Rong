//! Intl.NumberFormat's PartitionNumberPattern, written over ICU4X's data
//! rather than its `DecimalFormatter`: grouping honours `useGrouping:
//! "always"`, the notations follow ComputeExponent, and every part keeps
//! the type ECMA-402 gives it.

use crate::cldr_numbers::{ENTRIES, ROOT_SYMBOLS, RootSymbols, VALUES, Values};
use crate::data::IntlData;
use crate::number::{self, DigitOptions, IntlMV, Rounded};
pub(crate) use crate::plural::Notation;
use fixed_decimal::UnsignedDecimal;
use icu::decimal::provider::{
    DecimalCompactLongV1, DecimalCompactShortV1, DecimalDigitsV1, DecimalSymbolsV1,
};
use icu::plurals::{PluralOperands, PluralRules};
use icu_experimental::dimension::provider::percent::DecimalPercentV1;
use icu_pattern::{DoublePlaceholderKey, PatternItem};
use icu_provider::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// [[Style]], for the styles this module formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Style {
    Decimal,
    Percent,
}

/// [[UseGrouping]].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grouping {
    Always,
    Auto,
    Min2,
    Off,
}

/// [[SignDisplay]].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SignDisplay {
    Auto,
    Never,
    Always,
    ExceptZero,
    Negative,
}

/// One Intl.NumberFormat's resolved slots, as Rust reads them.
pub(crate) struct Options {
    pub locale: String,
    pub numbering_system: String,
    pub style: Style,
    pub notation: Notation,
    pub digits: DigitOptions,
    pub grouping: Grouping,
    pub sign: SignDisplay,
}

/// A part of the result: ECMA-402's [[Type]] and [[Value]].
pub(crate) type Part = (&'static str, String);

/// The digits of a numbering system.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Digits {
    /// Ten consecutive code points from this one.
    Range(char),
    List([char; 10]),
}

impl Digits {
    pub(crate) fn digit(&self, d: u32) -> char {
        match self {
            Digits::Range(zero) => char::from_u32(u32::from(*zero) + d).unwrap_or('?'),
            Digits::List(list) => list[d as usize],
        }
    }
}

/// A piece of a sign-dependent pattern.
#[derive(Clone, Debug)]
enum Piece {
    Literal(String),
    Number,
    Plus(String),
    Minus(String),
    PercentSign(String),
}

impl LocaleFormat {
    /// ASCII digits in the numbering system's.
    fn transliterate(&self, s: &str) -> String {
        s.chars()
            .map(|c| c.to_digit(10).map_or(c, |d| self.digits.digit(d)))
            .collect()
    }
}

/// What formatting needs for one locale, numbering system and style.
pub(crate) struct LocaleFormat {
    decimal: String,
    group: String,
    primary: u8,
    secondary: u8,
    min_grouping: u8,
    digits: Digits,
    cldr: &'static Values,
    /// The minus sign an exponent takes.
    minus: String,
    zero: Vec<Piece>,
    positive: Vec<Piece>,
    negative: Vec<Piece>,
}

/// ICU4X data per context, loaded on first use.
#[derive(Default)]
pub(crate) struct Cache {
    formats: RefCell<HashMap<(String, String, Style), Rc<LocaleFormat>>>,
    compact: RefCell<HashMap<(String, bool), Rc<Compact>>>,
}

/// A locale's compact patterns (short or long) and the plural rules that
/// pick among them.
pub(crate) struct Compact {
    patterns: DataPayload<DecimalCompactShortV1>,
    rules: PluralRules,
}

impl Compact {
    /// ComputeExponentForMagnitude for compact notation: the exponent of the
    /// last pattern at or below the magnitude, 0 below the first.
    fn exponent_for_magnitude(&self, magnitude: i16) -> i16 {
        self.patterns
            .get()
            .0
            .iter()
            .filter(|t| magnitude >= i16::from(t.sized))
            .last()
            .map_or(0, |t| {
                i16::from(t.sized) - i16::from(t.variable.get_default().0.get())
            })
    }

    /// The text around the number in the pattern for a rounded significand
    /// shown at `exponent`.
    fn affixes(&self, significand: &UnsignedDecimal, exponent: u8) -> (String, String) {
        let magnitude = if significand.is_zero() {
            0
        } else {
            significand.nonzero_magnitude_start()
        } + i16::from(exponent);
        let operands = PluralOperands::from_significand_and_exponent(significand, exponent);
        let mut prefix = String::new();
        let mut suffix = String::new();
        if let Some(t) = self
            .patterns
            .get()
            .0
            .iter()
            .filter(|t| magnitude >= i16::from(t.sized))
            .last()
        {
            let mut after = false;
            for item in t.variable.get(operands, &self.rules).1.iter() {
                match item {
                    PatternItem::Literal(text) if after => suffix.push_str(text),
                    PatternItem::Literal(text) => prefix.push_str(text),
                    PatternItem::Placeholder(_) => after = true,
                }
            }
        }
        (prefix, suffix)
    }
}

impl Cache {
    pub(crate) fn get(
        &self,
        data: &IntlData,
        locale: &str,
        numbering_system: &str,
        style: Style,
    ) -> Result<Rc<LocaleFormat>, String> {
        let key = (locale.to_string(), numbering_system.to_string(), style);
        if let Some(found) = self.formats.borrow().get(&key) {
            return Ok(found.clone());
        }
        let built = Rc::new(load(data, locale, numbering_system, style)?);
        self.formats.borrow_mut().insert(key, built.clone());
        Ok(built)
    }

    pub(crate) fn compact(
        &self,
        data: &IntlData,
        locale: &str,
        long: bool,
    ) -> Result<Rc<Compact>, String> {
        let key = (locale.to_string(), long);
        if let Some(found) = self.compact.borrow().get(&key) {
            return Ok(found.clone());
        }
        let parsed = locale
            .parse::<icu::locale::Locale>()
            .map_err(|_| format!("{locale} is not a locale"))?;
        let data_locale = DataLocale::from(&parsed);
        let provider = data.provider();
        let missing = |_| {
            format!(
                "the data blob has no {} compact patterns for {locale} or its fallbacks",
                if long { "long" } else { "short" }
            )
        };
        let patterns = if long {
            let response: DataResponse<DecimalCompactLongV1> = provider
                .load(request(&data_locale, Default::default()))
                .map_err(missing)?;
            response.payload.cast()
        } else {
            let response: DataResponse<DecimalCompactShortV1> = provider
                .load(request(&data_locale, Default::default()))
                .map_err(missing)?;
            response.payload
        };
        let rules = PluralRules::try_new_cardinal_unstable(&provider, (&parsed).into())
            .map_err(|_| format!("the data blob has no plural rules for {locale}"))?;
        let built = Rc::new(Compact { patterns, rules });
        self.compact.borrow_mut().insert(key, built.clone());
        Ok(built)
    }
}

fn request<'a>(locale: &'a DataLocale, attributes: &'a DataMarkerAttributes) -> DataRequest<'a> {
    DataRequest {
        id: DataIdentifierBorrowed::for_marker_attributes_and_locale(attributes, locale),
        ..Default::default()
    }
}

fn attributes(numbering_system: &str) -> Option<&DataMarkerAttributes> {
    DataMarkerAttributes::try_from_str(numbering_system).ok()
}

/// The locale's own numbering system ([[LocaleData]]'s first `nu` value).
pub(crate) fn default_numbering_system(data: &IntlData, locale: &str) -> Option<String> {
    let locale = DataLocale::from(&locale.parse::<icu::locale::Locale>().ok()?);
    let response: DataResponse<DecimalSymbolsV1> = data
        .provider()
        .load(request(&locale, Default::default()))
        .ok()?;
    Some(response.payload.get().strings.numsys().to_string())
}

/// The `nu` values a locale supports: its own first, then every numbering
/// system with a simple digit mapping.
pub(crate) fn numbering_systems(data: &IntlData, locale: &str) -> Vec<String> {
    let mut list: Vec<String> = default_numbering_system(data, locale).into_iter().collect();
    for (name, _) in SIMPLE_DIGITS {
        if list.first().is_none_or(|first| first != name) {
            list.push(name.to_string());
        }
    }
    list
}

fn load(
    data: &IntlData,
    locale: &str,
    numbering_system: &str,
    style: Style,
) -> Result<LocaleFormat, String> {
    let parsed = locale
        .parse::<icu::locale::Locale>()
        .map_err(|_| format!("{locale} is not a locale"))?;
    let data_locale = DataLocale::from(&parsed);
    let provider = data.provider();
    // Symbols for the numbering system, else the locale's own.
    let by_system: Option<DataResponse<DecimalSymbolsV1>> =
        attributes(numbering_system).and_then(|a| provider.load(request(&data_locale, a)).ok());
    let own: DataResponse<DecimalSymbolsV1> = provider
        .load(request(&data_locale, Default::default()))
        .map_err(|_| {
            format!("the data blob has no number symbols for {locale} or its fallbacks")
        })?;
    let locale_system = own.payload.get().strings.numsys().to_string();
    let has_own = by_system.is_some() || locale_system == numbering_system;
    let symbols = by_system.unwrap_or(own);
    let symbols = symbols.payload.get();
    let strings = &symbols.strings;
    // A system the locale has no symbols for uses root's for it where root
    // has its own (arab, arabext), as CLDR's alias to the locale's latn
    // symbols applies only to the others.
    let root = (!has_own)
        .then(|| ROOT_SYMBOLS.iter().find(|(s, _)| *s == numbering_system))
        .flatten()
        .map(|(_, symbols)| symbols);
    let pick =
        |own: &str, field: fn(&RootSymbols) -> &'static str| root.map_or(own, field).to_string();
    let decimal = pick(strings.decimal_separator(), |r| r.decimal);
    let group = pick(strings.grouping_separator(), |r| r.group);
    let (minus_prefix, minus_suffix) = match root {
        Some(r) => (r.minus_sign, ""),
        None => (strings.minus_sign_prefix(), strings.minus_sign_suffix()),
    };
    let (plus_prefix, plus_suffix) = match root {
        Some(r) => (r.plus_sign, ""),
        None => (strings.plus_sign_prefix(), strings.plus_sign_suffix()),
    };
    let digits = match SIMPLE_DIGITS.binary_search_by(|(n, _)| (*n).cmp(numbering_system)) {
        Ok(at) => SIMPLE_DIGITS[at].1,
        Err(_) => {
            let id = attributes(numbering_system).unwrap_or_default();
            let response: DataResponse<DecimalDigitsV1> = provider
                .load(DataRequest {
                    id: DataIdentifierBorrowed::for_marker_attributes(id),
                    ..Default::default()
                })
                .map_err(|_| format!("the data blob has no digits for {numbering_system}"))?;
            Digits::List(*response.payload.get())
        }
    };
    let cldr = cldr_values(data, &parsed, numbering_system);
    let minus = format!("{minus_prefix}{minus_suffix}");
    let signed = |prefix: &str, suffix: &str, sign: fn(String) -> Piece| {
        let mut pieces = Vec::new();
        if !prefix.is_empty() {
            pieces.push(sign(prefix.to_string()));
        }
        pieces.push(Piece::Number);
        if !suffix.is_empty() {
            pieces.push(sign(suffix.to_string()));
        }
        pieces
    };
    let (zero, positive, negative) = match style {
        Style::Decimal => (
            vec![Piece::Number],
            signed(plus_prefix, plus_suffix, Piece::Plus),
            signed(minus_prefix, minus_suffix, Piece::Minus),
        ),
        Style::Percent => {
            let response: DataResponse<DecimalPercentV1> = provider
                .load(request(&data_locale, Default::default()))
                .map_err(|_| {
                    format!("the data blob has no percent patterns for {locale} or its fallbacks")
                })?;
            let percent = response.payload.get();
            // ICU4X makes the percent data from the locale's latn symbols
            // whatever its own system; the pattern's percent sign becomes the
            // requested system's.
            let in_pattern = cldr_values(data, &parsed, "latn").percent_sign;
            let percent_sign = cldr.percent_sign;
            let literal = |text: &str, out: &mut Vec<Piece>| {
                let mut rest = text;
                while let Some(at) = rest.find(in_pattern).filter(|_| !in_pattern.is_empty()) {
                    if at > 0 {
                        out.push(Piece::Literal(rest[..at].to_string()));
                    }
                    out.push(Piece::PercentSign(percent_sign.to_string()));
                    rest = &rest[at + in_pattern.len()..];
                }
                if !rest.is_empty() {
                    out.push(Piece::Literal(rest.to_string()));
                }
            };
            // Its signs are latn's too; another system takes the signs
            // resolved above (ar-EG's arab minus is not its latn one).
            let sign = |own: &str, prefix: &str, suffix: &str| {
                if numbering_system == "latn" {
                    own.to_string()
                } else {
                    format!("{prefix}{suffix}")
                }
            };
            let mut zero = Vec::new();
            for item in percent.unsigned_pattern.iter() {
                match item {
                    PatternItem::Literal(text) => literal(text, &mut zero),
                    PatternItem::Placeholder(_) => zero.push(Piece::Number),
                }
            }
            let with_sign = |sign: Piece| {
                let mut out = Vec::new();
                for item in percent.signed_pattern.iter() {
                    match item {
                        PatternItem::Literal(text) => literal(text, &mut out),
                        PatternItem::Placeholder(DoublePlaceholderKey::Place0) => {
                            out.push(Piece::Number)
                        }
                        PatternItem::Placeholder(DoublePlaceholderKey::Place1) => {
                            out.push(sign.clone())
                        }
                    }
                }
                out
            };
            (
                zero,
                with_sign(Piece::Plus(sign(
                    &percent.plus_sign,
                    plus_prefix,
                    plus_suffix,
                ))),
                with_sign(Piece::Minus(sign(
                    &percent.minus_sign,
                    minus_prefix,
                    minus_suffix,
                ))),
            )
        }
    };
    Ok(LocaleFormat {
        decimal,
        group,
        primary: symbols.grouping_sizes.primary,
        secondary: symbols.grouping_sizes.secondary,
        min_grouping: symbols.grouping_sizes.min_grouping,
        digits,
        cldr,
        minus,
        zero,
        positive,
        negative,
    })
}

/// The CLDR values for the locale (through its fallbacks to root) in the
/// numbering system, else in `latn`.
fn cldr_values(data: &IntlData, locale: &icu::locale::Locale, system: &str) -> &'static Values {
    let mut chain = data.fallback_chain(locale);
    chain.push("und".to_string());
    for system in [system, "latn"] {
        for candidate in &chain {
            let key = format!("{candidate}/{system}");
            if let Ok(at) = ENTRIES.binary_search_by(|(k, _)| (*k).cmp(key.as_str())) {
                return &VALUES[usize::from(ENTRIES[at].1)];
            }
        }
    }
    let root = ENTRIES
        .binary_search_by(|(k, _)| (*k).cmp("und/latn"))
        .map_or(0, |at| usize::from(ENTRIES[at].1));
    &VALUES[root]
}

/// The sign category GetNumberFormatPattern decides on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    PlusZero,
    MinusZero,
    Positive,
    Negative,
}

/// ComputeExponentForMagnitude.
fn exponent_for_magnitude(notation: Notation, compact: Option<&Compact>, magnitude: i16) -> i16 {
    match notation {
        Notation::Standard => 0,
        Notation::Scientific => magnitude,
        Notation::Engineering => magnitude.div_euclid(3) * 3,
        Notation::CompactShort | Notation::CompactLong => {
            compact.map_or(0, |c| c.exponent_for_magnitude(magnitude))
        }
    }
}

/// ComputeExponent, then FormatNumericToString of the value scaled by it.
fn scale_and_round(
    options: &Options,
    compact: Option<&Compact>,
    negative: bool,
    x: &UnsignedDecimal,
) -> (i16, Rounded) {
    let scaled = |exponent: i16| {
        let mut value = x.clone();
        value.multiply_pow10(-exponent);
        number::format_numeric_to_string(&options.digits, negative, &value)
    };
    if x.is_zero() {
        return (0, scaled(0));
    }
    let magnitude = x.nonzero_magnitude_start();
    let exponent = exponent_for_magnitude(options.notation, compact, magnitude);
    let rounded = scaled(exponent);
    if rounded.value.is_zero() || rounded.value.nonzero_magnitude_start() == magnitude - exponent {
        return (exponent, rounded);
    }
    // 999 rounding up to 1000 moves to the next magnitude's exponent.
    let exponent = exponent_for_magnitude(options.notation, compact, magnitude + 1);
    (exponent, scaled(exponent))
}

/// PartitionNumberPattern.
pub(crate) fn format_to_parts(
    format: &LocaleFormat,
    options: &Options,
    compact: Option<&Compact>,
    x: &IntlMV,
) -> Vec<Part> {
    // The number's own parts (PartitionNotationSubPattern), and the
    // category of the rounded value.
    let (number, category): (Vec<Part>, Category) = match x {
        // GetNumberFormatPattern: NaN is positive-zero, so "always" signs
        // it ("+NaN") and "exceptZero" does not.
        IntlMV::NotANumber => (
            vec![("nan", format.cldr.nan.to_string())],
            Category::PlusZero,
        ),
        IntlMV::PositiveInfinity => (
            vec![("infinity", format.cldr.infinity.to_string())],
            Category::Positive,
        ),
        IntlMV::NegativeInfinity => (
            vec![("infinity", format.cldr.infinity.to_string())],
            Category::Negative,
        ),
        IntlMV::NegativeZero => {
            let rounded = number::format_numeric_to_string(
                &options.digits,
                true,
                &UnsignedDecimal::default(),
            );
            (
                notation_parts(format, options, compact, &rounded, 0),
                Category::MinusZero,
            )
        }
        IntlMV::Finite { negative, value } => {
            let mut value = value.clone();
            if options.style == Style::Percent {
                // 0.25 keeps its integer zero, now at the hundreds.
                value.multiply_pow10(2);
                value.trim_start();
            }
            let (exponent, rounded) = scale_and_round(options, compact, *negative, &value);
            let category = match (*negative, rounded.value.is_zero()) {
                (false, true) => Category::PlusZero,
                (true, true) => Category::MinusZero,
                (false, false) => Category::Positive,
                (true, false) => Category::Negative,
            };
            (
                notation_parts(format, options, compact, &rounded, exponent),
                category,
            )
        }
    };
    use Category::*;
    let pattern = match (options.sign, category) {
        (SignDisplay::Never, _) => &format.zero,
        (SignDisplay::Auto, PlusZero | Positive) => &format.zero,
        (SignDisplay::Auto, _) => &format.negative,
        (SignDisplay::Always, PlusZero | Positive) => &format.positive,
        (SignDisplay::Always, _) => &format.negative,
        (SignDisplay::ExceptZero, PlusZero | MinusZero) => &format.zero,
        (SignDisplay::ExceptZero, Positive) => &format.positive,
        (SignDisplay::ExceptZero, Negative) => &format.negative,
        (SignDisplay::Negative, Negative) => &format.negative,
        (SignDisplay::Negative, _) => &format.zero,
    };
    let mut parts = Vec::new();
    for piece in pattern {
        match piece {
            Piece::Literal(text) => parts.push(("literal", text.clone())),
            Piece::Number => parts.extend(number.iter().cloned()),
            Piece::Plus(sign) => parts.push(("plusSign", sign.clone())),
            Piece::Minus(sign) => parts.push(("minusSign", sign.clone())),
            Piece::PercentSign(sign) => parts.push(("percentSign", sign.clone())),
        }
    }
    parts
}

/// PartitionNotationSubPattern for a finite rounded value shown at
/// `exponent`.
fn notation_parts(
    format: &LocaleFormat,
    options: &Options,
    compact: Option<&Compact>,
    rounded: &Rounded,
    exponent: i16,
) -> Vec<Part> {
    let digits = digits_parts(format, options, &rounded.string);
    match options.notation {
        Notation::Standard => digits,
        Notation::Scientific | Notation::Engineering => {
            let mut parts = digits;
            parts.push(("exponentSeparator", format.cldr.exponential.to_string()));
            if exponent < 0 {
                parts.push(("exponentMinusSign", format.minus.clone()));
            }
            parts.push((
                "exponentInteger",
                format.transliterate(&exponent.unsigned_abs().to_string()),
            ));
            parts
        }
        Notation::CompactShort | Notation::CompactLong => {
            let (Some(compact), Ok(shown @ 1..)) = (compact, u8::try_from(exponent)) else {
                return digits;
            };
            let (prefix, suffix) = compact.affixes(&rounded.value, shown);
            let mut parts = compact_affix(&prefix);
            parts.extend(digits);
            parts.extend(compact_affix(&suffix));
            parts
        }
    }
}

/// A compact pattern's text beside the number: its spacing is a literal,
/// the rest the compact symbol or name ("K", "thousand", "万").
fn compact_affix(text: &str) -> Vec<Part> {
    let core = text.trim_matches(char::is_whitespace);
    if core.is_empty() {
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![("literal", text.to_string())]
        };
    }
    let start = text.len() - text.trim_start_matches(char::is_whitespace).len();
    let end = start + core.len();
    let mut parts = Vec::new();
    if start > 0 {
        parts.push(("literal", text[..start].to_string()));
    }
    parts.push(("compact", core.to_string()));
    if end < text.len() {
        parts.push(("literal", text[end..].to_string()));
    }
    parts
}

/// The integer, group, decimal and fraction parts of FormatNumericToString's
/// unsigned ASCII string, in the numbering system's digits.
fn digits_parts(format: &LocaleFormat, options: &Options, string: &str) -> Vec<Part> {
    let transliterate = |s: &str| format.transliterate(s);
    let (int, fraction) = match string.split_once('.') {
        Some((int, fraction)) => (int, Some(fraction)),
        None => (string, None),
    };
    let mut parts = Vec::new();
    for (i, group) in groups(format, options.grouping, int)
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            parts.push(("group", format.group.clone()));
        }
        parts.push(("integer", transliterate(group)));
    }
    if let Some(fraction) = fraction {
        parts.push(("decimal", format.decimal.clone()));
        parts.push(("fraction", transliterate(fraction)));
    }
    parts
}

/// The integer digits split where the locale groups them. `auto` uses the
/// locale's minimum grouping digits, `min2` two, `always` one.
fn groups<'s>(format: &LocaleFormat, grouping: Grouping, int: &'s str) -> Vec<&'s str> {
    let primary = usize::from(format.primary);
    let secondary = match format.secondary {
        0 => primary,
        s => usize::from(s),
    };
    let min = match grouping {
        Grouping::Off => return vec![int],
        Grouping::Always => 1,
        Grouping::Min2 => 2,
        Grouping::Auto => usize::from(format.min_grouping.max(1)),
    };
    if primary == 0 || int.len() < primary + min {
        return vec![int];
    }
    let mut ends = vec![int.len()];
    let mut end = int.len() - primary;
    while end > 0 {
        ends.push(end);
        end = end.saturating_sub(secondary);
    }
    ends.push(0);
    ends.reverse();
    ends.windows(2).map(|w| &int[w[0]..w[1]]).collect()
}

/// ECMA-402's numbering systems with simple digit mappings, sorted.
pub(crate) static SIMPLE_DIGITS: &[(&str, Digits)] = &[
    ("adlm", Digits::Range('\u{1e950}')),
    ("ahom", Digits::Range('\u{11730}')),
    ("arab", Digits::Range('\u{660}')),
    ("arabext", Digits::Range('\u{6f0}')),
    ("bali", Digits::Range('\u{1b50}')),
    ("beng", Digits::Range('\u{9e6}')),
    ("bhks", Digits::Range('\u{11c50}')),
    ("brah", Digits::Range('\u{11066}')),
    ("cakm", Digits::Range('\u{11136}')),
    ("cham", Digits::Range('\u{aa50}')),
    ("deva", Digits::Range('\u{966}')),
    ("diak", Digits::Range('\u{11950}')),
    ("fullwide", Digits::Range('\u{ff10}')),
    ("gara", Digits::Range('\u{10d40}')),
    ("gong", Digits::Range('\u{11da0}')),
    ("gonm", Digits::Range('\u{11d50}')),
    ("gujr", Digits::Range('\u{ae6}')),
    ("gukh", Digits::Range('\u{16130}')),
    ("guru", Digits::Range('\u{a66}')),
    (
        "hanidec",
        Digits::List([
            '\u{3007}', '\u{4e00}', '\u{4e8c}', '\u{4e09}', '\u{56db}', '\u{4e94}', '\u{516d}',
            '\u{4e03}', '\u{516b}', '\u{4e5d}',
        ]),
    ),
    ("hmng", Digits::Range('\u{16b50}')),
    ("hmnp", Digits::Range('\u{1e140}')),
    ("java", Digits::Range('\u{a9d0}')),
    ("kali", Digits::Range('\u{a900}')),
    ("kawi", Digits::Range('\u{11f50}')),
    ("khmr", Digits::Range('\u{17e0}')),
    ("knda", Digits::Range('\u{ce6}')),
    ("krai", Digits::Range('\u{16d70}')),
    ("lana", Digits::Range('\u{1a80}')),
    ("lanatham", Digits::Range('\u{1a90}')),
    ("laoo", Digits::Range('\u{ed0}')),
    ("latn", Digits::Range('\u{30}')),
    ("lepc", Digits::Range('\u{1c40}')),
    ("limb", Digits::Range('\u{1946}')),
    ("mathbold", Digits::Range('\u{1d7ce}')),
    ("mathdbl", Digits::Range('\u{1d7d8}')),
    ("mathmono", Digits::Range('\u{1d7f6}')),
    ("mathsanb", Digits::Range('\u{1d7ec}')),
    ("mathsans", Digits::Range('\u{1d7e2}')),
    ("mlym", Digits::Range('\u{d66}')),
    ("modi", Digits::Range('\u{11650}')),
    ("mong", Digits::Range('\u{1810}')),
    ("mroo", Digits::Range('\u{16a60}')),
    ("mtei", Digits::Range('\u{abf0}')),
    ("mymr", Digits::Range('\u{1040}')),
    ("mymrepka", Digits::Range('\u{116da}')),
    ("mymrpao", Digits::Range('\u{116d0}')),
    ("mymrshan", Digits::Range('\u{1090}')),
    ("mymrtlng", Digits::Range('\u{a9f0}')),
    ("nagm", Digits::Range('\u{1e4f0}')),
    ("newa", Digits::Range('\u{11450}')),
    ("nkoo", Digits::Range('\u{7c0}')),
    ("olck", Digits::Range('\u{1c50}')),
    ("onao", Digits::Range('\u{1e5f1}')),
    ("orya", Digits::Range('\u{b66}')),
    ("osma", Digits::Range('\u{104a0}')),
    ("outlined", Digits::Range('\u{1ccf0}')),
    ("rohg", Digits::Range('\u{10d30}')),
    ("saur", Digits::Range('\u{a8d0}')),
    ("segment", Digits::Range('\u{1fbf0}')),
    ("shrd", Digits::Range('\u{111d0}')),
    ("sind", Digits::Range('\u{112f0}')),
    ("sinh", Digits::Range('\u{de6}')),
    ("sora", Digits::Range('\u{110f0}')),
    ("sund", Digits::Range('\u{1bb0}')),
    ("sunu", Digits::Range('\u{11bf0}')),
    ("takr", Digits::Range('\u{116c0}')),
    ("talu", Digits::Range('\u{19d0}')),
    ("tamldec", Digits::Range('\u{be6}')),
    ("telu", Digits::Range('\u{c66}')),
    ("thai", Digits::Range('\u{e50}')),
    ("tibt", Digits::Range('\u{f20}')),
    ("tirh", Digits::Range('\u{114d0}')),
    ("tnsa", Digits::Range('\u{16ac0}')),
    ("tols", Digits::Range('\u{11de0}')),
    ("vaii", Digits::Range('\u{a620}')),
    ("wara", Digits::Range('\u{118e0}')),
    ("wcho", Digits::Range('\u{1e2f0}')),
];

#[cfg(test)]
mod tests {
    use super::*;

    // Both are binary-searched.
    #[test]
    fn lookup_tables_are_sorted_without_duplicates() {
        assert!(SIMPLE_DIGITS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(ENTRIES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(
            ENTRIES
                .iter()
                .all(|(_, at)| usize::from(*at) < VALUES.len())
        );
    }

    fn parts_of(
        locale: &str,
        system: &str,
        style: Style,
        sign: SignDisplay,
        value: &str,
    ) -> String {
        static BLOB: &[u8] = include_bytes!("../tests/data/intl-test.postcard");
        let data = IntlData::try_from_static_blob(BLOB).expect("test blob");
        let format = Cache::default()
            .get(&data, locale, system, style)
            .expect("format data");
        let options = Options {
            locale: locale.to_string(),
            numbering_system: system.to_string(),
            style,
            notation: Notation::Standard,
            digits: DigitOptions {
                min_integer: 1,
                min_fraction: 0,
                max_fraction: 0,
                min_significant: 1,
                max_significant: 21,
                increment: 1,
                rounding_type: number::RoundingType::FractionDigits,
                rounding_mode: number::RoundingMode::HalfExpand,
                strip_if_integer: false,
            },
            grouping: Grouping::Auto,
            sign,
        };
        format_to_parts(&format, &options, None, &number::to_intl_mv(value))
            .iter()
            .map(|(kind, text)| format!("{kind}:{}", text.escape_default()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn percent_signs_follow_the_requested_numbering_system() {
        let latn = |sign, value| parts_of("ar-EG", "latn", Style::Percent, sign, value);
        let decimal = |sign, value| parts_of("ar-EG", "latn", Style::Decimal, sign, value);
        // ar-EG's own system is arab; with latn the percent signs are the
        // decimal style's latn signs.
        let sign_of = |parts: String, kind: &str| {
            parts
                .split(' ')
                .find(|p| p.starts_with(kind))
                .map(str::to_string)
        };
        assert_eq!(
            sign_of(latn(SignDisplay::Negative, "-0.25"), "minusSign:"),
            sign_of(decimal(SignDisplay::Negative, "-1"), "minusSign:"),
        );
        assert_eq!(
            sign_of(latn(SignDisplay::Always, "0.25"), "plusSign:"),
            sign_of(decimal(SignDisplay::Always, "1"), "plusSign:"),
        );
        // ar-EG's own arab system takes arab's signs and percent sign, not
        // the latn ones the percent data holds.
        let own = parts_of("ar-EG", "arab", Style::Percent, SignDisplay::Auto, "-0.25");
        assert!(own.starts_with(r"minusSign:\u{61c}-"), "{own}");
        assert!(own.ends_with(r"percentSign:\u{66a}\u{61c}"), "{own}");
        // en with arab takes root's arab signs in both styles.
        let arab = parts_of("en", "arab", Style::Percent, SignDisplay::Auto, "-0.25");
        assert!(arab.starts_with(r"minusSign:\u{61c}-"), "{arab}");
    }

    #[test]
    fn compact_affixes_split_spacing_from_the_symbol() {
        assert_eq!(compact_affix(""), vec![]);
        assert_eq!(compact_affix("K"), vec![("compact", String::from("K"))]);
        assert_eq!(
            compact_affix("\u{a0}Tsd."),
            vec![
                ("literal", String::from("\u{a0}")),
                ("compact", String::from("Tsd."))
            ]
        );
        assert_eq!(
            compact_affix(" thousand"),
            vec![
                ("literal", String::from(" ")),
                ("compact", String::from("thousand"))
            ]
        );
    }

    #[test]
    fn nan_takes_the_positive_zero_pattern() {
        let nan = |sign| parts_of("en", "latn", Style::Decimal, sign, "NaN");
        assert_eq!(nan(SignDisplay::Auto), "nan:NaN");
        assert_eq!(nan(SignDisplay::Never), "nan:NaN");
        assert_eq!(nan(SignDisplay::Always), "plusSign:+ nan:NaN");
        assert_eq!(nan(SignDisplay::ExceptZero), "nan:NaN");
        assert_eq!(nan(SignDisplay::Negative), "nan:NaN");
    }
}
