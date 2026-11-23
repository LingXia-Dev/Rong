//! Intl.DateTimeFormat's patterns and FormatDateTimePattern, over ICU4X's
//! `DateTimeFormatter`. The option processing is the bootstrap's; this
//! module turns the resolved options into a formatter, reports the fields
//! of the pattern it chose, and formats a time value into ECMA-402 parts.

use crate::cldr_dates::{
    DAY_PERIOD_NAME_SETS, DAY_PERIOD_NAMES, DAY_PERIOD_RULES, DayPeriodName, DayPeriodRule,
    HOUR_CYCLES,
};
use crate::data::IntlData;
use icu::calendar::types::RataDie;
use icu::calendar::{Date, Iso};
use icu::datetime::fieldbag::DateTimeFieldBag;
use icu::datetime::fieldbag::field;
use icu::datetime::fieldsets::builder::{DateFields, FieldSetBuilder, ZoneStyle};
use icu::datetime::fieldsets::enums::CompositeFieldSet;
use icu::datetime::options::{Length, TimePrecision};
use icu::datetime::range::DateRangeFormatter;
use icu::datetime::{DateTimeFormatter, DateTimeFormatterPreferences};
use icu::time::zone::iana::IanaParserExtended;
use icu::time::zone::{IanaParser, UtcOffset};
use icu::time::{DateTime, Time, TimeZone, ZonedDateTime};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use writeable::{PartsWrite, Writeable};

/// The calendars ECMA-402 names (AvailableCalendars), sorted.
pub(crate) const CALENDARS: &[&str] = &[
    "buddhist",
    "chinese",
    "coptic",
    "dangi",
    "ethioaa",
    "ethiopic",
    "gregory",
    "hebrew",
    "indian",
    "islamic-civil",
    "islamic-tbla",
    "islamic-umalqura",
    "iso8601",
    "japanese",
    "persian",
    "roc",
];

/// The options a formatter depends on, as the bootstrap resolved them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Request {
    pub data_locale: String,
    pub calendar: String,
    pub numbering_system: String,
    /// h11, h12, h23 or h24; only consulted when the pattern has an hour.
    pub hour_cycle: Option<String>,
    pub date_style: Option<String>,
    pub time_style: Option<String>,
    /// The component options (formatOptions after defaults), by property.
    pub components: Vec<(String, String)>,
}

