//! Formats every case of a CLDR expectation table with ICU4X and prints, per
//! case, whether the output matches. Each date shape maps an ECMA-402 option
//! bag to the nearest ICU4X field set; `2-digit` hours map to column
//! alignment, the only padding ICU4X offers.

use fixed_decimal::{Decimal, FloatPrecision, SignedRoundingMode, UnsignedRoundingMode};
use icu::calendar::Date;
use icu::datetime::DateTimeFormatter;
use icu::datetime::fieldsets::builder::{DateFields, FieldSetBuilder};
use icu::datetime::options::{Alignment, Length, TimePrecision};
use icu::decimal::DecimalFormatter;
use icu::experimental::dimension::units::categorized_formatter::CategorizedFormatter;
use icu::experimental::dimension::units::options::Width;
use icu::experimental::measure::category::Duration;
use icu::experimental::relativetime::{
    RelativeTimeFormatter, RelativeTimeFormatterOptions, options::Numeric,
};
use icu::list::{ListFormatter, options::ListFormatterOptions};
use icu::locale::Locale;
use icu::time::{DateTime, Time};
use serde_json::Value;
use std::fmt::Write as _;

fn spaces(s: &str) -> String {
    s.replace(['\u{a0}', '\u{202f}'], " ")
}

fn date_shape(shape: &str) -> Option<(Option<DateFields>, Length, bool, bool)> {
    // (date fields, length, has time, two-digit hour)
    Some(match shape {
        "monthDay" => (Some(DateFields::MD), Length::Medium, false, false),
        "monthDayYear" => (Some(DateFields::YMD), Length::Medium, false, false),
        "numericMonthDay" => (Some(DateFields::MD), Length::Short, false, false),
        "weekdayMonthDay" => (Some(DateFields::MDE), Length::Long, false, false),
        "time" => (None, Length::Medium, true, false),
        "time2" => (None, Length::Medium, true, true),
        "monthDayTime" => (Some(DateFields::MD), Length::Medium, true, false),
        "monthDayTime2" => (Some(DateFields::MD), Length::Medium, true, true),
        "monthDayYearTime" => (Some(DateFields::YMD), Length::Medium, true, false),
        "monthDayYearTime2" => (Some(DateFields::YMD), Length::Medium, true, true),
        "weekdayTime" => (Some(DateFields::E), Length::Medium, true, false),
        _ => return None,
    })
}

fn format_date(loc: &Locale, shape: &str, at: [u32; 5]) -> Result<String, String> {
    let (fields, length, time, two_digit) =
        date_shape(shape).ok_or_else(|| format!("unknown shape {shape}"))?;
    let mut b = FieldSetBuilder::new();
    b.date_fields = fields;
    b.length = Some(length);
    if time {
        b.time_precision = Some(TimePrecision::Minute);
    }
    if two_digit {
        b.alignment = Some(Alignment::Column);
    }
    let fs = b.build_composite_datetime().map_err(|e| format!("{e:?}"))?;
    let f = DateTimeFormatter::try_new(loc.into(), fs).map_err(|e| format!("{e:?}"))?;
    let date = Date::try_new_iso(at[0] as i32, at[1] as u8 + 1, at[2] as u8)
        .map_err(|e| format!("{e:?}"))?;
    let time = Time::try_new(at[3] as u8, at[4] as u8, 0, 0).map_err(|e| format!("{e:?}"))?;
    Ok(f.format(&DateTime { date, time }).to_string())
}

/// ECMA-402 defaults: at most 3 fraction digits, rounding half away from zero.
fn number(loc: &Locale, value: f64, min: i16, max: i16) -> Result<String, String> {
    let mut d =
        Decimal::try_from_f64(value, FloatPrecision::RoundTrip).map_err(|e| format!("{e:?}"))?;
    d.round_with_mode(
        -max,
        SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand),
    );
    d.absolute.trim_end();
    d.absolute.pad_end(-min);
    let f =
        DecimalFormatter::try_new(loc.into(), Default::default()).map_err(|e| format!("{e:?}"))?;
    Ok(f.format(&d).to_string())
}

