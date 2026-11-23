//! Intl.NumberFormat's PartitionNumberPattern, written over ICU4X's data
//! rather than its `DecimalFormatter`: grouping honours `useGrouping:
//! "always"`, the notations follow ComputeExponent, and every part keeps
//! the type ECMA-402 gives it.

use crate::cldr_numbers::{ENTRIES, ROOT_SYMBOLS, RootSymbols, VALUES, Values};
use crate::cldr_units;
use crate::data::IntlData;
use crate::number::{self, DigitOptions, IntlMV, Rounded};
pub(crate) use crate::plural::Notation;
use fixed_decimal::UnsignedDecimal;
use icu::decimal::provider::{
    DecimalCompactLongV1, DecimalCompactShortV1, DecimalDigitsV1, DecimalSymbolsV1,
};
use icu::plurals::{PluralCategory, PluralOperands, PluralRules};
use icu_experimental::dimension::provider::currency::essentials::CurrencyEssentialsV1;
use icu_experimental::dimension::provider::currency::extended::CurrencyExtendedDataV1;
use icu_experimental::dimension::provider::currency::patterns::CurrencyPatternsDataV1;
use icu_experimental::dimension::provider::currency::symbols::CurrencySymbolsV1;
use icu_experimental::dimension::provider::percent::DecimalPercentV1;
use icu_pattern::DoublePlaceholderPattern;
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

/// [[CurrencyDisplay]].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CurrencyDisplay {
    Code,
    Symbol,
    NarrowSymbol,
    Name,
}

/// [[UnitDisplay]].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum UnitDisplay {
    Long,
    Short,
    Narrow,
}

/// What the currency and unit styles put around the number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Measure {
    Currency {
        code: String,
        display: CurrencyDisplay,
        accounting: bool,
    },
    Unit {
        unit: String,
        display: UnitDisplay,
    },
}

/// One Intl.NumberFormat's resolved slots, as Rust reads them.
pub(crate) struct Options {
    pub locale: String,
    pub numbering_system: String,
    /// The number's own style: currency and unit are decimal with a
    /// measure around it.
    pub style: Style,
    pub measure: Option<Measure>,
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
pub(crate) enum Piece {
    Literal(String),
    Number,
    Plus(String),
    Minus(String),
    PercentSign(String),
    Currency(String),
    Unit(String),
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
    plus: String,
    zero: Vec<Piece>,
    positive: Vec<Piece>,
    negative: Vec<Piece>,
}

/// ICU4X data per context, loaded on first use.
#[derive(Default)]
pub(crate) struct Cache {
    formats: RefCell<HashMap<(String, String, Style), Rc<LocaleFormat>>>,
    compact: RefCell<HashMap<(String, bool), Rc<Compact>>>,
    measures: RefCell<HashMap<(String, String, Measure), Rc<Affixes>>>,
}

/// A currency's or unit's text around the number.
pub(crate) enum Affixes {
    /// Patterns by sign, the currency in place (code, symbol).
    Signed {
        zero: Vec<Piece>,
        positive: Vec<Piece>,
        negative: Vec<Piece>,
    },
    /// The signed number inside a pattern its plural category picks
    /// (currency names, units).
    Plural {
        rules: PluralRules,
        patterns: PluralPatterns,
    },
}

/// The plural-dependent patterns of a measure.
pub(crate) enum PluralPatterns {
    CurrencyName {
        patterns: DataPayload<CurrencyPatternsDataV1>,
        names: Option<DataPayload<CurrencyExtendedDataV1>>,
        code: String,
    },
    /// A unit CLDR names, or a numerator over a denominator.
    Unit(&'static [u16; 7]),
    PerUnit {
        numerator: &'static [u16; 7],
        denominator: &'static [u16; 7],
        compound: u16,
    },
}

/// The index of a plural category among CLDR's counts.
fn count_index(category: PluralCategory) -> usize {
    match category {
        PluralCategory::Zero => 0,
        PluralCategory::One => 1,
        PluralCategory::Two => 2,
        PluralCategory::Few => 3,
        PluralCategory::Many => 4,
        PluralCategory::Other => 5,
    }
}

/// String `at` of the unit table.
fn unit_text(at: u16) -> Option<&'static str> {
    (at != cldr_units::NONE).then(|| {
        let at = usize::from(at);
        &cldr_units::TEXT[cldr_units::OFFSETS[at] as usize..cldr_units::OFFSETS[at + 1] as usize]
    })
}

/// A unit's pattern for a plural category, else its `other` one.
fn unit_pattern(entry: &[u16; 7], category: PluralCategory) -> &'static str {
    unit_text(entry[count_index(category)])
        .or_else(|| unit_text(entry[5]))
        .unwrap_or("{0}")
}

