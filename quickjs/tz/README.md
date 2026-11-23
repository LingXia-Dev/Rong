# rong_quickjs_tz

Time zones for the `Intl` that RongJS provides on QuickJS: a zone's UTC offset
at an instant, wall-clock fields to an instant with Temporal's default
(`compatible`) disambiguation, and zone-name validation. Zones come from the
system tz database (a bundled copy on Windows, and as a fallback on Android
and HarmonyOS).

Internal to QuickJS's `Intl` (`rong_quickjs_intl`, behind `rong`'s
`quickjs-intl` feature); not for direct use. JavaScriptCore and ArkJS have
zone data in their own `Intl` and never link it, and embedders use the
standard `Intl` rather than calling it. It is its own crate so its tests and
the device check need neither a JS engine nor ICU4X.
