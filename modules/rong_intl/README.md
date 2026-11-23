# rong_intl

ECMA-402 `Intl` for RongJS engines that have none. QuickJS gets it;
JavaScriptCore and ArkJS keep their own, and `init` leaves them untouched.

Locale data comes from an ICU4X data blob that the embedder loads per app
(`IntlData::try_from_blob`), so an app carries only the locales it ships.
The default locale, time zone and hour cycle come from `IntlSettings`.
