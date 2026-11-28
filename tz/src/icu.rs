//! HarmonyOS's own time zones, through the system ICU (`libicu.so` in the NDK).
//!
//! The app sandbox cannot read `/system/etc/zoneinfo`, but the system ICU reads
//! its own data on the app's behalf: this is the database the rest of the
//! device uses. Tests run it against macOS's system ICU (`libicucore`), which
//! exports the same C API under the same names.

use std::collections::HashMap;
use std::sync::OnceLock;

type UChar = u16;
type UErrorCode = i32;
type UDate = f64;

#[repr(C)]
struct UCalendar {
    _private: [u8; 0],
}

#[repr(C)]
struct UEnumeration {
    _private: [u8; 0],
}

// `ucal.h`: UCalendarType, UCalendarDateFields, UCalendarAttribute,
// UCalendarWallTimeOption.
const UCAL_GREGORIAN: i32 = 1;
const UCAL_MONTH: i32 = 2;
const UCAL_DATE: i32 = 5;
const UCAL_HOUR_OF_DAY: i32 = 11;
const UCAL_MINUTE: i32 = 12;
const UCAL_SECOND: i32 = 13;
const UCAL_MILLISECOND: i32 = 14;
const UCAL_ZONE_OFFSET: i32 = 15;
const UCAL_DST_OFFSET: i32 = 16;
const UCAL_EXTENDED_YEAR: i32 = 19;
const UCAL_REPEATED_WALL_TIME: i32 = 3;
const UCAL_SKIPPED_WALL_TIME: i32 = 4;
const UCAL_WALLTIME_LAST: i32 = 0;
const UCAL_WALLTIME_FIRST: i32 = 1;
/// ICU's earliest date: a Gregorian change before it makes the calendar
/// proleptic Gregorian, as ECMAScript dates are.
const MIN_MILLIS: UDate = -184_303_902_528_000_000.0;

#[cfg_attr(target_env = "ohos", link(name = "icu"))]
#[cfg_attr(not(target_env = "ohos"), link(name = "icucore"))]
unsafe extern "C" {
    fn ucal_open(
        zone_id: *const UChar,
        len: i32,
        locale: *const std::ffi::c_char,
        kind: i32,
        status: *mut UErrorCode,
    ) -> *mut UCalendar;
    fn ucal_close(cal: *mut UCalendar);
    fn ucal_setMillis(cal: *mut UCalendar, at: UDate, status: *mut UErrorCode);
    fn ucal_getMillis(cal: *const UCalendar, status: *mut UErrorCode) -> UDate;
    fn ucal_get(cal: *const UCalendar, field: i32, status: *mut UErrorCode) -> i32;
    fn ucal_set(cal: *mut UCalendar, field: i32, value: i32);
    fn ucal_clear(cal: *mut UCalendar);
    fn ucal_setAttribute(cal: *mut UCalendar, attr: i32, value: i32);
    fn ucal_setGregorianChange(cal: *mut UCalendar, date: UDate, status: *mut UErrorCode);
    fn ucal_getDefaultTimeZone(result: *mut UChar, capacity: i32, status: *mut UErrorCode) -> i32;
    fn ucal_openTimeZones(status: *mut UErrorCode) -> *mut UEnumeration;
    fn uenum_next(
        en: *mut UEnumeration,
        len: *mut i32,
        status: *mut UErrorCode,
    ) -> *const std::ffi::c_char;
    fn uenum_close(en: *mut UEnumeration);
}

fn failed(status: UErrorCode) -> bool {
    status > 0
}

/// The system ICU's zone names, by lowercase name, each with its own spelling.
pub(crate) struct Icu {
    names: HashMap<String, String>,
}

