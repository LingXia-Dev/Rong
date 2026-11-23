# rong_tz

Time zones for the `Intl` that RongJS provides on QuickJS: a zone's UTC offset
at an instant, wall-clock fields to an instant with Temporal's default
(`compatible`) disambiguation, and zone-name validation. Zones come from the
system tz database (a bundled copy on Windows, and as a fallback on Android
and HarmonyOS).

This crate is internal to `rong_intl` and is linked only by its QuickJS build.
Engines with their own `Intl` (JavaScriptCore, ArkJS) do not link it, and
embedders use the standard `Intl` rather than calling it.