fn relative(
    loc: &Locale,
    value: i64,
    unit: &str,
    numeric: &str,
    style: &str,
) -> Result<String, String> {
    let opts = RelativeTimeFormatterOptions::default().with_numeric(if numeric == "auto" {
        Numeric::Auto
    } else {
        Numeric::Always
    });
    let prefs = loc.into();
    let f = match (style, unit) {
        ("long", "minute") => RelativeTimeFormatter::try_new_long_minute(prefs, opts),
        ("long", "hour") => RelativeTimeFormatter::try_new_long_hour(prefs, opts),
        ("long", "day") => RelativeTimeFormatter::try_new_long_day(prefs, opts),
        ("short", "minute") => RelativeTimeFormatter::try_new_short_minute(prefs, opts),
        ("short", "hour") => RelativeTimeFormatter::try_new_short_hour(prefs, opts),
        ("short", "day") => RelativeTimeFormatter::try_new_short_day(prefs, opts),
        _ => return Err(format!("unsupported {style} {unit}")),
    }
    .map_err(|e| format!("{e:?}"))?;
    Ok(f.format(Decimal::from(value)).to_string())
}

fn duration(loc: &Locale, value: i64, unit: &str) -> Result<String, String> {
    let make = match unit {
        "minute" => Duration::minute,
        "hour" => Duration::hour,
        "day" => Duration::day,
        _ => return Err(format!("unsupported unit {unit}")),
    };
    let f = CategorizedFormatter::try_new_core(loc.into(), make(), Width::Short.into())
        .or_else(|_| {
            CategorizedFormatter::try_new_extended(loc.into(), make(), Width::Short.into())
        })
        .map_err(|e| format!("{e:?}"))?;
    let d = Decimal::from(value);
    Ok(f.format_fixed_decimal(&d).to_string())
}

fn list(loc: &Locale, items: &[Value]) -> Result<String, String> {
    let f = ListFormatter::try_new_and(loc.into(), ListFormatterOptions::default())
        .map_err(|e| format!("{e:?}"))?;
    let items: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
    Ok(f.format(items.into_iter()).to_string())
}

fn bytes(loc: &Locale, value: f64) -> Result<String, String> {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let (mut shown, mut unit) = (value.max(0.0), 0usize);
    while shown >= 1000.0 && unit < units.len() - 1 {
        shown /= 1000.0;
        unit += 1;
    }
    let mut rounded = if unit == 0 {
        shown.round()
    } else {
        (shown * 10.0).round() / 10.0
    };
    if rounded >= 1000.0 && unit < units.len() - 1 {
        rounded = 1.0;
        unit += 1;
    }
    Ok(format!("{} {}", number(loc, rounded, 0, 1)?, units[unit]))
}

