use crate::{TzError, WallClock, canonical_zone, instant_from_wall_clock, offset_seconds};

const fn wall(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> WallClock {
    WallClock {
        year,
        month,
        day,
        hour,
        minute,
        second: 0,
        millisecond: 0,
    }
}

pub(crate) fn check() -> Result<usize, String> {
    let mut passed = 0;
    let mut expect = |what: &str, ok: bool| {
        if ok {
            passed += 1;
            Ok(())
        } else {
            Err(format!("{what} (database: {})", crate::source()))
        }
    };

    // A gap: 02:30 does not exist on 2024-03-10 in New York; compatible moves
    // it forward to 03:30 EDT, 07:30Z.
    expect(
        "New York gap 2024-03-10 02:30 -> 07:30Z",
        instant_from_wall_clock("America/New_York", wall(2024, 3, 10, 2, 30))
            == Ok(1_710_055_800_000),
    )?;
    // An overlap: 01:30 happens twice on 2024-11-03; compatible takes the
    // earlier, EDT, 05:30Z.
    expect(
        "New York overlap 2024-11-03 01:30 -> 05:30Z",
        instant_from_wall_clock("America/New_York", wall(2024, 11, 3, 1, 30))
            == Ok(1_730_611_800_000),
    )?;
    // Southern hemisphere: Sydney is on daylight time in January.
    expect(
        "Sydney January +11:00",
        offset_seconds("Australia/Sydney", 1_705_276_800_000) == Ok(39_600),
    )?;
    expect(
        "Sydney July +10:00",
        offset_seconds("Australia/Sydney", 1_721_001_600_000) == Ok(36_000),
    )?;
    expect(
        "Sydney 2024-06-01 22:00 -> 12:00Z",
        instant_from_wall_clock("Australia/Sydney", wall(2024, 6, 1, 22, 0))
            == Ok(1_717_243_200_000),
    )?;
    // Half- and quarter-hour offsets.
    expect(
        "Kolkata +05:30",
        offset_seconds("Asia/Kolkata", 1_717_243_200_000) == Ok(19_800),
    )?;
    expect(
        "Kathmandu +05:45",
        offset_seconds("Asia/Kathmandu", 1_717_243_200_000) == Ok(20_700),
    )?;
    expect(
        "Chatham +12:45 in July",
        offset_seconds("Pacific/Chatham", 1_721_001_600_000) == Ok(45_900),
    )?;
    // UTC and offset zones.
    expect("UTC", offset_seconds("UTC", 1_717_243_200_000) == Ok(0))?;
    expect("+05:30", offset_seconds("+05:30", 0) == Ok(19_800))?;
    expect(
        "-08 normalised",
        canonical_zone("-08").as_deref() == Ok("-08:00"),
    )?;
    // Names: case-insensitive match, canonical spelling.
    expect(
        "america/new_york -> America/New_York",
        canonical_zone("america/new_york").as_deref() == Ok("America/New_York"),
    )?;
    // A source that exists but holds no zones is not a source.
    expect(
        "a present source that cannot answer is rejected",
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(crate::open_path))
            .is_some_and(|db| db.is_none()),
    )?;
    // Unknown and malformed.
    expect(
        "unknown zone",
        matches!(
            canonical_zone("Mars/Olympus_Mons"),
            Err(TzError::UnknownZone(_))
        ),
    )?;
    for bad in [
        "",
        "New York",
        "../etc/passwd",
        "Europe//Paris",
        "+5:30",
        "+24:00",
        "+05:30:00",
    ] {
        expect(
            &format!("malformed {bad:?}"),
            matches!(canonical_zone(bad), Err(TzError::InvalidName(_))),
        )?;
    }
    Ok(passed)
}

#[cfg(test)]
mod tests {
    use crate::{TzError, WallClock, instant_from_wall_clock, offset_seconds};
    use jiff::Timestamp;

    const BASE: WallClock = WallClock {
        year: 2024,
        month: 1,
        day: 31,
        hour: 23,
        minute: 59,
        second: 59,
        millisecond: 999,
    };