impl Affixes {
    /// The pattern around the signed number for a value's plural operands
    /// (`None`: not finite, CLDR's `other`).
    fn plural_pieces(&self, operands: Option<PluralOperands>) -> Option<Vec<Piece>> {
        let Affixes::Plural { rules, patterns } = self else {
            return None;
        };
        let category = operands.map_or(PluralCategory::Other, |o| rules.category_for(o));
        let unit = |text: &str| -> Vec<Piece> {
            let (before, after) = text.split_once("{0}").unwrap_or((text, ""));
            let mut pieces = unit_pieces(before);
            pieces.push(Piece::Number);
            pieces.extend(unit_pieces(after));
            pieces
        };
        Some(match patterns {
            PluralPatterns::CurrencyName {
                patterns,
                names,
                code,
            } => {
                let pattern = match operands {
                    Some(o) => patterns.get().get(o, rules),
                    None => patterns.get().elements.get_default().1,
                };
                let name = names.as_ref().map_or(code.as_str(), |n| match operands {
                    Some(o) => n.get().get(o, rules),
                    None => n.get().elements.get_default().1,
                });
                pattern_pieces(pattern, name, None)
            }
            PluralPatterns::Unit(entry) => unit(unit_pattern(entry, category)),
            PluralPatterns::PerUnit {
                numerator,
                denominator,
                compound,
            } => {
                let numerator = unit_pattern(numerator, category);
                let text = match unit_text(denominator[6]) {
                    Some(per) => per.replace("{0}", numerator),
                    None => {
                        let name = unit_pattern(denominator, PluralCategory::One)
                            .replace("{0}", "")
                            .trim()
                            .to_string();
                        unit_text(*compound)
                            .unwrap_or("{0}/{1}")
                            .replace("{1}", &name)
                            .replace("{0}", numerator)
                    }
                };
                unit(&text)
            }
        })
    }
}

/// A currency pattern's pieces: `{0}` the number, `{1}` the currency; a
/// minus sign in its text (CLDR's `-`, or the locale's) becomes `minus`.
fn pattern_pieces(
    pattern: &DoublePlaceholderPattern,
    currency: &str,
    minus: Option<&str>,
) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for item in pattern.iter() {
        match item {
            PatternItem::Placeholder(DoublePlaceholderKey::Place0) => pieces.push(Piece::Number),
            PatternItem::Placeholder(DoublePlaceholderKey::Place1) => {
                pieces.push(Piece::Currency(currency.to_string()))
            }
            PatternItem::Literal(text) => {
                let Some(minus) = minus else {
                    pieces.push(Piece::Literal(text.to_string()));
                    continue;
                };
                let needle = if text.contains(minus) { minus } else { "-" };
                let mut rest = text;
                while let Some(at) = rest.find(needle) {
                    if at > 0 {
                        pieces.push(Piece::Literal(rest[..at].to_string()));
                    }
                    pieces.push(Piece::Minus(minus.to_string()));
                    rest = &rest[at + needle.len()..];
                }
                if !rest.is_empty() {
                    pieces.push(Piece::Literal(rest.to_string()));
                }
            }
        }
    }
    pieces
}

