//! Locale-independent number rounding: ToIntlMathematicalValue's string
//! step and FormatNumericToString with ToRawPrecision and ToRawFixed, on
//! exact decimals so that strings and BigInts keep every digit.

use fixed_decimal::{RoundingIncrement, UnsignedDecimal, UnsignedRoundingMode};

/// An Intl mathematical value. Finite values are kept as sign and magnitude;
/// a finite zero is never negative (that is `NegativeZero`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum IntlMV {
    NotANumber,
    PositiveInfinity,
    NegativeInfinity,
    NegativeZero,
    Finite {
        negative: bool,
        value: UnsignedDecimal,
    },
}

/// StrWhiteSpaceChar: WhiteSpace and LineTerminator.
fn is_str_white_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\u{b}'
            | '\u{c}'
            | ' '
            | '\u{a0}'
            | '\u{feff}'
            | '\n'
            | '\r'
            | '\u{2028}'
            | '\u{2029}'
    ) || (c > '\u{ff}' && c.is_whitespace() && c != '\u{85}' && c != '\u{180e}')
}

/// The StringIntlMV of a StringNumericLiteral, followed by RoundMVResult's
/// checks: what ToIntlMathematicalValue returns for a String (a Number or
/// BigInt arrives here as its decimal string, and -0 as `"-0"`).
pub(crate) fn to_intl_mv(input: &str) -> IntlMV {
    let text = input.trim_matches(is_str_white_space);
    if text.is_empty() {
        return finite(false, UnsignedDecimal::default());
    }
    if let Some(value) = non_decimal_integer(text) {
        return value;
    }
    let (negative, body) = match text.as_bytes()[0] {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    if body == "Infinity" {
        return if negative {
            IntlMV::NegativeInfinity
        } else {
            IntlMV::PositiveInfinity
        };
    }
    let Some((digits, exponent)) = decimal_literal(body) else {
        return IntlMV::NotANumber;
    };
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return if negative {
            IntlMV::NegativeZero
        } else {
            finite(false, UnsignedDecimal::default())
        };
    }
    // RoundMVResult: a value too large for a Number is an infinity, one too
    // small is a zero.
    let approx: f64 = format!("{digits}e{exponent}").parse().unwrap_or(f64::NAN);
    if approx.is_infinite() {
        return if negative {
            IntlMV::NegativeInfinity
        } else {
            IntlMV::PositiveInfinity
        };
    }
    if approx == 0.0 {
        return if negative {
            IntlMV::NegativeZero
        } else {
            finite(false, UnsignedDecimal::default())
        };
    }
    // Rounding never looks past the 121st significant digit (21 integer plus
    // 100 fraction), so longer inputs keep a nonzero tail as one sticky digit.
    const KEEP: usize = 200;
    let (digits, exponent) = if digits.len() > KEEP {
        let dropped = &digits[KEEP..];
        let sticky = if dropped.bytes().any(|b| b != b'0') {
            "1"
        } else {
            "0"
        };
        let shift = i64::try_from(dropped.len()).unwrap_or(i64::MAX) - 1;
        (
            format!("{}{sticky}", &digits[..KEEP]),
            exponent.saturating_add(shift),
        )
    } else {
        (digits.to_string(), exponent)
    };
    let mut value = UnsignedDecimal::try_from_str(&digits).unwrap_or_default();
    // A finite, nonzero Number has a magnitude well inside i16.
    value.multiply_pow10(i16::try_from(exponent).unwrap_or(0));
    finite(negative, value)
}

fn finite(negative: bool, value: UnsignedDecimal) -> IntlMV {
    IntlMV::Finite { negative, value }
}

