# rong_quickjs_intl

ECMA-402 `Intl` for QuickJS, which has none. Internal to `rong_quickjs`: turn
it on with `rong`'s `quickjs-intl` feature and use `rong::quickjs::intl`, not
this crate. JavaScriptCore and ArkJS have their own `Intl` and never link it.

Locale data comes from an ICU4X data blob that the embedder loads per app
(`IntlData::try_from_blob`), so an app carries only the locales it ships.
The default locale, time zone and hour cycle come from `IntlSettings`.

## Features and what they add

Everything is off by default; a build without `quickjs-intl` has no `Intl` on
QuickJS and none of this code or data.

| `rong` feature | Adds | Linux | macOS / iOS | Android | HarmonyOS | Windows |
|---|---|---|---|---|---|---|
| `quickjs-intl` | `Intl` on QuickJS: ICU4X code for the constructors that have landed, and time zones for `DateTimeFormat` | ICU4X code measured as each constructor lands | same | same | same | same |
| (part of `quickjs-intl`, from `DateTimeFormat` on) | time zone code; bundled zone data only where the platform may lack a system database | 256 KB | 484 / 480 KB | 511 KB, incl. 207 KB data | not measured; about Android | 581 KB, incl. 207 KB data |
| `quickjs-intl-collator` (planned, with `Collator`) | `Collator` code; its data is a separate optional blob (about 585 KB at root) | to be measured | | | | |

Sizes are added to a release library (`opt-level = "s"`, LTO). Locale data
is not compiled in: the app's blob holds it (about 160 KB, 48 KB gzipped, for
`en-US`, `he` and `es` without collation).

## Making an app's data blob

Build the blob with `icu4x-datagen` (the version in `rong::quickjs::intl::ICU4X_VERSION`),
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