/// A formatter and what its pattern holds.
pub(crate) struct Format {
    formatter: DateTimeFormatter<CompositeFieldSet>,
    /// What a range formatter is built from, on first use.
    prefs: DateTimeFormatterPreferences,
    field_set: CompositeFieldSet,
    range: RefCell<Option<Rc<DateRangeFormatter<CompositeFieldSet>>>>,
    /// The pattern's fields as resolvedOptions reports them.
    pub fields: Vec<(&'static str, String)>,
    /// What the parts need that ICU4X cannot write.
    fixups: Fixups,
}

/// What the parts need that ICU4X cannot write.
struct Fixups {
    /// ICU4X has no h24: the pattern is h23's and hour 0 reads 24.
    hour_24: bool,
    /// The hour ICU4X adds to minutes, seconds or a lone day period, which
    /// the request did not ask for: taken out of the parts.
    strip_hour: bool,
    /// A lone minute or second asked as numeric, which ICU4X writes
    /// 2-digit.
    numeric_alone: Option<&'static str>,
    /// The numbering system's zero.
    zero: char,
    /// The flexible day period (`B`) ICU4X 2.4 cannot write: its am/pm part
    /// takes the CLDR name.
    day_period: Option<DayPeriods>,
}

/// A locale's flexible day periods in one width.
struct DayPeriods {
    rules: &'static [DayPeriodRule],
    names: &'static [DayPeriodName],
    /// 1 narrow, 2 short, 3 long.
    width: u8,
}

impl DayPeriods {
    fn for_locale(data: &IntlData, locale: &str, width: u8) -> Option<Self> {
        let parsed = locale.parse::<icu::locale::Locale>().ok()?;
        let mut chain = data.fallback_chain(&parsed);
        chain.push(String::from("und"));
        let find = |table: &[(&'static str, u16)]| {
            chain.iter().find_map(|l| {
                table
                    .binary_search_by(|(k, _)| (*k).cmp(l.as_str()))
                    .ok()
                    .map(|at| table[at].1)
            })
        };
        let names = DAY_PERIOD_NAME_SETS[usize::from(find(DAY_PERIOD_NAMES)?)];
        let rules = chain.iter().find_map(|l| {
            DAY_PERIOD_RULES
                .binary_search_by(|(k, _)| (*k).cmp(l.as_str()))
                .ok()
                .map(|at| DAY_PERIOD_RULES[at].1)
        })?;
        Some(DayPeriods {
            rules,
            names,
            width,
        })
    }

    /// The name of the period a local time falls in: an `at` rule only at
    /// its exact minute.
    fn name(&self, time: Time) -> Option<&'static str> {
        let minute = u16::from(time.hour.number()) * 60 + u16::from(time.minute.number());
        let exact = time.second.number() == 0 && time.subsecond.number() == 0;
        let named = |period: &str| {
            self.names
                .iter()
                .find(|(p, ..)| *p == period)
                .map(|(_, narrow, short, long)| match self.width {
                    1 => *narrow,
                    2 => *short,
                    _ => *long,
                })
                .filter(|n| !n.is_empty())
        };
        let at = self
            .rules
            .iter()
            .filter(|(_, from, before)| from == before && exact && *from == minute)
            .find_map(|(p, ..)| named(p));
        at.or_else(|| {
            self.rules
                .iter()
                .filter(|(_, from, before)| {
                    from != before
                        && if from < before {
                            *from <= minute && minute < *before
                        } else {
                            minute >= *from || minute < *before
                        }
                })
                .find_map(|(p, ..)| named(p))
        })
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    formats: RefCell<HashMap<Request, Rc<Format>>>,
    hour_cycles: RefCell<HashMap<String, [String; 3]>>,
}

impl Cache {
    pub(crate) fn get(&self, data: &IntlData, request: &Request) -> Result<Rc<Format>, String> {
        if let Some(found) = self.formats.borrow().get(request) {
            return Ok(found.clone());
        }
        let built = Rc::new(build(data, request)?);
        self.formats
            .borrow_mut()
            .insert(request.clone(), built.clone());
        Ok(built)
    }

