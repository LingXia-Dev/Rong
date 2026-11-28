# rong_tz

Time zones for the `Intl` that RongJS provides on QuickJS: a zone's UTC offset
at an instant, wall-clock fields to an instant with Temporal's default
(`compatible`) disambiguation, and zone-name validation. Zones come from the
system tz database (a bundled copy on Windows, and as a fallback on Android
and HarmonyOS with the `bundled` feature).

| `rong` feature | Android, HarmonyOS without a system database |
| --- | --- |
| `intl-bundled-tzdb` | the bundled copy (207 KB of zone data) |
| `intl` | named zones throw `no time zone database`; UTC and offset zones still work |

Android 10+ ships its database in the `com.android.tzdata` APEX and
HarmonyOS in `/system/etc/zoneinfo`; the copy covers older or stripped
systems. The HarmonyOS app sandbox cannot read those files, so there zones
come from the system ICU (`libicu.so`), which reads them for the app. A database counts only if it resolves `Etc/UTC` and
`Asia/Shanghai`, so a present but empty one falls through the same way.

Internal to rong's `Intl` (`rong_intl`, behind `rong`'s `intl` feature);
not for direct use. JavaScriptCore and ArkJS have
zone data in their own `Intl` and never link it, and embedders use the
standard `Intl` rather than calling it. It is its own crate so its tests and
the device check need neither a JS engine nor ICU4X.
