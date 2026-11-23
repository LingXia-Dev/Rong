`intl-test.postcard`: ICU4X 2.4.0 data for the tests: the locale services
(aliases, likely subtags, parents) and plural, decimal, compact-decimal, percent and currency-fraction data
for 30 locales. Made with icu4x-datagen 2.4.0 (installed with
`--features unstable`):

    icu4x-datagen --format blob --out intl-test.postcard \
      --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 LocaleLikelySubtagsScriptRegionV1 \
        LocaleLikelySubtagsExtendedV1 LocaleParentsV1 \
        PluralsCardinalV1 PluralsOrdinalV1 PluralsRangesV1 \
        DecimalSymbolsV1 DecimalDigitsV1 DecimalCompactShortV1 DecimalCompactLongV1 \
        DecimalPercentV1 CurrencyFractionsV1 \
      --locales en en-US en-GB fr de es it pt ru pl uk cs ar he ja zh ko hi cy sl lt ga br mt ro fa gv zh-Hant en-IN pt-PT

107,449 bytes. sha256 f60a1b038c3cc0b4effc3ff2a58c5080bf6e1f6d1f51a9322cf8e84475910b7b.

`locale-services.postcard`: the locale services only, no locale-specific
data; `data_errors.rs` uses it as a blob that lacks what a constructor needs.
Made with

    icu4x-datagen --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 \
      LocaleLikelySubtagsScriptRegionV1 LocaleLikelySubtagsExtendedV1 LocaleParentsV1 \
      --locales none --format blob --out locale-services.postcard

89,720 bytes. sha256 b41fd1a31fd9cffc593cf2d3eca41e00d782c459dea8b5eb1a08f73f2a6ff40e.