    /// [[hourCycle]], [[hourCycle12]] and [[hourCycle24]] of a locale.
    pub(crate) fn hour_cycles(&self, data: &IntlData, locale: &str) -> [String; 3] {
        if let Some(found) = self.hour_cycles.borrow().get(locale) {
            return found.clone();
        }
        let found = hour_cycles(data, locale);
        self.hour_cycles
            .borrow_mut()
            .insert(locale.to_string(), found.clone());
        found
    }
}

fn preferences(
    locale: &str,
    extensions: &[(&str, &str)],
) -> Result<DateTimeFormatterPreferences, String> {
    let mut tag = locale.to_string();
    if !extensions.is_empty() {
        tag.push_str("-u");
        for (key, value) in extensions {
            tag.push('-');
            tag.push_str(key);
            tag.push('-');
            tag.push_str(value);
        }
    }
    let parsed = tag
        .parse::<icu::locale::Locale>()
        .map_err(|_| format!("{tag} is not a locale"))?;
    Ok(DateTimeFormatterPreferences::from(&parsed))
}

/// The locale's default hour cycle and its 12- and 24-hour ones, from
/// CLDR's timeData for its likely region.
fn hour_cycles(data: &IntlData, locale: &str) -> [String; 3] {
    let found = locale
        .parse::<icu::locale::LanguageIdentifier>()
        .ok()
        .and_then(|mut id| {
            if let Ok(expander) =
                icu::locale::LocaleExpander::try_new_extended_unstable(&data.provider())
            {
                let _ = expander.maximize(&mut id);
            }
            let region = id.region?;
            [format!("{}-{}", id.language, region), region.to_string()]
                .into_iter()
                .find_map(|key| {
                    HOUR_CYCLES
                        .binary_search_by(|(k, ..)| (*k).cmp(key.as_str()))
                        .ok()
                })
        })
        .or_else(|| HOUR_CYCLES.binary_search_by(|(k, ..)| (*k).cmp("001")).ok());
    let (preferred, twelve) =
        found.map_or(("h23", "h12"), |at| (HOUR_CYCLES[at].1, HOUR_CYCLES[at].2));
    [
        preferred.to_string(),
        twelve.to_string(),
        String::from("h23"),
    ]
}

/// 1970-01-01.
const EPOCH_RATA_DIE: i64 = 719_163;

/// 1970-01-01 at `time`, in an unnamed zone at UTC.
fn zoned_utc(
    time: Time,
) -> ZonedDateTime<Iso, icu::time::TimeZoneInfo<icu::time::zone::models::AtTime>> {
    let date = Date::from_rata_die(RataDie::new(EPOCH_RATA_DIE), Iso);
    ZonedDateTime {
        date,
        time,
        zone: TimeZone::UNKNOWN
            .with_offset(Some(UtcOffset::zero()))
            .at_date_time(DateTime { date, time }),
    }
}

fn component<'a>(request: &'a Request, key: &str) -> Option<&'a str> {
    request
        .components
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn field_set(request: &Request) -> Result<CompositeFieldSet, String> {
    if request.date_style.is_some() || request.time_style.is_some() {
        let mut builder = FieldSetBuilder::new();
        let length = |style: &str| match style {
            "full" | "long" => Length::Long,
            "medium" => Length::Medium,
            _ => Length::Short,
        };
        if let Some(style) = request.date_style.as_deref() {
            builder.length = Some(length(style));
            builder.date_fields = Some(if style == "full" {
                DateFields::YMDE
            } else {
                DateFields::YMD
            });
        }
        if let Some(style) = request.time_style.as_deref() {
            if builder.length.is_none() {
                builder.length = Some(length(style));
            }
            builder.time_precision = Some(if style == "short" {
                TimePrecision::Minute
            } else {
                TimePrecision::Second
            });
            builder.zone_style = match style {
                "full" => Some(ZoneStyle::SpecificLong),
                "long" => Some(ZoneStyle::SpecificShort),
                _ => None,
            };
        }
        return builder
            .build_composite()
            .map_err(|e| format!("no pattern for these styles: {e:?}"));
    }
    let c = |key| component(request, key);
    let text = |v: Option<&str>| -> Option<u8> {
        match v? {
            "narrow" => Some(0),
            "short" => Some(1),
            "long" => Some(2),
            _ => None,
        }
    };
    let mut bag = DateTimeFieldBag::default();
    bag.era = text(c("era"))
        .map(|w| [field::Era::Narrow, field::Era::Short, field::Era::Long][w as usize]);
    bag.weekday = text(c("weekday")).map(|w| {
        [
            field::Weekday::Narrow,
            field::Weekday::Short,
            field::Weekday::Long,
        ][w as usize]
    });
    bag.year = c("year").map(|v| match v {
        "2-digit" => field::Year::TwoDigit,
        _ => field::Year::Numeric,
    });
    bag.month = c("month").map(|v| match v {
        "2-digit" => field::Month::TwoDigit,
        "narrow" => field::Month::Narrow,
        "short" => field::Month::Short,
        "long" => field::Month::Long,
        _ => field::Month::Numeric,
    });
    bag.day = c("day").map(|v| match v {
        "2-digit" => field::Day::TwoDigit,
        _ => field::Day::Numeric,
    });
    bag.day_period = text(c("dayPeriod")).map(|w| {
        [
            field::DayPeriod::FlexibleNarrow,
            field::DayPeriod::FlexibleShort,
            field::DayPeriod::FlexibleLong,
        ][w as usize]
    });
    bag.hour = c("hour").map(|v| match v {
        "2-digit" => field::Hour::TwoDigit,
        _ => field::Hour::Numeric,
    });
    // A day period alone stands beside an hour the parts drop again.
    let lone_day_period = bag.day_period.is_some()
        && bag.hour.is_none()
        && c("minute").is_none()
        && c("second").is_none();
    if lone_day_period {
        bag.hour = Some(field::Hour::Numeric);
        bag.hour_kind = Some(field::HourKind::Clock12);
    } else if bag.hour.is_some() {
        bag.hour_kind = match request.hour_cycle.as_deref() {
            Some("h11" | "h12") => Some(field::HourKind::Clock12),
            Some(_) => Some(field::HourKind::Clock24),
            None => None,
        };
    }
    bag.minute = c("minute").map(|v| match v {
        "2-digit" => field::Minute::TwoDigit,
        _ => field::Minute::Numeric,
    });
    bag.second = c("second").map(|v| match v {
        "2-digit" => field::Second::TwoDigit,
        _ => field::Second::Numeric,
    });
    bag.fractional_second_digits = c("fractionalSecondDigits").map(|v| match v {
        "1" => field::FractionalSecondDigits::F1,
        "2" => field::FractionalSecondDigits::F2,
        _ => field::FractionalSecondDigits::F3,
    });
    bag.time_zone_name = c("timeZoneName").map(|v| match v {
        "long" => field::TimeZoneName::LongSpecific,
        "shortOffset" => field::TimeZoneName::ShortOffset,
        "longOffset" => field::TimeZoneName::LongOffset,
        "shortGeneric" => field::TimeZoneName::ShortGeneric,
        "longGeneric" => field::TimeZoneName::LongGeneric,
        _ => field::TimeZoneName::ShortSpecific,
    });
    Ok(bag.to_composite_field_set())
}

fn build(data: &IntlData, request: &Request) -> Result<Format, String> {
    let mut extensions = vec![
        ("ca", request.calendar.as_str()),
        ("nu", request.numbering_system.as_str()),
    ];
    let hour_24 = request.hour_cycle.as_deref() == Some("h24");
    if let Some(hc) = request.hour_cycle.as_deref() {
        extensions.push(("hc", if hour_24 { "h23" } else { hc }));
    }
    let prefs = preferences(&request.data_locale, &extensions)?;
    let field_set = field_set(request)?;
    let formatter = DateTimeFormatter::try_new_unstable(&data.provider(), prefs, field_set)
        .map_err(|e| {
            format!(
                "the data blob has no date and time data for {} in the {} calendar: {e}",
                request.data_locale, request.calendar
            )
        })?;
    // The pattern does not depend on the instant for these field sets.
    let sample = zoned_utc(Time::try_new(0, 0, 0, 0).map_err(|e| e.to_string())?);
    let pattern = formatter.format(&sample).pattern().to_string();
    let mut fields = pattern_fields(&pattern);
    let components = request.date_style.is_none() && request.time_style.is_none();
    let strip_hour = components
        && component(request, "hour").is_none()
        && fields.iter().any(|(k, _)| *k == "hour");
    if strip_hour {
        fields.retain(|(k, _)| *k != "hour");
    }
    let lone = |key| {
        component(request, key) == Some("numeric")
            && request
                .components
                .iter()
                .all(|(k, _)| k == key || k == "timeZoneName")
    };
    let numeric_alone = if !strip_hour {
        None
    } else if lone("minute") {
        Some("minute")
    } else if lone("second") {
        Some("second")
    } else {
        None
    };
    let numeric_field = |key: &str| (Some(key) == numeric_alone).then_some("numeric");
    for (key, value) in &mut fields {
        if let Some(numeric) = numeric_field(key) {
            *value = numeric.to_string();
        }
    }
    // The flexible day period stands where the 12-hour pattern has am/pm.
    let twelve_hour = has_symbol(&pattern, &['h', 'K']);
    let day_period = component(request, "dayPeriod")
        .filter(|_| components && twelve_hour)
        .and_then(|width| {
            let width = match width {
                "narrow" => 1,
                "short" => 2,
                _ => 3,
            };
            DayPeriods::for_locale(data, &request.data_locale, width)
        });
    if let Some(width) = component(request, "dayPeriod").filter(|_| day_period.is_some()) {
        fields.push(("dayPeriod", width.to_string()));
    }
    Ok(Format {
        formatter,
        prefs,
        field_set,
        range: RefCell::new(None),
        fields,
        fixups: Fixups {
            hour_24,
            strip_hour,
            numeric_alone,
            zero: crate::number_format::SIMPLE_DIGITS
                .binary_search_by(|(n, _)| (*n).cmp(request.numbering_system.as_str()))
                .map_or('0', |at| crate::number_format::SIMPLE_DIGITS[at].1.digit(0)),
            day_period,
        },
    })
}

/// Whether a UTS 35 pattern has one of these field symbols outside quotes.
fn has_symbol(pattern: &str, symbols: &[char]) -> bool {
    let mut quoted = false;
    pattern.chars().any(|c| {
        if c == '\'' {
            quoted = !quoted;
        }
        !quoted && symbols.contains(&c)
    })
}

/// The ECMA-402 component values of a UTS 35 pattern's fields.
fn pattern_fields(pattern: &str) -> Vec<(&'static str, String)> {
    let mut fields = Vec::new();
    let mut chars = pattern.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if c == '\'' {
            quoted = !quoted;
            continue;
        }
        if quoted || !c.is_ascii_alphabetic() {
            continue;
        }
        let mut n = 1;
        while chars.peek() == Some(&c) {
            chars.next();
            n += 1;
        }
        let text = |n: usize| match n {
            4 => "long",
            5 | 6 => "narrow",
            _ => "short",
        };
        let digits = |n: usize| if n == 2 { "2-digit" } else { "numeric" };
        let (key, value): (&'static str, String) = match c {
            'G' => ("era", text(n).into()),
            'y' | 'Y' | 'u' | 'r' | 'U' => ("year", digits(n).into()),
            'M' | 'L' => (
                "month",
                match n {
                    1 => "numeric",
                    2 => "2-digit",
                    3 => "short",
                    4 => "long",
                    _ => "narrow",
                }
                .into(),
            ),
            'd' => ("day", digits(n).into()),
            'E' => ("weekday", text(n).into()),
            'c' | 'e' if n >= 3 => ("weekday", text(n).into()),
            'B' => ("dayPeriod", text(n).into()),
            'h' | 'H' | 'K' | 'k' => ("hour", digits(n).into()),
            'm' => ("minute", digits(n).into()),
            's' => ("second", digits(n).into()),
            'S' => ("fractionalSecondDigits", n.min(3).to_string()),
            'z' => ("timeZoneName", if n >= 4 { "long" } else { "short" }.into()),
            'O' | 'Z' | 'x' | 'X' => (
                "timeZoneName",
                if n >= 4 { "longOffset" } else { "shortOffset" }.into(),
            ),
            'v' | 'V' => (
                "timeZoneName",
                if n >= 4 {
                    "longGeneric"
                } else {
                    "shortGeneric"
                }
                .into(),
            ),
            _ => continue,
        };
        if !fields.iter().any(|(k, _)| *k == key) {
            fields.push((key, value));
        }
    }
    fields
}