/// StrDecimalLiteral without its sign or `Infinity`: the significant digits
/// and the power of ten they are scaled by.
fn decimal_literal(body: &str) -> Option<(String, i64)> {
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let (int, frac) = match mantissa.split_once('.') {
        Some((int, frac)) => (int, frac),
        None => (mantissa, ""),
    };
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (int.is_empty() && frac.is_empty()) || !all_digits(int) || !all_digits(frac) {
        return None;
    }
    let exponent = match exponent {
        None => 0,
        Some(e) => {
            let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
            if digits.is_empty() || !all_digits(digits) {
                return None;
            }
            // Saturate: past this the value is an infinity or a zero anyway.
            let magnitude = digits
                .trim_start_matches('0')
                .parse::<i64>()
                .unwrap_or(if digits.trim_start_matches('0').is_empty() {
                    0
                } else {
                    1 << 40
                })
                .min(1 << 40);
            if e.starts_with('-') {
                -magnitude
            } else {
                magnitude
            }
        }
    };
    let scale = i64::try_from(frac.len()).ok()?;
    Some((format!("{int}{frac}"), exponent - scale))
}

/// NonDecimalIntegerLiteral (`0x`, `0o`, `0b`), unsigned, exact.
fn non_decimal_integer(text: &str) -> Option<IntlMV> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'0' {
        return None;
    }
    let radix = match bytes[1] {
        b'x' | b'X' => 16,
        b'o' | b'O' => 8,
        b'b' | b'B' => 2,
        _ => return None,
    };
    let digits = &text[2..];
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return Some(IntlMV::NotANumber);
    }
    // Base conversion into little-endian decimal digits.
    let mut decimal: Vec<u8> = vec![0];
    for c in digits.chars() {
        let mut carry = c.to_digit(radix).unwrap_or(0);
        for d in decimal.iter_mut() {
            let v = u32::from(*d) * radix + carry;
            *d = (v % 10) as u8;
            carry = v / 10;
        }
        while carry > 0 {
            decimal.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    let text: String = decimal.iter().rev().map(|d| char::from(b'0' + d)).collect();
    Some(to_intl_mv(&text))
}

/// The spec's rounding modes, before GetUnsignedRoundingMode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoundingMode {
    Ceil,
    Floor,
    Expand,
    Trunc,
    HalfCeil,
    HalfFloor,
    HalfExpand,
    HalfTrunc,
    HalfEven,
}

impl RoundingMode {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "ceil" => Self::Ceil,
            "floor" => Self::Floor,
            "expand" => Self::Expand,
            "trunc" => Self::Trunc,
            "halfCeil" => Self::HalfCeil,
            "halfFloor" => Self::HalfFloor,
            "halfExpand" => Self::HalfExpand,
            "halfTrunc" => Self::HalfTrunc,
            "halfEven" => Self::HalfEven,
            _ => return None,
        })
    }

    /// GetUnsignedRoundingMode.
    fn unsigned(self, negative: bool) -> UnsignedRoundingMode {
        use UnsignedRoundingMode as U;
        match (self, negative) {
            (Self::Ceil, false) | (Self::Floor, true) | (Self::Expand, _) => U::Expand,
            (Self::Ceil, true) | (Self::Floor, false) | (Self::Trunc, _) => U::Trunc,
            (Self::HalfCeil, false) | (Self::HalfFloor, true) | (Self::HalfExpand, _) => {
                U::HalfExpand
            }
            (Self::HalfCeil, true) | (Self::HalfFloor, false) | (Self::HalfTrunc, _) => {
                U::HalfTrunc
            }
            (Self::HalfEven, _) => U::HalfEven,
        }
    }
}

/// [[RoundingType]].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoundingType {
    FractionDigits,
    SignificantDigits,
    MorePrecision,
    LessPrecision,
}

impl RoundingType {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "fractionDigits" => Self::FractionDigits,
            "significantDigits" => Self::SignificantDigits,
            "morePrecision" => Self::MorePrecision,
            "lessPrecision" => Self::LessPrecision,
            _ => return None,
        })
    }
}

/// The slots SetNumberFormatDigitOptions fills, already validated by it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DigitOptions {
    pub min_integer: u8,
    pub min_fraction: u8,
    pub max_fraction: u8,
    pub min_significant: u8,
    pub max_significant: u8,
    pub increment: u16,
    pub rounding_type: RoundingType,
    pub rounding_mode: RoundingMode,
    pub strip_if_integer: bool,
}

/// FormatNumericToString's result for the magnitude of a value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Rounded {
    /// The rounded magnitude, holding exactly the digits the string shows.
    pub value: UnsignedDecimal,
    pub string: String,
}