fn main() {
    let path = std::env::args().nth(1).expect("cases file");
    let data: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let instants = [
        [2026, 10, 22, 8, 5],
        [2026, 10, 22, 18, 30],
        [2025, 8, 5, 21, 7],
    ];
    let (mut total, mut same) = (0, 0);
    let mut report = String::new();
    let mut check =
        |lang: &str, kind: &str, input: String, got: Result<String, String>, want: &str| {
            total += 1;
            let got = got.unwrap_or_else(|e| format!("ERROR {e}"));
            let ok = spaces(&got) == spaces(want);
            if ok {
                same += 1;
            }
            let _ = writeln!(
                report,
                "{}\t{lang}\t{kind}\t{input}\twant {want:?}\tgot {got:?}",
                if ok { "SAME" } else { "DIFF" }
            );
        };
    for (lang, v) in data.as_object().unwrap() {
        let loc: Locale = lang.parse().unwrap();
        for (shape, outs) in v["dates"].as_object().unwrap() {
            for (at, want) in instants.iter().zip(outs.as_array().unwrap()) {
                check(
                    lang,
                    "date",
                    format!("{shape} {at:?}"),
                    format_date(&loc, shape, *at),
                    want.as_str().unwrap(),
                );
            }
        }
        for c in v["relative"].as_array().unwrap() {
            let c = c.as_array().unwrap();
            let (val, unit, numeric, style) = (
                c[0].as_i64().unwrap(),
                c[1].as_str().unwrap(),
                c[2].as_str().unwrap(),
                c[3].as_str().unwrap(),
            );
            check(
                lang,
                "relative",
                format!("{val} {unit} {numeric} {style}"),
                relative(&loc, val, unit, numeric, style),
                c[4].as_str().unwrap(),
            );
        }
        for c in v["durations"].as_array().unwrap() {
            let c = c.as_array().unwrap();
            let (val, unit) = (c[0].as_i64().unwrap(), c[1].as_str().unwrap());
            check(
                lang,
                "duration",
                format!("{val} {unit}"),
                duration(&loc, val, unit),
                c[2].as_str().unwrap(),
            );
        }
        for c in v["lists"].as_array().unwrap() {
            let c = c.as_array().unwrap();
            check(
                lang,
                "list",
                c[0].to_string(),
                list(&loc, c[0].as_array().unwrap()),
                c[1].as_str().unwrap(),
            );
        }
        for c in v["numbers"].as_array().unwrap() {
            let c = c.as_array().unwrap();
            let val = c[0].as_f64().unwrap();
            let o = &c[1];
            let max = o["maximumFractionDigits"].as_i64().unwrap_or(3) as i16;
            let min = o["minimumFractionDigits"].as_i64().unwrap_or(0) as i16;
            check(
                lang,
                "number",
                format!("{val} {o}"),
                number(&loc, val, min, max),
                c[2].as_str().unwrap(),
            );
        }
        for c in v["bytes"].as_array().unwrap() {
            let c = c.as_array().unwrap();
            let val = c[0].as_f64().unwrap();
            check(
                lang,
                "bytes",
                format!("{val}"),
                bytes(&loc, val),
                c[1].as_str().unwrap(),
            );
        }
    }
    print!("{report}");
    bench();
    println!("TOTAL {same} of {total} identical (U+00A0/U+202F compared as spaces)");
}

/// Native cost only (no JS bridge): building each formatter, and one format.
fn bench() {
    use std::hint::black_box;
    use std::time::Instant;
    let loc: Locale = "he".parse().unwrap();
    fn time(name: &str, n: u32, mut f: impl FnMut()) {
        let t = Instant::now();
        for _ in 0..n {
            f();
        }
        println!("BENCH {name}: {} ns", t.elapsed().as_nanos() / n as u128);
    }
    time(
        "construct DateTimeFormatter monthDayYearTime",
        2_000,
        || {
            black_box(format_date(&loc, "monthDayYearTime", [2026, 10, 22, 8, 5]).unwrap());
        },
    );
    let mut b = FieldSetBuilder::new();
    b.date_fields = Some(DateFields::YMD);
    b.length = Some(Length::Medium);
    b.time_precision = Some(TimePrecision::Minute);
    let f =
        DateTimeFormatter::try_new((&loc).into(), b.build_composite_datetime().unwrap()).unwrap();
    let dt = DateTime {
        date: Date::try_new_iso(2026, 11, 22).unwrap(),
        time: Time::try_new(8, 5, 0, 0).unwrap(),
    };
    time("format date+time", 200_000, || {
        black_box(f.format(black_box(&dt)).to_string());
    });
    time("construct DecimalFormatter", 20_000, || {
        black_box(DecimalFormatter::try_new((&loc).into(), Default::default()).unwrap());
    });
    let df = DecimalFormatter::try_new((&loc).into(), Default::default()).unwrap();
    time(
        "format number 12345.678 (from f64, rounded)",
        200_000,
        || {
            let mut d =
                Decimal::try_from_f64(black_box(12345.678), FloatPrecision::RoundTrip).unwrap();
            d.round_with_mode(
                -1,
                SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand),
            );
            black_box(df.format(&d).to_string());
        },
    );
    time("construct PluralRules", 20_000, || {
        black_box(icu::plurals::PluralRules::try_new_cardinal((&loc).into()).unwrap());
    });
    time("construct ListFormatter", 20_000, || {
        black_box(
            ListFormatter::try_new_and((&loc).into(), ListFormatterOptions::default()).unwrap(),
        );
    });
    time("construct RelativeTimeFormatter", 20_000, || {
        black_box(
            RelativeTimeFormatter::try_new_long_day(
                (&loc).into(),
                RelativeTimeFormatterOptions::default(),
            )
            .unwrap(),
        );
    });
}