/// A time zone as the bootstrap resolved it: an IANA name, or an offset
/// `±HH:MM`.
pub(crate) fn zone_offset_seconds(zone: &str, epoch_ms: i64) -> Result<i32, String> {
    if let Some(minutes) = offset_minutes(zone) {
        return Ok(minutes * 60);
    }
    rong_quickjs_tz::offset_seconds(zone, epoch_ms).map_err(|e| e.to_string())
}

fn offset_minutes(zone: &str) -> Option<i32> {
    let bytes = zone.as_bytes();
    if bytes.len() != 6 || bytes[3] != b':' {
        return None;
    }
    let sign = match bytes[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let hours: i32 = zone.get(1..3)?.parse().ok()?;
    let minutes: i32 = zone.get(4..6)?.parse().ok()?;
    Some(sign * (hours * 60 + minutes))
}

/// A part: ECMA-402's [[Type]], [[Value]] and, for ranges, [[Source]].
pub(crate) type Part = (&'static str, String, &'static str);

/// The ICU4X input for one instant.
type Input = ZonedDateTime<Iso, icu::time::TimeZoneInfo<icu::time::zone::models::AtTime>>;

/// ToLocalTime: the ICU4X input for a time value and its offset in seconds.
fn local_time(data: &IntlData, zone: &str, epoch_ms: f64) -> Result<(Input, i32), String> {
    let ms = epoch_ms as i64;
    let offset = zone_offset_seconds(zone, ms)?;
    let local = ms + i64::from(offset) * 1000;
    let day = local.div_euclid(86_400_000);
    let in_day = local.rem_euclid(86_400_000);
    let date = Date::from_rata_die(RataDie::new(EPOCH_RATA_DIE + day), Iso);
    let time = Time::try_new(
        (in_day / 3_600_000) as u8,
        (in_day / 60_000 % 60) as u8,
        (in_day / 1000 % 60) as u8,
        (in_day % 1000) as u32 * 1_000_000,
    )
    .map_err(|e| e.to_string())?;
    let id = if offset_minutes(zone).is_some() {
        TimeZone::UNKNOWN
    } else {
        zone_id(data, zone)
    };
    let zone_info = id
        .with_offset(UtcOffset::try_from_seconds(offset).ok())
        .at_date_time(DateTime { date, time });
    Ok((
        ZonedDateTime {
            date,
            time,
            zone: zone_info,
        },
        offset,
    ))
}

/// What ICU4X cannot write itself: h24's hour 24, the flexible day period,
/// and parts without the hour it implied. Each part takes its own side's
/// local time.
fn adjust(format: &Fixups, parts: Vec<Part>, start: Time, end: Time) -> Vec<Part> {
    let time_of = |source: &str| if source == "endRange" { end } else { start };
    let mut out: Vec<Part> = Vec::with_capacity(parts.len());
    // Whether a dropped field lies between the last kept part and the next.
    let mut dropped = false;
    for (kind, mut text, source) in parts {
        let time = time_of(source);
        match kind {
            "hour" if format.strip_hour => {
                dropped = true;
                continue;
            }
            "dayPeriod" if format.strip_hour && format.day_period.is_none() => {
                dropped = true;
                continue;
            }
            "hour" if format.hour_24 && time.hour.number() == 0 => text = twenty_four(&text),
            "dayPeriod" => {
                if let Some(name) = format.day_period.as_ref().and_then(|d| d.name(time)) {
                    text = name.to_string();
                }
            }
            _ => {}
        }
        if Some(kind) == format.numeric_alone {
            let mut chars = text.chars();
            if let (Some(first), Some(_), None) = (chars.next(), chars.next(), chars.next())
                && first == format.zero
            {
                text = text[first.len_utf8()..].to_string();
            }
        }
        if kind == "literal" && dropped {
            match out.last_mut() {
                // Nothing before it: the separator went with the field.
                None => {
                    dropped = false;
                    continue;
                }
                // Two separators meet where a field was: keep the wider.
                Some((last_kind, last_text, _)) if *last_kind == "literal" => {
                    if text.chars().count() > last_text.chars().count() {
                        *last_text = text;
                    }
                    dropped = false;
                    continue;
                }
                _ => {}
            }
        }
        if kind != "literal" {
            dropped = false;
        }
        out.push((kind, text, source));
    }
    // A separator left at the end went with a dropped field.
    if dropped && out.last().is_some_and(|(k, ..)| *k == "literal") {
        out.pop();
    }
    out
}

/// FormatDateTimePattern for a time value already through TimeClip.
pub(crate) fn format_to_parts(
    data: &IntlData,
    format: &Format,
    zone: &str,
    epoch_ms: f64,
) -> Result<Vec<Part>, String> {
    let (input, _) = local_time(data, zone, epoch_ms)?;
    let mut sink = Collector::default();
    format
        .formatter
        .format(&input)
        .write_to_parts(&mut sink)
        .map_err(|_| String::from("formatting failed"))?;
    Ok(adjust(&format.fixups, sink.parts(), input.time, input.time))
}

/// PartitionDateTimeRangePattern for time values already through TimeClip.
pub(crate) fn format_range_to_parts(
    data: &IntlData,
    format: &Format,
    zone: &str,
    x: f64,
    y: f64,
) -> Result<Vec<Part>, String> {
    let range = {
        let mut slot = format.range.borrow_mut();
        match slot.as_ref() {
            Some(range) => range.clone(),
            None => {
                let built = Rc::new(
                    DateRangeFormatter::try_new_unstable(
                        &data.provider(),
                        format.prefs,
                        format.field_set,
                    )
                    .map_err(|e| format!("the data blob has no date range patterns: {e}"))?,
                );
                *slot = Some(built.clone());
                built
            }
        }
    };
    let (start, start_offset) = local_time(data, zone, x)?;
    let (mut end, end_offset) = local_time(data, zone, y)?;
    // ICU4X compares the zone with its name timestamp, so two instants in
    // one zone never share a range pattern; at the same offset the names are
    // the same.
    if start_offset == end_offset {
        end.zone = start.zone;
    }
    let mut sink = Collector::default();
    range
        .format(&start, &end)
        .write_to_parts(&mut sink)
        .map_err(|_| String::from("formatting failed"))?;
    Ok(adjust(&format.fixups, sink.parts(), start.time, end.time))
}

/// "0" or "00" as 24 in the same digits.
fn twenty_four(zero: &str) -> String {
    let Some(z) = zero.chars().last() else {
        return String::from("24");
    };
    let shift = |d: u32| char::from_u32(u32::from(z) + d).unwrap_or('?');
    [shift(2), shift(4)].iter().collect()
}

/// The BCP 47 zone ICU4X names an IANA zone by; unknown if the blob lacks
/// the zone identifiers (names then fall back to offsets).
fn zone_id(data: &IntlData, zone: &str) -> TimeZone {
    IanaParser::try_new_unstable(&data.provider())
        .map(|parser| parser.as_borrowed().parse(zone))
        .unwrap_or(TimeZone::UNKNOWN)
}

/// GetAvailableNamedTimeZoneIdentifier's [[PrimaryIdentifier]] for a zone
/// the system database holds: IANA's canonical name for a link, "UTC" for
/// UTC's aliases.
pub(crate) fn primary_zone(data: &IntlData, name: &str) -> Result<String, String> {
    let spelled = rong_quickjs_tz::canonical_zone(name).map_err(|e| e.to_string())?;
    let canonical = IanaParserExtended::<IanaParser>::try_new_unstable(&data.provider())
        .ok()
        .map(|parser| parser.as_borrowed().parse(&spelled).canonical.to_string())
        .filter(|c| c != "Etc/Unknown")
        .unwrap_or(spelled);
    const UTC: &[&str] = &[
        "Etc/UTC",
        "Etc/GMT",
        "GMT",
        "Etc/UCT",
        "UCT",
        "Etc/Universal",
        "Universal",
        "Etc/Zulu",
        "Zulu",
        "Etc/GMT0",
        "Etc/GMT+0",
        "Etc/GMT-0",
        "GMT0",
        "GMT+0",
        "GMT-0",
        "Etc/Greenwich",
        "Greenwich",
        "UTC",
    ];
    if UTC.iter().any(|u| u.eq_ignore_ascii_case(&canonical)) {
        return Ok(String::from("UTC"));
    }
    Ok(canonical)
}

/// Collects written text with the parts around each run.
#[derive(Default)]
struct Collector {
    text: String,
    /// (start, end, part, depth), innermost first.
    spans: Vec<(usize, usize, writeable::Part, usize)>,
    depth: usize,
}

impl fmt::Write for Collector {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.text.push_str(s);
        Ok(())
    }
}