/// FormatNumericToString on the magnitude of a value whose sign is
/// `negative` (the sign only steers directed rounding modes).
pub(crate) fn format_numeric_to_string(
    options: &DigitOptions,
    negative: bool,
    x: &UnsignedDecimal,
) -> Rounded {
    let mode = options.rounding_mode.unsigned(negative);
    let precision = || to_raw_precision(x, options.min_significant, options.max_significant, mode);
    let fixed = || {
        to_raw_fixed(
            x,
            options.min_fraction,
            options.max_fraction,
            options.increment,
            mode,
        )
    };
    let (mut value, _) = match options.rounding_type {
        RoundingType::SignificantDigits => precision(),
        RoundingType::FractionDigits => fixed(),
        RoundingType::MorePrecision | RoundingType::LessPrecision => {
            let significant = precision();
            let fraction = fixed();
            let fixed_is_more_precise = fraction.1 < significant.1;
            let more = options.rounding_type == RoundingType::MorePrecision;
            if more == fixed_is_more_precise {
                fraction
            } else {
                significant
            }
        }
    };
    if options.strip_if_integer {
        value.trim_end_if_integer();
    }
    if options.min_integer > 1 {
        value.pad_start(i16::from(options.min_integer) - 1);
    }
    let string = value.to_string();
    Rounded { value, string }
}

fn magnitude(x: &UnsignedDecimal) -> i16 {
    if x.is_zero() {
        0
    } else {
        x.nonzero_magnitude_start()
    }
}

/// ToRawPrecision: the rounded value and its [[RoundingMagnitude]].
fn to_raw_precision(
    x: &UnsignedDecimal,
    min_precision: u8,
    max_precision: u8,
    mode: UnsignedRoundingMode,
) -> (UnsignedDecimal, i16) {
    let (min_p, max_p) = (i16::from(min_precision), i16::from(max_precision));
    let mut value = x.clone();
    value.round_with_mode(magnitude(x) - max_p + 1, mode);
    // Rounding up can carry into a new leading digit.
    let e = magnitude(&value);
    value.trim_end();
    let last = e - min_p + 1;
    if last < 0 {
        value.pad_end(last);
    }
    (value, e - max_p + 1)
}

/// ToRawFixed: the rounded value and its [[RoundingMagnitude]].
fn to_raw_fixed(
    x: &UnsignedDecimal,
    min_fraction: u8,
    max_fraction: u8,
    increment: u16,
    mode: UnsignedRoundingMode,
) -> (UnsignedDecimal, i16) {
    let (multiple, shift) = split_increment(increment);
    let mut value = x.clone();
    value.round_with_mode_and_increment(-i16::from(max_fraction) + shift, mode, multiple);
    value.trim_end();
    if min_fraction > 0 {
        value.pad_end(-i16::from(min_fraction));
    }
    (value, -i16::from(max_fraction))
}

