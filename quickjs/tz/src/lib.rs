//! Time zones for the `Intl` that rong provides on QuickJS.
//!
//! Internal to QuickJS's `Intl` (`rong_quickjs_intl`, behind `rong`'s
//! `quickjs-intl` feature); not for direct use. An engine with its own `Intl`
//! (JavaScriptCore, ArkJS) carries none of it, and embedders reach zones
//! through the standard `Intl`.
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
//! | HarmonyOS | `/system/etc/zoneinfo/tzdata` (packed), then `/system/etc/zoneinfo` (TZif), then the general search |
//! | Windows | bundled (Windows has no IANA rules) |
//!
//! A source counts only if it resolves `Etc/UTC` and `America/New_York`.
//! When no system database answers, the bundled copy is used (Android,
//! HarmonyOS and Windows carry one; Linux and Apple builds do not). The choice is reported once, through
//! the reporter set with [`set_reporter`] (the `log` crate otherwise), so it
//! is visible on devices. [`configure`] lets an embedder name a path or ask
//! for the bundled copy; [`source`] reports which database is in use.

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::{Offset, TimeZone, TimeZoneDatabase};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

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

/// What `configure` and the first lookup agree on, under one lock.
enum State {
    Unset,
    /// Opened and checked by `configure`, with what it skipped.
    Set(Database, Vec<String>),
    /// The first lookup has chosen; nothing changes after this.
    InUse,
}

/// The process's choice of database. A type rather than bare statics so tests
/// can make their own with a slow opener.
struct Registry {
    state: Mutex<State>,
    database: OnceLock<Database>,
    open: Opener,
}

type Opener = fn(Source, &mut Vec<String>) -> Result<Database, ConfigureError>;

static REGISTRY: Registry = Registry::new(open_source);
type Reporter = Box<dyn Fn(&str) + Send + Sync>;
static REPORTER: OnceLock<Reporter> = OnceLock::new();

/// Why [`configure`] refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigureError {
    /// `configure` was already called; the first call stands.
    AlreadyConfigured,
    /// A lookup already opened the database; it cannot change afterwards.
    AlreadyInUse,
    /// The source cannot answer: a path that does not exist or holds no
    /// zones, or `Bundled` in a build without bundled data.
    Unavailable(String),
}

impl std::fmt::Display for ConfigureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyConfigured => f.write_str("the time zone database was already configured"),
            Self::AlreadyInUse => f.write_str("the time zone database is already in use"),
            Self::Unavailable(why) => write!(f, "time zone database unavailable: {why}"),
        }
    }
}

impl std::error::Error for ConfigureError {}

/// Chooses the database. Call it at most once, before the first lookup of
/// any kind (including [`source`]); a second call, or a call after a lookup,
/// returns an error and changes nothing. The source is opened and checked
/// here, so a path that cannot answer, or `Bundled` where no data is bundled
/// (Linux, macOS, iOS), is `Unavailable` at this call.
pub fn configure(source: Source) -> Result<(), ConfigureError> {
    REGISTRY.configure(source)
}

impl Registry {
    const fn new(open: Opener) -> Self {
        Self {
            state: Mutex::new(State::Unset),
            database: OnceLock::new(),
            open,
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn configure(&self, source: Source) -> Result<(), ConfigureError> {
        Self::accepts(&self.state())?;
        // Open outside the lock; check again before storing.
        let mut skipped = Vec::new();
        let database = (self.open)(source, &mut skipped)?;
        let mut state = self.state();
        Self::accepts(&state)?;
        *state = State::Set(database, skipped);
        Ok(())
    }

    fn accepts(state: &State) -> Result<(), ConfigureError> {
        match state {
            State::Unset => Ok(()),
            State::Set(..) => Err(ConfigureError::AlreadyConfigured),
            State::InUse => Err(ConfigureError::AlreadyInUse),
        }
    }

    /// The first call opens the database while holding the state lock and
    /// marks it in use, so a concurrent `configure` either lands before the
    /// choice or gets `AlreadyInUse`, never silently ignored.
    fn database(&self) -> &Database {
        let mut line = None;
        let database = self.database.get_or_init(|| {
            let mut state = self.state();
            let (database, skipped) = match std::mem::replace(&mut *state, State::InUse) {
                State::Set(database, skipped) => (database, skipped),
                State::Unset | State::InUse => {
                    let mut skipped = Vec::new();
                    // System never fails; it falls back to bundled or none.
                    let database = (self.open)(Source::System, &mut skipped)
                        .unwrap_or_else(|_| none_database());
                    (database, skipped)
                }
            };
            line = Some(report_line(&database, &skipped));
            database
        });
        // Outside the init, so a reporter that calls back in cannot deadlock.
        if let Some(line) = line {
            report(&line);
        }
        database
    }
}

/// Sets where rong_quickjs_tz's one report (which database it chose) goes, such as
/// the host's device log. The first call wins; returns whether this one did.
pub fn set_reporter(report: impl Fn(&str) + Send + Sync + 'static) -> bool {
    REPORTER.set(Box::new(report)).is_ok()
}

fn report(line: &str) {
    match REPORTER.get() {
        Some(report) => report(line),
        None => log::info!("{line}"),
    }
}

/// Which database is in use, such as `"system zoneinfo /usr/share/zoneinfo"`
/// or `"bundled"`.
pub fn source() -> &'static str {
    &database().source
}

fn database() -> &'static Database {
    REGISTRY.database()
}

