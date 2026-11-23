`intl-test.postcard`: ICU4X 2.4.0 data for the tests: the locale services
(aliases, likely subtags, parents) and plural, decimal and compact-decimal data
for 25 locales. Made with icu4x-datagen 2.4.0 (installed with
`--features unstable`):

    icu4x-datagen --format blob --out intl-test.postcard \
      --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 LocaleLikelySubtagsScriptRegionV1 \
        LocaleLikelySubtagsExtendedV1 LocaleParentsV1 \
        PluralsCardinalV1 PluralsOrdinalV1 PluralsRangesV1 \
        DecimalSymbolsV1 DecimalDigitsV1 DecimalCompactShortV1 DecimalCompactLongV1 \
      --locales en en-US en-GB fr de es it pt ru pl uk cs ar he ja zh ko hi cy sl lt ga br mt ro

104,608 bytes. sha256 6862edf0ddbb50d2cf89c6c29b61abd14b6b13caf63f82879126da15925a875e.
