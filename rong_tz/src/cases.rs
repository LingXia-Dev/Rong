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
    #[test]
    fn shared_table_passes_on_this_host() {
        let passed = super::check().unwrap();
        eprintln!("[rong_tz] {passed} cases passed on {}", crate::source());
    }
}
