//! Time zones for RongJS and its embedders, on every engine.
//!
//! Three questions, answered from one tz database per process:
//! - [`offset_seconds`]: a zone's UTC offset at an instant;
//! - [`instant_from_wall_clock`]: wall-clock fields in a zone to an instant,
//!   resolving gaps and overlaps like Temporal's default (`compatible`);
//! - [`canonical_zone`]: whether a name is a zone, and its canonical spelling.
//!
//! The database is the system's where it has one, since that is what the rest
//! of the device shows the user:
//!
//! | Platform | Source |
//! |---|---|
//! | macOS, iOS, Linux | `/usr/share/zoneinfo` (TZif files) |
//! | Android 10+ | `/apex/com.android.tzdata/etc/tz/tzdata` (packed) |
//! | Android 9 and older | `/system/usr/share/zoneinfo/tzdata` (packed; its rules may be stale) |
//! | HarmonyOS | `/system/etc/zoneinfo` (TZif or packed), then the general search |
//! | Windows | bundled (Windows has no IANA rules) |
//!
//! When no system database is found, the bundled copy is used (Android,
//! HarmonyOS and Windows carry one) and one log line says so. [`configure`]
//! lets an embedder name a path or ask for the bundled copy before first use.
//! [`source`] reports which was chosen.

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::{Offset, TimeZone, TimeZoneDatabase};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// Where the tz database comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The platform's own database, as in the table above.
    System,
    /// A TZif directory or a packed `tzdata` file.
    Path(PathBuf),
    /// The copy built into this binary, where one is (see the table above).
    Bundled,
}

/// Wall-clock fields; month and day count from 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WallClock {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TzError {
    /// Not a zone name or offset at all.
    InvalidName(String),
    /// Well-formed, but no such zone in the database.
    UnknownZone(String),
    /// Fields that are not a date and time, or a time outside the range
    /// the database covers.
    OutOfRange,
}

impl std::fmt::Display for TzError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "invalid time zone name: {name:?}"),
            Self::UnknownZone(name) => write!(f, "unknown time zone: {name:?}"),
            Self::OutOfRange => f.write_str("date or time out of range"),
        }
    }
}

impl std::error::Error for TzError {}

struct Database {
    db: TimeZoneDatabase,
    source: String,
}

static CONFIGURED: RwLock<Option<Source>> = RwLock::new(None);
static DATABASE: OnceLock<Database> = OnceLock::new();

/// Chooses the database. Takes effect only before the first lookup; returns
/// whether it did.
pub fn configure(source: Source) -> bool {
    if DATABASE.get().is_some() {
        return false;
    }
    *CONFIGURED.write().unwrap_or_else(|e| e.into_inner()) = Some(source);
    true
}

/// Which database is in use, such as `"system zoneinfo /usr/share/zoneinfo"`
/// or `"bundled"`.
pub fn source() -> &'static str {
    &database().source
}

fn database() -> &'static Database {
    DATABASE.get_or_init(|| {
        let chosen = CONFIGURED
            .read()
            .map(|c| c.clone())
            .unwrap_or_default()
            .unwrap_or(Source::System);
        let found = match chosen {
            Source::System => system_database(),
            Source::Path(path) => open_path(&path),
            Source::Bundled => None,
        };
        let database = found.unwrap_or_else(|| {
            let db = TimeZoneDatabase::bundled();
            let source = if db.is_definitively_empty() {
                String::from("none: no system database and none bundled")
            } else {
                String::from("bundled")
            };
            Database { db, source }
        });
        log::info!("rong_tz: time zone database: {}", database.source);
        database
    })
}

fn open_path(path: &std::path::Path) -> Option<Database> {
    let label = path.display();
    if path.is_dir() {
        let db = TimeZoneDatabase::from_dir(path).ok()?;
        (!db.is_definitively_empty()).then(|| Database {
            db,
            source: format!("zoneinfo {label}"),
        })
    } else {
        let db = TimeZoneDatabase::from_concatenated_path(path).ok()?;
        (!db.is_definitively_empty()).then(|| Database {
            db,
            source: format!("packed {label}"),
        })
    }
}

fn system_database() -> Option<Database> {
    // Places the general search below does not look.
    let first: &[&str] = if cfg!(target_os = "android") {
        &["/apex/com.android.tzdata/etc/tz/tzdata"]
    } else if cfg!(target_env = "ohos") {
        &["/system/etc/zoneinfo", "/system/etc/zoneinfo/tzdata"]
    } else {
        &[]
    };
    for path in first {
        let path = std::path::Path::new(path);
        if path.exists()
            && let Some(db) = open_path(path)
        {
            return Some(Database {
                source: format!("system {}", db.source),
                db: db.db,
            });
        }
    }
    // TZDIR, the usual zoneinfo directories, Android's older packed file.
    // On Windows this is already the bundled copy.
    let db = TimeZoneDatabase::from_env();
    if db.is_definitively_empty() {
        return None;
    }
    let source = if cfg!(windows) {
        String::from("bundled")
    } else {
        String::from("system (searched)")
    };
    Some(Database { db, source })
}