fn open_source(source: Source, skipped: &mut Vec<String>) -> Result<Database, ConfigureError> {
    match source {
        Source::System => Ok(system_database(SYSTEM_FIRST, skipped)
            .or_else(bundled)
            .unwrap_or_else(none_database)),
        Source::Path(path) => open_path(&path).ok_or_else(|| {
            ConfigureError::Unavailable(format!("{} cannot answer", path.display()))
        }),
        Source::Bundled => bundled().ok_or_else(|| {
            ConfigureError::Unavailable(String::from("no bundled data in this build"))
        }),
    }
}

fn report_line(database: &Database, skipped: &[String]) -> String {
    let mut line = format!("rong_quickjs_tz: time zone database: {}", database.source);
    for note in skipped {
        line.push_str("; skipped ");
        line.push_str(note);
    }
    line
}

/// A database counts only if it answers: an existing but empty directory
/// opens without error and then fails every lookup.
fn answers(db: &TimeZoneDatabase) -> bool {
    db.get("Etc/UTC").is_ok() && db.get("America/New_York").is_ok()
}

/// The copy built into this binary: Android, HarmonyOS and Windows only.
fn bundled() -> Option<Database> {
    let db = TimeZoneDatabase::bundled();
    answers(&db).then(|| Database {
        db,
        source: String::from("bundled"),
    })
}

fn none_database() -> Database {
    Database {
        db: TimeZoneDatabase::none(),
        source: String::from("none: no usable system database and none bundled"),
    }
}

fn open_path(path: &std::path::Path) -> Option<Database> {
    let label = path.display();
    let (db, source) = if path.is_dir() {
        (
            TimeZoneDatabase::from_dir(path).ok()?,
            format!("zoneinfo {label}"),
        )
    } else {
        (
            TimeZoneDatabase::from_concatenated_path(path).ok()?,
            format!("packed {label}"),
        )
    };
    answers(&db).then_some(Database { db, source })
}

/// Places the general search does not look, tried first. On HarmonyOS the
/// packed file comes before its directory, which would open but not answer.
const SYSTEM_FIRST: &[&str] = if cfg!(target_os = "android") {
    &["/apex/com.android.tzdata/etc/tz/tzdata"]
} else if cfg!(target_env = "ohos") {
    &["/system/etc/zoneinfo/tzdata", "/system/etc/zoneinfo"]
} else {
    &[]
};

fn system_database(first: &[&str], skipped: &mut Vec<String>) -> Option<Database> {
    for path in first {
        let path = std::path::Path::new(path);
        if !path.exists() {
            continue;
        }
        match open_path(path) {
            Some(db) => {
                return Some(Database {
                    source: format!("system {}", db.source),
                    db: db.db,
                });
            }
            None => skipped.push(format!("{} (cannot answer)", path.display())),
        }
    }
    // TZDIR, the usual zoneinfo directories, Android's older packed file.
    // On Windows this is already the bundled copy.
    let db = TimeZoneDatabase::from_env();
    if !answers(&db) {
        skipped.push(String::from("the general search (cannot answer)"));
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
    let (sign, rest) = match name.as_bytes().split_first()? {
        (b'+', rest) => (1, rest),
        (b'-', rest) => (-1, rest),
        _ => return None,
    };
    let (h, m) = match *rest {
        [h1, h2] => ([h1, h2], *b"00"),
        [h1, h2, m1, m2] | [h1, h2, b':', m1, m2] => ([h1, h2], [m1, m2]),
        _ => return None,
    };
    let two = |[a, b]: [u8; 2]| {
        (a.is_ascii_digit() && b.is_ascii_digit())
            .then(|| i32::from(a - b'0') * 10 + i32::from(b - b'0'))
    };
    let (hours, minutes) = (two(h)?, two(m)?);
    (hours <= 23 && minutes <= 59).then_some(sign * (hours * 3600 + minutes * 60))
}

/// A zone name's shape. A segment may not start with a sign, so a malformed
/// offset (`+2400`) is `InvalidName`, not `UnknownZone`; `Etc/GMT+5` and
/// `America/Port-au-Prince` still pass.
fn is_name_syntax(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with(['+', '-'])
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
    // Range-check before narrowing or scaling: 1000..=65535 would overflow
    // the nanosecond product.
    if fields.millisecond > 999 {
        return Err(TzError::OutOfRange);
    }
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