impl PartsWrite for Collector {
    type SubPartsWrite = Self;
    fn with_part(
        &mut self,
        part: writeable::Part,
        mut f: impl FnMut(&mut Self::SubPartsWrite) -> fmt::Result,
    ) -> fmt::Result {
        let start = self.text.len();
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        self.spans.push((start, self.text.len(), part, self.depth));
        result
    }
}

impl Collector {
    /// ECMA-402 parts: each run takes its outermost date-time field part
    /// (a second with a fraction splits at the decimal separator) and the
    /// range side it falls in.
    fn parts(&self) -> Vec<Part> {
        use icu::datetime::parts as dt;
        let classify = |at: usize| -> (&'static str, &'static str) {
            let mut field: Option<(writeable::Part, usize)> = None;
            let mut inner: Option<(writeable::Part, usize)> = None;
            let mut source = "shared";
            for (s, e, part, depth) in &self.spans {
                if !(*s <= at && at < *e) {
                    continue;
                }
                if *part == dt::RANGE_START {
                    source = "startRange";
                } else if *part == dt::RANGE_END {
                    source = "endRange";
                } else if part.category == "datetime" {
                    if field.is_none_or(|(_, d)| *depth < d) {
                        field = Some((*part, *depth));
                    }
                } else if inner.is_none_or(|(_, d)| *depth > d) {
                    inner = Some((*part, *depth));
                }
            }
            let kind = match field.map(|(p, _)| p) {
                Some(p) if p == dt::ERA => "era",
                Some(p) if p == dt::YEAR || p == dt::EXTENDED_YEAR => "year",
                Some(p) if p == dt::RELATED_YEAR => "relatedYear",
                Some(p) if p == dt::YEAR_NAME => "yearName",
                Some(p) if p == dt::MONTH => "month",
                Some(p) if p == dt::DAY => "day",
                Some(p) if p == dt::WEEKDAY => "weekday",
                Some(p) if p == dt::DAY_PERIOD => "dayPeriod",
                Some(p) if p == dt::HOUR => "hour",
                Some(p) if p == dt::MINUTE => "minute",
                Some(p) if p == dt::SECOND => match inner.map(|(p, _)| p) {
                    Some(p) if p == icu::decimal::parts::DECIMAL => "literal",
                    Some(p) if p == icu::decimal::parts::FRACTION => "fractionalSecond",
                    _ => "second",
                },
                Some(p) if p == dt::TIME_ZONE_NAME => "timeZoneName",
                _ => "literal",
            };
            (kind, source)
        };
        let mut parts: Vec<Part> = Vec::new();
        for (at, c) in self.text.char_indices() {
            let (kind, source) = classify(at);
            // CLDR's narrow no-break and thin spaces read as plain spaces,
            // as JavaScriptCore and V8 write them for web compatibility.
            let c = if c == '\u{202f}' || c == '\u{2009}' {
                ' '
            } else {
                c
            };
            match parts.last_mut() {
                Some((k, text, s)) if *k == kind && *s == source => text.push(c),
                _ => parts.push((kind, c.to_string(), source)),
            }
        }
        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_fields_read_ecma_402_values() {
        let fields = pattern_fields("EEEE, MMMM d, y 'at' h:mm:ss a zzzz");
        let get = |k| {
            fields
                .iter()
                .find(|(f, _)| *f == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("weekday"), Some("long"));
        assert_eq!(get("month"), Some("long"));
        assert_eq!(get("day"), Some("numeric"));
        assert_eq!(get("year"), Some("numeric"));
        assert_eq!(get("minute"), Some("2-digit"));
        assert_eq!(get("timeZoneName"), Some("long"));
        assert_eq!(get("hour"), Some("numeric"));
        assert_eq!(get("dayPeriod"), None);
    }

    #[test]
    fn offsets_parse_only_the_resolved_form() {
        assert_eq!(offset_minutes("+05:30"), Some(330));
        assert_eq!(offset_minutes("-00:45"), Some(-45));
        assert_eq!(offset_minutes("+0530"), None);
        assert_eq!(offset_minutes("Europe/Paris"), None);
    }

    fn test_data() -> IntlData {
        static BLOB: &[u8] = include_bytes!("../tests/data/intl-test.postcard");
        IntlData::try_from_static_blob(BLOB).expect("test blob")
    }

    fn at(hour: u8, minute: u8) -> Time {
        Time::try_new(hour, minute, 0, 0).expect("time")
    }

    #[test]
    fn flexible_day_periods_follow_the_language_rules() {
        let data = test_data();
        let en = DayPeriods::for_locale(&data, "en-GB", 3).expect("en names");
        assert_eq!(en.name(at(13, 30)), Some("in the afternoon"));
        assert_eq!(en.name(at(12, 0)), Some("noon"));
        assert_eq!(en.name(at(0, 0)), Some("midnight"));
        assert_eq!(en.name(at(0, 1)), Some("in the morning"));
        assert_eq!(en.name(at(22, 0)), Some("at night"));
        // ja's night2 runs past midnight, from 23:00 to 04:00.
        let ja = DayPeriods::for_locale(&data, "ja", 3).expect("ja names");
        assert_eq!(ja.name(at(2, 0)), ja.name(at(23, 30)));
        assert_ne!(ja.name(at(2, 0)), ja.name(at(20, 0)));
    }

    #[test]
    fn hour_cycles_come_from_the_likely_region() {
        let data = test_data();
        assert_eq!(hour_cycles(&data, "en"), ["h12", "h12", "h23"]);
        assert_eq!(hour_cycles(&data, "de"), ["h23", "h12", "h23"]);
        assert_eq!(hour_cycles(&data, "ja"), ["h23", "h11", "h23"]);
    }

    #[test]
    fn a_dropped_hour_takes_its_separator() {
        let fixups = Fixups {
            hour_24: false,
            strip_hour: true,
            numeric_alone: None,
            zero: '0',
            day_period: None,
        };
        let parts: Vec<Part> = vec![
            ("hour", "1".into(), "shared"),
            ("literal", ":".into(), "shared"),
            ("minute", "04".into(), "shared"),
            ("literal", ":".into(), "shared"),
            ("second", "05".into(), "shared"),
            ("literal", " ".into(), "shared"),
            ("dayPeriod", "PM".into(), "shared"),
        ];
        let kept = adjust(&fixups, parts, at(13, 4), at(13, 4));
        let text: String = kept.iter().map(|(_, t, _)| t.as_str()).collect();
        assert_eq!(text, "04:05");
    }

    #[test]
    fn hour_24_keeps_the_digits() {
        assert_eq!(twenty_four("0"), "24");
        assert_eq!(twenty_four("00"), "24");
        assert_eq!(twenty_four("٠"), "٢٤");
    }
}
