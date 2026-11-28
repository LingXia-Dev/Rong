# rong_intl

ECMA-402 `Intl` for the engines that have none, on rong's engine-generic API.
Turn it on with `rong`'s `intl` feature: `rong::intl` gives a context its data
and settings, and each engine's thin `init` installs it (QuickJS:
`rong::quickjs::intl::init`; JSVM on HarmonyOS, a V8 built without ICU:
`rong::jsvm::intl::init`). JavaScriptCore has its own `Intl` and never links
it.

Locale data comes from an ICU4X data blob that the embedder loads per app
(`IntlData::try_from_blob`), so an app carries only the locales it ships.
The default locale, time zone and hour cycle come from `IntlSettings`.

## Features and what they add

Everything is off by default; a build without `intl` has no `Intl` on
QuickJS and none of this code or data.

| `rong` feature | Adds | Linux | macOS / iOS | Android | HarmonyOS | Windows |
|---|---|---|---|---|---|---|
| `intl` | `Intl` on QuickJS; the rows below are what each part adds | | | | | |
| (base) | locale services: canonicalization, fallback, `getCanonicalLocales`, the data blob reader | 116 KB (54 KB gzip) | 97 / 80 KB (48 / 47 KB gzip) | 106 KB (52 KB gzip) | not measured | 104 KB (52 KB gzip) |
| (`PluralRules`) | plural rules and ranges, ECMA-402 rounding on exact decimals, compact exponents | 124 KB (66 KB gzip) | 97 / 112 KB (62 / 61 KB gzip) | 114 KB (65 KB gzip) | not measured | 117 KB (63 KB gzip) |
| (`NumberFormat`, decimal and percent) | the formatting layer, `icu_experimental` percent data, the CLDR number table (about 21 KB of this, from its layout: 389 entries, 68 value rows) | 120 KB (42 KB gzip) | 81 / 96 KB (36 / 35 KB gzip) | 112 KB (38 KB gzip) | not measured | 97 KB (39 KB gzip) |
| (part of `intl`, from `DateTimeFormat` on) | time zone code (`rong_tz`); bundled zone data only with `intl-bundled-tzdb`, on Android and HarmonyOS, and always on Windows | 256 KB | 484 / 480 KB | 511 KB, incl. 207 KB data | not measured; about Android | 581 KB, incl. 207 KB data |
| (`DateTimeFormat`) | `DateTimeFormat` (`format`, `formatToParts`, `formatRange`, `formatRangeToParts`), CLDR `timeData` hour cycles and flexible day periods (`src/cldr_dates.rs`) | to be measured | | | | |
| (`toLocale*`) | `toLocale{String,DateString,TimeString}` on `Number`, `BigInt`, `Date`, `Array`, `%TypedArray%`; `String.prototype.toLocale{Upper,Lower}Case` (`CaseMapper`) | to be measured | | | | |
| (`NumberFormat`, currency, unit, notation & range) | `style: "currency"` and `"unit"`, `scientific`/`engineering`/`compact` notation, `formatRange`/`formatRangeToParts`, CLDR unit table (`src/cldr_units.rs`) | to be measured | | | | |
| (`RelativeTimeFormat`, `ListFormat`) | `Intl.RelativeTimeFormat` and `Intl.ListFormat` (`format`, `formatToParts`, `resolvedOptions`, `supportedLocalesOf`) | to be measured | | | | |
| `intl-collator` (planned, with `Collator`) | `Collator` code; its data is a separate optional blob (about 585 KB at root) | to be measured | | | | |

Sizes are added to a stripped release `cdylib` (`opt-level = "s"`, LTO, one
codegen unit) that runs one QuickJS script; KB is 1,024 bytes. Linux is
x86_64, macOS and iOS aarch64, Android aarch64, Windows x86_64 MSVC. HarmonyOS
is not in the size job; expect about Android. Locale data
is not compiled in: the app's blob holds it (about 160 KB, 48 KB gzipped, for
`en-US`, `he` and `es` without collation).

## Making an app's data blob

Build the blob with `icu4x-datagen` (the version in `rong::intl::ICU4X_VERSION`),
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
- `LocaleLikelySubtagsExtendedV1` (73 KB, 26 KB gzipped) is optional:
  `--markers-for-bin` keeps it, and a blob made with an explicit `--markers`
  list may leave it out. Without it, languages and scripts the common likely
  subtags lack (about 7,200, among them 81 CLDR locales such as `haw`, `yi`,
  `kab`, `dz`, `nb`) have no likely region: their default hour cycle is the
  world's (`h23`; `haw` is `h12` with it), and a split region takes its
  first replacement (`ab-SU` is `ab-RU`, not `ab-GE`). Every other tag
  answers the same.

The default locale is one the blob holds: the host's locale (`IntlSettings`)
if the blob supports it, through the same lookup as a requested locale;
otherwise the fallback the app declares with `IntlData::with_fallback_locale`;
otherwise the first locale the blob holds for that constructor. Declare the
fallback whenever the blob may lack the host's locale: without it, two
constructors given no locale can resolve to different locales. Each such
fallback is reported once through `set_reporter`.

## Data from outside the blob

- ICU4X's `icu_experimental` crate supplies percent, currency and relative-time
  data. It is pre-1.0, so it is pinned exactly (`=0.7.0`, the release that pairs
  with ICU4X 2.4.0). Data markers used: `DecimalPercentV1`, `CurrencyFractionsV1`,
  `CurrencyEssentialsV1`, `CurrencyExtendedDataV1`, `CurrencyPatternsDataV1`,
  `CurrencySymbolsV1`, and `DatetimeRelative*V1`. See
  `tests/data/README.md` for the full list of ICU4X data markers per constructor.
- Some CLDR data has no ICU4X 2.4.0 data marker and is taken from CLDR 49.0.0
  directly until ICU4X ships it (each generator fails if the pinned ICU4X uses
  another CLDR release, and CI's `cldr-table` job verifies all three tables are
  current):
  - `tools/cldr_numbers.py` → `src/cldr_numbers.rs`: exponent separator, NaN,
    infinity sign, range/approximately patterns, and standalone percent sign.
  - `tools/cldr_dates.py` → `src/cldr_dates.rs`: CLDR `timeData` preferred hour
    cycles and flexible day period (`B`) rules and names.
  - `tools/cldr_units.py` → `src/cldr_units.rs`: sanctioned ECMA-402 unit and
    `-per-` compound patterns.
- The digits of each numbering system come from ECMA-402's table of
  numbering systems with simple digit mappings, compiled in.