/// A permitted roundingIncrement (1, 2, 5, 10, 20, 25, ... 5000) as a
/// multiple at a digit position: 250 is multiples of 25, one place up.
fn split_increment(increment: u16) -> (RoundingIncrement, i16) {
    let mut n = increment.max(1);
    let mut shift = 0;
    while n % 10 == 0 {
        n /= 10;
        shift += 1;
    }
    let multiple = match n {
        2 => RoundingIncrement::MultiplesOf2,
        5 => RoundingIncrement::MultiplesOf5,
        25 => RoundingIncrement::MultiplesOf25,
        _ => RoundingIncrement::MultiplesOf1,
    };
    (multiple, shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(s: &str) -> String {
        match to_intl_mv(s) {
            IntlMV::Finite { negative, value } => {
                format!("{}{}", if negative { "-" } else { "" }, value)
            }
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn strings_become_exact_values() {
        assert_eq!(mv(""), "0");
        assert_eq!(mv("  \n12.50\u{2028}"), "12.50");
        assert_eq!(mv("-0"), "NegativeZero");
        assert_eq!(mv("-0.000"), "NegativeZero");
        assert_eq!(mv("+.5"), "0.5");
        assert_eq!(mv("5."), "5");
        assert_eq!(mv("1e3"), "1000");
        assert_eq!(mv("1.5E-3"), "0.0015");
        assert_eq!(
            mv("123456789012345678901234567890"),
            "123456789012345678901234567890"
        );
        assert_eq!(mv("0x1F"), "31");
        assert_eq!(mv("0b101"), "5");
        assert_eq!(mv("0o17"), "15");
        assert_eq!(mv("-Infinity"), "NegativeInfinity");
        assert_eq!(mv("1e400"), "PositiveInfinity");
        assert_eq!(mv("-1e-400"), "NegativeZero");
        for bad in [
            "abc",
            "1_000",
            "-0x1",
            "0x",
            ".",
            "1e",
            "1e+",
            "Infinityx",
            "--1",
            "1.2.3",
        ] {
            assert_eq!(mv(bad), "NotANumber", "{bad}");
        }
    }

    fn options(rounding_type: RoundingType) -> DigitOptions {
        DigitOptions {
            min_integer: 1,
            min_fraction: 0,
            max_fraction: 3,
            min_significant: 1,
            max_significant: 21,
            increment: 1,
            rounding_type,
            rounding_mode: RoundingMode::HalfExpand,
            strip_if_integer: false,
        }
    }

    fn format(o: &DigitOptions, s: &str) -> String {
        match to_intl_mv(s) {
            IntlMV::Finite { negative, value } => {
                format_numeric_to_string(o, negative, &value).string
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fraction_digits() {
        let mut o = options(RoundingType::FractionDigits);
        assert_eq!(format(&o, "1.23456"), "1.235");
        assert_eq!(format(&o, "1.5"), "1.5");
        assert_eq!(format(&o, "1000"), "1000");
        o.min_fraction = 2;
        assert_eq!(format(&o, "1"), "1.00");
        o.strip_if_integer = true;
        assert_eq!(format(&o, "1"), "1");
        assert_eq!(format(&o, "1.1"), "1.10");
        o.min_integer = 3;
        assert_eq!(format(&o, "7.1"), "007.10");
    }

    #[test]
    fn increments_and_modes() {
        let mut o = options(RoundingType::FractionDigits);
        o.min_fraction = 2;
        o.max_fraction = 2;
        o.increment = 25;
        assert_eq!(format(&o, "1.13"), "1.25");
        assert_eq!(format(&o, "1.12"), "1.00");
        o.increment = 50;
        assert_eq!(format(&o, "1.26"), "1.50");
        o.min_fraction = 0;
        o.max_fraction = 0;
        o.increment = 5000;
        assert_eq!(format(&o, "7600"), "10000");
        o.increment = 1;
        o.rounding_mode = RoundingMode::Floor;
        assert_eq!(format(&o, "-1.5"), "2");
        assert_eq!(format(&o, "1.5"), "1");
        o.rounding_mode = RoundingMode::HalfEven;
        assert_eq!(format(&o, "2.5"), "2");
        assert_eq!(format(&o, "3.5"), "4");
    }

    #[test]
    fn significant_digits() {
        let mut o = options(RoundingType::SignificantDigits);
        o.max_significant = 3;
        assert_eq!(format(&o, "123456"), "123000");
        assert_eq!(format(&o, "0.0012345"), "0.00123");
        assert_eq!(format(&o, "9.999"), "10");
        o.min_significant = 3;
        assert_eq!(format(&o, "1"), "1.00");
        assert_eq!(format(&o, "0"), "0.00");
    }

    #[test]
    fn rounding_priority() {
        // Compact defaults: at most 2 significant or 0 fraction digits, more precise wins.
        let mut o = options(RoundingType::MorePrecision);
        o.max_fraction = 0;
        o.max_significant = 2;
        assert_eq!(format(&o, "1.5"), "1.5");
        assert_eq!(format(&o, "1500000"), "1500000");
        o.rounding_type = RoundingType::LessPrecision;
        assert_eq!(format(&o, "1.5"), "2");
        assert_eq!(format(&o, "1500000"), "1500000");
        assert_eq!(format(&o, "1234"), "1200");
    }
}
