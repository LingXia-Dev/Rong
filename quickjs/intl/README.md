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
| `quickjs-intl` | `Intl` on QuickJS; the rows below are what each part adds | | | | | |
| (base) | locale services: canonicalization, fallback, `getCanonicalLocales`, the data blob reader | 116 KB (54 KB gzip) | 97 / 80 KB (48 / 47 KB gzip) | 106 KB (52 KB gzip) | not measured | 104 KB (52 KB gzip) |
| (`PluralRules`) | plural rules and ranges, ECMA-402 rounding on exact decimals, compact exponents | 124 KB (66 KB gzip) | 97 / 112 KB (62 / 61 KB gzip) | 114 KB (65 KB gzip) | not measured | 117 KB (63 KB gzip) |
| (`NumberFormat`, decimal and percent) | the formatting layer, `icu_experimental` percent data, the CLDR number table (about 21 KB of this, from its layout: 389 entries, 68 value rows) | 120 KB (42 KB gzip) | 81 / 96 KB (36 / 35 KB gzip) | 112 KB (38 KB gzip) | not measured | 97 KB (39 KB gzip) |
| (part of `quickjs-intl`, from `DateTimeFormat` on) | time zone code; bundled zone data only where the platform may lack a system database | 256 KB | 484 / 480 KB | 511 KB, incl. 207 KB data | not measured; about Android | 581 KB, incl. 207 KB data |
| `quickjs-intl-collator` (planned, with `Collator`) | `Collator` code; its data is a separate optional blob (about 585 KB at root) | to be measured | | | | |

Sizes are added to a stripped release `cdylib` (`opt-level = "s"`, LTO, one
codegen unit) that runs one QuickJS script; KB is 1,024 bytes. Linux is
x86_64, macOS and iOS aarch64, Android aarch64, Windows x86_64 MSVC. HarmonyOS
is not in the size job; expect about Android. Locale data
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

The default locale is one the blob holds: the host's locale (`IntlSettings`)
if the blob supports it, through the same lookup as a requested locale;
otherwise the fallback the app declares with `IntlData::with_fallback_locale`;
otherwise the first locale the blob holds for that constructor. Declare the
fallback whenever the blob may lack the host's locale: without it, two
constructors given no locale can resolve to different locales. Each such
fallback is reported once through `set_reporter`.

## Data from outside the blob

- ICU4X's `icu_experimental` crate supplies percent data now (currency and
  unit data with the styles that use them). It is pre-1.0, so it is pinned
  exactly (`=0.7.0`, the release that pairs with ICU4X 2.4.0). Data markers
  used: `DecimalPercentV1`, `CurrencyFractionsV1`.
- Some CLDR number data has no ICU4X 2.4.0 data marker. It is taken from CLDR
  directly until ICU4X ships it: the exponent separator, NaN, the infinity
  sign, the range pattern with the approximately sign, and the percent sign
  as a symbol of its own. `tools/cldr_numbers.py` generates
  `src/cldr_numbers.rs` from the CLDR release ICU4X 2.4.0's data uses (CLDR
  49.0.0; the script fails if the pinned ICU4X uses another), keeping only
  the locales whose values differ from the root's. It is compiled in; CI
  checks it is current.
- The digits of each numbering system come from ECMA-402's table of
  numbering systems with simple digit mappings, compiled in.