impl Icu {
    /// `None` when ICU lists no zones.
    pub(crate) fn open() -> Option<Self> {
        let mut status = 0;
        let list = unsafe { ucal_openTimeZones(&mut status) };
        if list.is_null() || failed(status) {
            return None;
        }
        let mut names = HashMap::new();
        loop {
            let mut len = 0;
            let mut status = 0;
            let next = unsafe { uenum_next(list, &mut len, &mut status) };
            if next.is_null() || failed(status) {
                break;
            }
            let bytes = unsafe { std::slice::from_raw_parts(next.cast::<u8>(), len as usize) };
            if let Ok(name) = std::str::from_utf8(bytes) {
                names.insert(name.to_ascii_lowercase(), name.to_string());
            }
        }
        unsafe { uenum_close(list) };
        (!names.is_empty()).then_some(Self { names })
    }

    /// The zone's own spelling, matched without regard to case; links keep
    /// their own name, as in the TZif databases.
    pub(crate) fn name(&self, name: &str) -> Option<&str> {
        self.names
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// A Gregorian calendar in `zone`, closed on drop.
struct Calendar(*mut UCalendar);

impl Calendar {
    fn open(zone: &str) -> Option<Self> {
        let id: Vec<UChar> = zone.encode_utf16().collect();
        let mut status = 0;
        let cal = unsafe {
            ucal_open(
                id.as_ptr(),
                id.len() as i32,
                c"en_US_POSIX".as_ptr(),
                UCAL_GREGORIAN,
                &mut status,
            )
        };
        if cal.is_null() || failed(status) {
            if !cal.is_null() {
                unsafe { ucal_close(cal) };
            }
            return None;
        }
        let mut status = 0;
        unsafe { ucal_setGregorianChange(cal, MIN_MILLIS, &mut status) };
        Some(Self(cal))
    }
}

impl Drop for Calendar {
    fn drop(&mut self) {
        unsafe { ucal_close(self.0) };
    }
}

/// `zone`'s UTC offset in seconds at `epoch_ms`.
pub(crate) fn offset_seconds(zone: &str, epoch_ms: i64) -> Option<i32> {
    let cal = Calendar::open(zone)?;
    let mut status = 0;
    unsafe { ucal_setMillis(cal.0, epoch_ms as UDate, &mut status) };
    let raw = unsafe { ucal_get(cal.0, UCAL_ZONE_OFFSET, &mut status) };
    let dst = unsafe { ucal_get(cal.0, UCAL_DST_OFFSET, &mut status) };
    (!failed(status)).then(|| (raw + dst) / 1000)
}

/// The instant at which `zone`'s clocks read these fields (already checked to
/// be a real date and time). A time skipped by a gap is read with the offset
/// before it, so it moves forward by the gap's length; a repeated time takes
/// the earlier instant: Temporal's `compatible`.
pub(crate) fn instant(zone: &str, fields: crate::WallClock) -> Option<i64> {
    let cal = Calendar::open(zone)?;
    unsafe {
        ucal_setAttribute(cal.0, UCAL_SKIPPED_WALL_TIME, UCAL_WALLTIME_LAST);
        ucal_setAttribute(cal.0, UCAL_REPEATED_WALL_TIME, UCAL_WALLTIME_FIRST);
        ucal_clear(cal.0);
        ucal_set(cal.0, UCAL_EXTENDED_YEAR, fields.year);
        ucal_set(cal.0, UCAL_MONTH, i32::from(fields.month) - 1);
        ucal_set(cal.0, UCAL_DATE, i32::from(fields.day));
        ucal_set(cal.0, UCAL_HOUR_OF_DAY, i32::from(fields.hour));
        ucal_set(cal.0, UCAL_MINUTE, i32::from(fields.minute));
        ucal_set(cal.0, UCAL_SECOND, i32::from(fields.second));
        ucal_set(cal.0, UCAL_MILLISECOND, i32::from(fields.millisecond));
    }
    let mut status = 0;
    let at = unsafe { ucal_getMillis(cal.0, &mut status) };
    (!failed(status) && at.is_finite()).then_some(at as i64)
}

/// The zone ICU uses for this process, which HarmonyOS keeps in step with the
/// system setting.
#[cfg_attr(not(target_env = "ohos"), allow(dead_code))]
pub(crate) fn default_zone() -> Option<String> {
    let mut buffer = [0 as UChar; 128];
    let mut status = 0;
    let len =
        unsafe { ucal_getDefaultTimeZone(buffer.as_mut_ptr(), buffer.len() as i32, &mut status) };
    if failed(status) || len <= 0 || len as usize > buffer.len() {
        return None;
    }
    String::from_utf16(&buffer[..len as usize]).ok()
}

/// Opened once: listing the zones is the costly part.
pub(crate) fn shared() -> Option<&'static Icu> {
    static ICU: OnceLock<Option<Icu>> = OnceLock::new();
    ICU.get_or_init(Icu::open).as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WallClock;

    #[test]
    fn the_system_icu_answers_the_three_questions() {
        let icu = shared().expect("the system ICU lists zones");
        assert_eq!(icu.name("asia/shanghai"), Some("Asia/Shanghai"));
        assert_eq!(icu.name("Asia/Calcutta"), Some("Asia/Calcutta"));
        assert_eq!(icu.name("Mars/Olympus_Mons"), None);

        // New York for its DST (Shanghai has none): 2026-07-01T12:00Z is EDT,
        // 2026-01-01T12:00Z EST. Kathmandu was +05:30 in 1970.
        assert_eq!(
            offset_seconds("America/New_York", 1_782_907_200_000),
            Some(-4 * 3600)
        );
        assert_eq!(
            offset_seconds("America/New_York", 1_767_268_800_000),
            Some(-5 * 3600)
        );
        assert_eq!(
            offset_seconds("Asia/Kathmandu", 0),
            Some(5 * 3600 + 30 * 60)
        );

        let at = |day, hour, minute| WallClock {
            year: 2026,
            month: 3,
            day,
            hour,
            minute,
            ..WallClock::default()
        };
        // 2026-03-08 02:30 is skipped in New York: read with EST, it is 07:30Z.
        assert_eq!(
            instant("America/New_York", at(8, 2, 30)),
            Some(1_772_955_000_000)
        );
        // 2026-11-01 01:30 happens twice: the earlier is EDT, 05:30Z.
        let repeated = WallClock {
            month: 11,
            day: 1,
            hour: 1,
            minute: 30,
            ..at(1, 0, 0)
        };
        assert_eq!(
            instant("America/New_York", repeated),
            Some(1_793_511_000_000)
        );
        // Before 1582, dates are proleptic Gregorian, as in ECMAScript.
        let early = WallClock {
            year: 1000,
            month: 1,
            day: 1,
            ..WallClock::default()
        };
        assert_eq!(instant("Etc/UTC", early), Some(-30_610_224_000_000));
    }

    /// Through the crate's own lookups, ICU and the host's TZif files agree.
    #[test]
    fn icu_agrees_with_the_hosts_zoneinfo() {
        let icu = crate::Zones::Icu(shared().expect("the system ICU lists zones"));
        assert!(crate::answers(&icu));
        let files = crate::Zones::Jiff(jiff::tz::TimeZoneDatabase::from_env());
        let offset = |zones: &crate::Zones, name, ms| match zones.get(name) {
            Ok(crate::Zone::Jiff(tz)) => tz
                .to_offset(jiff::Timestamp::from_millisecond(ms).unwrap())
                .seconds(),
            Ok(crate::Zone::Icu(name)) => offset_seconds(name, ms).unwrap(),
            Err(e) => panic!("{name}: {e}"),
        };
        // 2024-01-15 and 2024-07-15, 00:00Z.
        for ms in [1_705_276_800_000, 1_721_001_600_000] {
            for name in [
                "America/Sao_Paulo",
                "Europe/London",
                "Europe/Berlin",
                "Asia/Shanghai",
                "Asia/Kolkata",
                "Asia/Kathmandu",
                "Australia/Sydney",
                "Australia/Lord_Howe",
                "Pacific/Chatham",
                "Africa/Casablanca",
            ] {
                assert_eq!(
                    offset(&icu, name, ms),
                    offset(&files, name, ms),
                    "{name} at {ms}"
                );
            }
        }
        assert!(matches!(
            icu.get("Nowhere/Atlantis"),
            Err(crate::TzError::UnknownZone(_))
        ));
    }
}
