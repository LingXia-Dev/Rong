//! Measures what rong_tz adds to a binary: the same exported function with
//! and without it.

#[unsafe(no_mangle)]
pub extern "C" fn tz_probe(epoch_ms: i64) -> i64 {
    #[cfg(feature = "tz")]
    {
        let offset = rong_tz::offset_seconds("Europe/Paris", epoch_ms).unwrap_or(0) as i64;
        let instant = rong_tz::instant_from_wall_clock("Asia/Kolkata", rong_tz::WallClock {
            year: 2024,
            month: 6,
            day: 1,
            ..Default::default()
        })
        .unwrap_or(0);
        let name = rong_tz::canonical_zone("america/new_york").map_or(0, |s| s.len() as i64);
        offset + instant + name + rong_tz::source().len() as i64
    }
    #[cfg(not(feature = "tz"))]
    {
        epoch_ms
    }
}