    /// Every field at and past its bounds, in a fixed zone so no database is
    /// needed. Runs in debug (overflow panics) and release (overflow wraps).
    #[test]
    fn wall_clock_fields_at_their_bounds() {
        let ok = |f: WallClock| instant_from_wall_clock("+05:30", f).is_ok();
        let out = |f: WallClock| instant_from_wall_clock("+05:30", f) == Err(TzError::OutOfRange);
        assert!(ok(BASE));
        for millisecond in [1000, 2148, 4295, u16::MAX] {
            assert!(
                out(WallClock {
                    millisecond,
                    ..BASE
                }),
                "millisecond {millisecond}"
            );
        }
        for year in [-9998, 9999] {
            assert!(ok(WallClock { year, ..BASE }), "year {year}");
        }
        for year in [i32::MIN, -10000, 10000, i32::from(i16::MAX) + 1, i32::MAX] {
            assert!(out(WallClock { year, ..BASE }), "year {year}");
        }
        // Inside the civil range, but before the first representable instant
        // once the offset is applied.
        assert!(out(WallClock {
            year: -9999,
            month: 1,
            day: 1,
            hour: 0,
            ..BASE
        }));
        for month in [0, 13, 127, 128, u8::MAX] {
            assert!(out(WallClock { month, ..BASE }), "month {month}");
        }
        for day in [0, 32, 128, u8::MAX] {
            assert!(out(WallClock { day, ..BASE }), "day {day}");
        }
        assert!(out(WallClock {
            month: 2,
            day: 30,
            ..BASE
        }));
        for hour in [24, 128, u8::MAX] {
            assert!(out(WallClock { hour, ..BASE }), "hour {hour}");
        }
        for minute in [60, 128, u8::MAX] {
            assert!(out(WallClock { minute, ..BASE }), "minute {minute}");
        }
        for second in [60, 128, u8::MAX] {
            assert!(out(WallClock { second, ..BASE }), "second {second}");
        }
        assert_eq!(
            instant_from_wall_clock(
                "+00:00",
                WallClock {
                    year: 1970,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    millisecond: 999
                }
            ),
            Ok(999)
        );
    }

    #[test]
    fn offsets_match_only_the_exact_layouts() {
        for (name, seconds) in [
            ("+05", 18_000),
            ("-05", -18_000),
            ("+0530", 19_800),
            ("+05:30", 19_800),
            ("-23:59", -86_340),
            ("+23:59", 86_340),
            ("+00:00", 0),
            ("-00", 0),
        ] {
            assert_eq!(crate::parse_offset(name), Some(seconds), "{name}");
        }
        for name in [
            "+:0530", "+0530:", "+0:530", "+:::05", "+05:", "+:05", "+05::30", "+5", "+530",
            "+05300", "+05:3", "+05:300", "+24", "+24:00", "+2400", "+99", "+0560", "+05:60",
            "+٠٥", "+0a", "+05:3a", "05:30", "+", "-",
        ] {
            assert_eq!(crate::parse_offset(name), None, "{name}");
        }
    }

    #[test]
    fn every_malformed_offset_is_an_invalid_name() {
        for name in [
            "+24:00", "+2400", "+99", "+0560", "-0", "+:0530", "Etc/+05", "Etc/-x",
        ] {
            assert!(
                matches!(crate::canonical_zone(name), Err(TzError::InvalidName(_))),
                "{name}"
            );
        }
        for name in ["Etc/GMT+5", "Etc/GMT-14", "America/Port-au-Prince", "UTC"] {
            assert!(crate::is_name_syntax(name), "{name}");
        }
    }

    /// A directory with no zones in it opens without error in jiff.
    fn empty_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rong_tz-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_source_that_cannot_answer_is_skipped_and_reported() {
        let dir = empty_dir("skip");
        let label = dir.to_str().unwrap();
        assert!(crate::open_path(&dir).is_none());

        let mut skipped = Vec::new();
        let db = crate::system_database(&[label], &mut skipped);
        let db = db.unwrap_or_else(crate::bundled_or_none);
        assert!(!db.source.contains(label), "{}", db.source);
        let line = crate::report_line(&db, &skipped);
        assert!(
            line.contains(label) && line.contains("cannot answer"),
            "{line}"
        );
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn epoch_milliseconds_at_their_bounds() {
        let min = Timestamp::MIN.as_millisecond();
        let max = Timestamp::MAX.as_millisecond();
        for ms in [min, 0, max] {
            assert!(offset_seconds("+05:30", ms).is_ok(), "{ms}");
        }
        for ms in [i64::MIN, min - 1, max + 1, i64::MAX] {
            assert_eq!(
                offset_seconds("+05:30", ms),
                Err(TzError::OutOfRange),
                "{ms}"
            );
        }
    }

    #[test]
    fn a_second_configure_is_an_error() {
        // The first call may race the table test's first lookup.
        let first = crate::configure(crate::Source::System);
        assert!(matches!(
            first,
            Ok(()) | Err(crate::ConfigureError::AlreadyInUse)
        ));
        assert!(crate::configure(crate::Source::Bundled).is_err());
    }

    #[test]
    fn shared_table_passes_on_this_host() {
        let passed = super::check().unwrap();
        eprintln!("[rong_tz] {passed} cases passed on {}", crate::source());
    }
}