/// An offset zone (`+05:30`, `-08`, `+0530`) as ECMA-402 accepts them: sign,
/// two-digit hours, optional minutes, no seconds.
fn parse_offset(name: &str) -> Option<i32> {
    let (sign, rest) = match name.as_bytes().first()? {
        b'+' => (1, &name[1..]),
        b'-' => (-1, &name[1..]),
        _ => return None,
    };
    let digits: String = rest.chars().filter(|c| *c != ':').collect();
    let colon = rest.contains(':');
    if !digits.bytes().all(|b| b.is_ascii_digit())
        || !(digits.len() == 2 || digits.len() == 4)
        || (colon && rest.len() != 5)
    {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = if digits.len() == 4 {
        digits[2..].parse().ok()?
    } else {
        0
    };
    (hours <= 23 && minutes <= 59).then_some(sign * (hours * 3600 + minutes * 60))
}

fn is_name_syntax(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+'))
        })
}

fn zone(name: &str) -> Result<TimeZone, TzError> {
    if let Some(seconds) = parse_offset(name) {
        let offset =
            Offset::from_seconds(seconds).map_err(|_| TzError::InvalidName(name.into()))?;
        return Ok(TimeZone::fixed(offset));
    }
    if !is_name_syntax(name) {
        return Err(TzError::InvalidName(name.into()));
    }
    if name.eq_ignore_ascii_case("UTC") {
        return Ok(TimeZone::UTC);
    }
    database()
        .db
        .get(name)
        .map_err(|_| TzError::UnknownZone(name.into()))
}

/// The zone's canonical spelling if `name` is a zone in the database (matched
/// without regard to case) or an offset, normalised to `±HH:MM`. Links keep
/// their own name: `Asia/Calcutta` stays `Asia/Calcutta`.
pub fn canonical_zone(name: &str) -> Result<String, TzError> {
    if let Some(seconds) = parse_offset(name) {
        let sign = if seconds < 0 { '-' } else { '+' };
        let s = seconds.abs();
        return Ok(format!("{sign}{:02}:{:02}", s / 3600, s % 3600 / 60));
    }
    let tz = zone(name)?;
    Ok(tz.iana_name().unwrap_or("UTC").to_string())
}

/// The zone's UTC offset in seconds at `epoch_ms` (milliseconds since the
/// Unix epoch).
pub fn offset_seconds(name: &str, epoch_ms: i64) -> Result<i32, TzError> {
    let tz = zone(name)?;
    let at = Timestamp::from_millisecond(epoch_ms).map_err(|_| TzError::OutOfRange)?;
    Ok(tz.to_offset(at).seconds())
}

/// The instant, in milliseconds since the Unix epoch, at which the zone's
/// clocks read `fields`. A time skipped by a gap moves forward by the gap's
/// length; a time repeated by an overlap takes the earlier instant
/// (Temporal's `compatible`).
pub fn instant_from_wall_clock(name: &str, fields: WallClock) -> Result<i64, TzError> {
    let tz = zone(name)?;
    let narrow = |v: u8| i8::try_from(v).map_err(|_| TzError::OutOfRange);
    let dt = DateTime::new(
        i16::try_from(fields.year).map_err(|_| TzError::OutOfRange)?,
        narrow(fields.month)?,
        narrow(fields.day)?,
        narrow(fields.hour)?,
        narrow(fields.minute)?,
        narrow(fields.second)?,
        i32::from(fields.millisecond) * 1_000_000,
    )
    .map_err(|_| TzError::OutOfRange)?;
    let at = tz
        .to_ambiguous_timestamp(dt)
        .compatible()
        .map_err(|_| TzError::OutOfRange)?;
    Ok(at.as_millisecond())
}

/// The system's own zone, if it has an IANA name.
pub fn system_zone() -> Option<String> {
    TimeZone::try_system()
        .ok()
        .and_then(|tz| tz.iana_name().map(str::to_string))
}

/// The shared table every host runs: a DST gap and an overlap, a
/// southern-hemisphere zone, `:30` and `:45` offsets, UTC, an unknown name and
/// malformed names. Returns how many cases passed, or the first failure.
/// Embedders can call it on a device to check that host's database.
pub fn check_cases() -> Result<usize, String> {
    cases::check()
}

mod cases;