/// A unit pattern's text beside the number: its spacing is a literal, the
/// rest the unit.
fn unit_pieces(text: &str) -> Vec<Piece> {
    affix(text, "unit")
        .into_iter()
        .map(|(kind, t)| match kind {
            "literal" => Piece::Literal(t),
            _ => Piece::Unit(t),
        })
        .collect()
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

impl Cache {
    /// The currency's or unit's affixes, for a format of the locale and
    /// numbering system.
    pub(crate) fn measure(
        &self,
        data: &IntlData,
        format: &LocaleFormat,
        options: &Options,
        measure: &Measure,
    ) -> Result<Rc<Affixes>, String> {
        let key = (
            options.locale.clone(),
            options.numbering_system.clone(),
            measure.clone(),
        );
        if let Some(found) = self.measures.borrow().get(&key) {
            return Ok(found.clone());
        }
        let built = Rc::new(load_measure(
            data,
            format,
            &options.locale,
            &options.numbering_system,
            measure,
        )?);
        self.measures.borrow_mut().insert(key, built.clone());
        Ok(built)
    }
}

fn load_measure(
    data: &IntlData,
    format: &LocaleFormat,
    locale: &str,
    numbering_system: &str,
    measure: &Measure,
) -> Result<Affixes, String> {
    let parsed = locale
        .parse::<icu::locale::Locale>()
        .map_err(|_| format!("{locale} is not a locale"))?;
    let data_locale = DataLocale::from(&parsed);
    let provider = data.provider();
    let rules = || {
        PluralRules::try_new_cardinal_unstable(&provider, (&parsed).into())
            .map_err(|_| format!("the data blob has no plural rules for {locale}"))
    };
    match measure {
        Measure::Currency {
            code,
            display: CurrencyDisplay::Name,
            ..
        } => {
            let patterns: DataResponse<CurrencyPatternsDataV1> = provider
                .load(request(&data_locale, Default::default()))
                .map_err(|_| format!("the data blob has no currency name patterns for {locale}"))?;
            // A currency CLDR has no name for is written by its code.
            let names: Option<DataResponse<CurrencyExtendedDataV1>> =
                attributes(code).and_then(|a| provider.load(request(&data_locale, a)).ok());
            Ok(Affixes::Plural {
                rules: rules()?,
                patterns: PluralPatterns::CurrencyName {
                    patterns: patterns.payload,
                    names: names.map(|n| n.payload),
                    code: code.clone(),
                },
            })
        }
        Measure::Currency {
            code,
            display,
            accounting,
        } => {
            // The symbol, else (narrow) the symbol, else the code; a code
            // has letters at both ends.
            let symbol = |attribute: String| -> Option<(String, bool, bool)> {
                let a = attributes(&attribute)?;
                let response: DataResponse<CurrencySymbolsV1> =
                    provider.load(request(&data_locale, a)).ok()?;
                let symbol = response.payload.get();
                (!symbol.as_str().is_empty()).then(|| {
                    (
                        symbol.as_str().to_string(),
                        symbol.starts_with_letter(),
                        symbol.ends_with_letter(),
                    )
                })
            };
            let (text, starts, ends) = match display {
                CurrencyDisplay::NarrowSymbol => {
                    symbol(format!("{code}n")).or_else(|| symbol(code.clone()))
                }
                CurrencyDisplay::Symbol => symbol(code.clone()),
                _ => None,
            }
            .unwrap_or_else(|| (code.clone(), true, true));
            let by_system: Option<DataResponse<CurrencyEssentialsV1>> =
                attributes(numbering_system)
                    .and_then(|a| provider.load(request(&data_locale, a)).ok());
            let essentials = match by_system {
                Some(found) => found,
                None => provider
                    .load(request(&data_locale, Default::default()))
                    .map_err(|_| format!("the data blob has no currency patterns for {locale}"))?,
            };
            let essentials = essentials.payload.get();
            let (positive, negative) = if *accounting {
                (
                    essentials.get_positive_accounting(starts, ends),
                    essentials.get_negative_accounting(starts, ends),
                )
            } else {
                (
                    essentials.get_positive(starts, ends),
                    essentials.get_negative(starts, ends),
                )
            };
            let minus = Some(format.minus.as_str());
            let zero = pattern_pieces(positive, &text, minus);
            // Without a negative pattern the minus sign goes before the
            // whole (UTS 35).
            let negative = match negative {
                Some(pattern) => pattern_pieces(pattern, &text, minus),
                None => std::iter::once(Piece::Minus(format.minus.clone()))
                    .chain(zero.iter().cloned())
                    .collect(),
            };
            // A plus sign stands where the minus does.
            let positive = if negative.iter().any(|p| matches!(p, Piece::Minus(_))) {
                negative
                    .iter()
                    .map(|p| match p {
                        Piece::Minus(_) => Piece::Plus(format.plus.clone()),
                        other => other.clone(),
                    })
                    .collect()
            } else {
                std::iter::once(Piece::Plus(format.plus.clone()))
                    .chain(zero.iter().cloned())
                    .collect()
            };
            Ok(Affixes::Signed {
                zero,
                positive,
                negative,
            })
        }
        Measure::Unit { unit, display } => {
            let mut chain = data.fallback_chain(&parsed);
            chain.push(String::from("und"));
            let row = chain
                .iter()
                .find_map(|l| {
                    cldr_units::LOCALES
                        .binary_search_by(|(k, _)| (*k).cmp(l.as_str()))
                        .ok()
                })
                .map(|at| &cldr_units::ROWS[usize::from(cldr_units::LOCALES[at].1)])
                .ok_or_else(|| format!("no unit patterns for {locale}"))?;
            let width = match display {
                UnitDisplay::Long => 0,
                UnitDisplay::Short => 1,
                UnitDisplay::Narrow => 2,
            };
            let entry = |name: &str| {
                cldr_units::UNITS.binary_search(&name).ok().map(|at| {
                    &cldr_units::ENTRIES[usize::from(row[width * cldr_units::UNITS.len() + at])]
                })
            };
            let patterns = match entry(unit) {
                Some(found) => PluralPatterns::Unit(found),
                None => {
                    let (numerator, denominator) = unit
                        .split_once("-per-")
                        .and_then(|(n, d)| Some((entry(n)?, entry(d)?)))
                        .ok_or_else(|| format!("{unit} is not a sanctioned unit"))?;
                    PluralPatterns::PerUnit {
                        numerator,
                        denominator,
                        compound: row[3 * cldr_units::UNITS.len() + width],
                    }
                }
            };
            Ok(Affixes::Plural {
                rules: rules()?,
                patterns,
            })
        }
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
        plus: format!("{plus_prefix}{plus_suffix}"),
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
    measure: Option<&Affixes>,
    x: &IntlMV,
) -> Vec<Part> {
    partition(format, options, compact, measure, x).0
}

/// A range part: ECMA-402's [[Type]], [[Value]] and [[Source]].
pub(crate) type RangePart = (&'static str, String, &'static str);

/// PartitionNumberRangePattern; NaN ends are the caller's RangeError.
pub(crate) fn format_range_to_parts(
    format: &LocaleFormat,
    options: &Options,
    compact: Option<&Compact>,
    measure: Option<&Affixes>,
    x: &IntlMV,
    y: &IntlMV,
) -> Vec<RangePart> {
    let (start, start_core) = partition(format, options, compact, measure, x);
    let (end, end_core) = partition(format, options, compact, measure, y);
    let text = |parts: &[Part]| parts.iter().map(|(_, t)| t.as_str()).collect::<String>();
    let tagged = |parts: &[Part], source| {
        parts
            .iter()
            .map(|(kind, value)| (*kind, value.clone(), source))
            .collect::<Vec<RangePart>>()
    };
    if text(&start) == text(&end) {
        // FormatApproximately: CLDR's approximately pattern around the
        // whole, its sign symbol a part of its own.
        let (before, after) = format
            .cldr
            .approximately
            .split_once("{0}")
            .unwrap_or(("~", ""));
        let mut out = approximately(before, format.cldr.approximately_sign);
        out.extend(tagged(&start, "shared"));
        out.extend(approximately(after, format.cldr.approximately_sign));
        return out;
    }
    // CollapseNumberRange, as ICU's default: the text around the numbers
    // (sign, currency, unit, percent) is written once when both ends share
    // it and it is longer than one code point.
    let (start_before, start_number, start_after) = (
        &start[..start_core.0],
        &start[start_core.0..start_core.1],
        &start[start_core.1..],
    );
    let (end_before, end_number, end_after) = (
        &end[..end_core.0],
        &end[end_core.0..end_core.1],
        &end[end_core.1..],
    );
    let affix_length = text(start_before).chars().count() + text(start_after).chars().count();
    let collapse = start_before == end_before && start_after == end_after && affix_length > 1;
    // A repeated affix, or a compact symbol inside the number, puts spaces
    // around the separator.
    let spaced =
        (!collapse && affix_length > 0) || start_number.iter().any(|(kind, _)| *kind == "compact");
    let (before, rest) = format
        .cldr
        .range
        .split_once("{0}")
        .unwrap_or(("", "{0}–{1}"));
    let (between, after) = rest.split_once("{1}").unwrap_or(("–", ""));
    let mut separator = between.to_string();
    if spaced {
        if !separator.starts_with(char::is_whitespace) {
            separator.insert(0, ' ');
        }
        if !separator.ends_with(char::is_whitespace) {
            separator.push(' ');
        }
    }
    let shared_literal =
        |text: &str| (!text.is_empty()).then(|| ("literal", text.to_string(), "shared"));
    let mut out: Vec<RangePart> = shared_literal(before).into_iter().collect();
    if collapse {
        out.extend(tagged(start_before, "shared"));
        out.extend(tagged(start_number, "startRange"));
        out.extend(shared_literal(&separator));
        out.extend(tagged(end_number, "endRange"));
        out.extend(tagged(start_after, "shared"));
    } else {
        out.extend(tagged(&start, "startRange"));
        out.extend(shared_literal(&separator));
        out.extend(tagged(&end, "endRange"));
    }
    out.extend(shared_literal(after));
    out
}

/// An approximately pattern's text: its `~` is the locale's sign.
fn approximately(text: &str, sign: &str) -> Vec<RangePart> {
    let mut out = Vec::new();
    for (i, piece) in text.split('~').enumerate() {
        if i > 0 {
            out.push(("approximatelySign", sign.to_string(), "shared"));
        }
        if !piece.is_empty() {
            out.push(("literal", piece.to_string(), "shared"));
        }
    }
    out
}

/// PartitionNumberPattern, with where the number's own parts lie.
fn partition(
    format: &LocaleFormat,
    options: &Options,
    compact: Option<&Compact>,
    measure: Option<&Affixes>,
    x: &IntlMV,
) -> (Vec<Part>, (usize, usize)) {
    // The plural operands of the value shown, for a measure's pattern.
    let mut operands = None;
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
            operands = Some(PluralOperands::from(&UnsignedDecimal::default()));
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
            operands = Some(match (compact, u8::try_from(exponent)) {
                (Some(_), Ok(shown)) => {
                    PluralOperands::from_significand_and_exponent(&rounded.value, shown)
                }
                _ => {
                    let mut shown = rounded.value.clone();
                    shown.multiply_pow10(exponent);
                    PluralOperands::from(&shown)
                }
            });
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
    // GetNumberFormatPattern: the zero, positive or negative pattern.
    #[derive(Clone, Copy)]
    enum Which {
        Zero,
        Positive,
        Negative,
    }
    let which = match (options.sign, category) {
        (SignDisplay::Never, _) => Which::Zero,
        (SignDisplay::Auto, PlusZero | Positive) => Which::Zero,
        (SignDisplay::Auto, _) => Which::Negative,
        (SignDisplay::Always, PlusZero | Positive) => Which::Positive,
        (SignDisplay::Always, _) => Which::Negative,
        (SignDisplay::ExceptZero, PlusZero | MinusZero) => Which::Zero,
        (SignDisplay::ExceptZero, Positive) => Which::Positive,
        (SignDisplay::ExceptZero, Negative) => Which::Negative,
        (SignDisplay::Negative, Negative) => Which::Negative,
        (SignDisplay::Negative, _) => Which::Zero,
    };
    let pick = |zero, positive, negative| match which {
        Which::Zero => zero,
        Which::Positive => positive,
        Which::Negative => negative,
    };
    let signed: &[Piece] = match measure {
        Some(Affixes::Signed {
            zero,
            positive,
            negative,
        }) => pick(zero, positive, negative),
        _ => pick(&format.zero, &format.positive, &format.negative),
    };
    // A plural measure's pattern holds the signed number.
    let outer = measure.and_then(|m| m.plural_pieces(operands));
    let mut parts = Vec::new();
    let mut core = (0, 0);
    let mut push = |piece: &Piece, parts: &mut Vec<Part>| {
        if let Piece::Number = piece {
            core.0 = parts.len();
            parts.extend(number.iter().cloned());
            core.1 = parts.len();
        } else {
            push_piece(piece, parts);
        }
    };
    for piece in outer.as_deref().unwrap_or(signed) {
        match piece {
            Piece::Number if outer.is_some() => {
                for inner in signed {
                    push(inner, &mut parts);
                }
            }
            _ => push(piece, &mut parts),
        }
    }
    (parts, core)
}

fn push_piece(piece: &Piece, parts: &mut Vec<Part>) {
    match piece {
        Piece::Literal(text) => parts.push(("literal", text.clone())),
        Piece::Number => {}
        Piece::Plus(sign) => parts.push(("plusSign", sign.clone())),
        Piece::Minus(sign) => parts.push(("minusSign", sign.clone())),
        Piece::PercentSign(sign) => parts.push(("percentSign", sign.clone())),
        Piece::Currency(text) => parts.push(("currency", text.clone())),
        Piece::Unit(text) => parts.push(("unit", text.clone())),
    }
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
            let mut parts = affix(&prefix, "compact");
            parts.extend(digits);
            parts.extend(affix(&suffix, "compact"));
            parts
        }
    }
}

