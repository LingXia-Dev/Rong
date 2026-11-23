# rong_intl

ECMA-402 `Intl` for RongJS engines that have none. QuickJS gets it;
JavaScriptCore and ArkJS keep their own, and `init` leaves them untouched.

Locale data comes from an ICU4X data blob that the embedder loads per app
(`IntlData::try_from_blob`), so an app carries only the locales it ships.
The default locale, time zone and hour cycle come from `IntlSettings`.

## Making an app's data blob

Build the blob with `icu4x-datagen` (the version in `rong_intl::ICU4X_VERSION`),
naming exactly the locales the app ships:

    icu4x-datagen --markers-for-bin <app binary> --format blob \
      --locales en-US he es --out app.postcard

How the tag list maps to what is exported:

- A tag exports that locale and the locales it falls back through, up to
  `und`: `en-US` exports `en-US`, `en` and `und`; `he` exports `he` and `und`.
  Lookups for other regions of a shipped language fall back inside the blob
  (`en-GB` is answered from `en`).
- A bare language exports every regional variant CLDR has for it: `en` alone
  exports about 100 English locales (`en-001`, `en-150`, `en-GB`, ...),
  263 KB against 103 KB for `en-US`. Name the regions the app needs instead.
- `none` exports only root data; `modern` exports every modern locale.
- Collation is large (about 585 KB at root) and will be its own optional
  blob. `Collator` is not implemented yet; the plan is that without the
  collation data it compares in code-point order after NFC normalization and
  says so once in the log, and cannot honour `sensitivity: "base"` or
  `"accent"`.
