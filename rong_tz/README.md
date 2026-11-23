# rong_tz

Time zones for RongJS and its embedders, independent of any JS engine: a
zone's UTC offset at an instant, wall-clock fields to an instant with
Temporal's default (`compatible`) disambiguation, and zone-name validation.
Zones come from the system tz database (a bundled copy on Windows).
`rong_intl`'s `DateTimeFormat` uses the same functions, so an app and its
`Intl` agree on every offset.