/// A pattern's text beside the number: its spacing is a literal, the rest
/// a part of `kind` (a compact symbol or name: "K", "thousand", "万"; a
/// unit).
fn affix(text: &str, kind: &'static str) -> Vec<Part> {
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
    parts.push((kind, core.to_string()));
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
        parts_with(locale, system, style, None, sign, value)
            .iter()
            .map(|(kind, text)| format!("{kind}:{}", text.escape_default()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn text_of(locale: &str, measure: Measure, sign: SignDisplay, value: &str) -> String {
        parts_with(locale, "latn", Style::Decimal, Some(measure), sign, value)
            .iter()
            .map(|(_, text)| text.as_str())
            .collect()
    }

    fn parts_with(
        locale: &str,
        system: &str,
        style: Style,
        measure: Option<Measure>,
        sign: SignDisplay,
        value: &str,
    ) -> Vec<Part> {
        static BLOB: &[u8] = include_bytes!("../tests/data/intl-test.postcard");
        let data = IntlData::try_from_static_blob(BLOB).expect("test blob");
        let cache = Cache::default();
        let format = cache
            .get(&data, locale, system, style)
            .expect("format data");
        let options = Options {
            locale: locale.to_string(),
            numbering_system: system.to_string(),
            style,
            measure: measure.clone(),
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
        let affixes = measure.map(|m| {
            cache
                .measure(&data, &format, &options, &m)
                .expect("measure data")
        });
        format_to_parts(
            &format,
            &options,
            None,
            affixes.as_deref(),
            &number::to_intl_mv(value),
        )
    }

    #[test]
    fn unit_table_lookups_are_sorted() {
        assert!(cldr_units::UNITS.windows(2).all(|w| w[0] < w[1]));
        assert!(cldr_units::LOCALES.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(
            cldr_units::OFFSETS.last().map(|end| *end as usize),
            Some(cldr_units::TEXT.len())
        );
    }

    #[test]
    fn units_take_their_plural_pattern() {
        let unit = |unit: &str, display| Measure::Unit {
            unit: unit.to_string(),
            display,
        };
        let long = |value| {
            text_of(
                "en",
                unit("kilometer", UnitDisplay::Long),
                SignDisplay::Auto,
                value,
            )
        };
        assert_eq!(long("1"), "1 kilometer");
        assert_eq!(long("-2"), "-2 kilometers");
        let speed = text_of(
            "en",
            unit("kilometer-per-hour", UnitDisplay::Short),
            SignDisplay::Auto,
            "5",
        );
        assert_eq!(speed, "5 km/h");
        // No CLDR name: the denominator's per-unit pattern.
        let rate = text_of(
            "en",
            unit("byte-per-second", UnitDisplay::Short),
            SignDisplay::Auto,
            "5",
        );
        assert_eq!(rate, "5 byte/s");
        let percent = text_of(
            "en",
            unit("percent", UnitDisplay::Short),
            SignDisplay::Auto,
            "50",
        );
        assert_eq!(percent, "50%");
    }

    #[test]
    fn currencies_take_their_sign_patterns() {
        let usd = |display, accounting| Measure::Currency {
            code: String::from("USD"),
            display,
            accounting,
        };
        let symbol = |sign, value| text_of("en", usd(CurrencyDisplay::Symbol, false), sign, value);
        assert_eq!(symbol(SignDisplay::Auto, "-1"), "-$1");
        assert_eq!(symbol(SignDisplay::Always, "1"), "+$1");
        let accounting = text_of(
            "en",
            usd(CurrencyDisplay::Symbol, true),
            SignDisplay::Auto,
            "-1",
        );
        assert_eq!(accounting, "($1)");
        let name = |value| {
            text_of(
                "en",
                usd(CurrencyDisplay::Name, false),
                SignDisplay::Auto,
                value,
            )
        };
        assert_eq!(name("1"), "1 US dollar");
        assert_eq!(name("-2"), "-2 US dollars");
        let code = text_of(
            "en",
            usd(CurrencyDisplay::Code, false),
            SignDisplay::Auto,
            "1",
        );
        assert_eq!(code, "USD\u{a0}1");
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
        assert_eq!(affix("", "compact"), vec![]);
        assert_eq!(affix("K", "compact"), vec![("compact", String::from("K"))]);
        assert_eq!(
            affix("\u{a0}Tsd.", "compact"),
            vec![
                ("literal", String::from("\u{a0}")),
                ("compact", String::from("Tsd."))
            ]
        );
        assert_eq!(
            affix(" thousand", "compact"),
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
