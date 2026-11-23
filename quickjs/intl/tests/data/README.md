`intl-test.postcard`: ICU4X 2.4.0 data for the tests: the locale services
(aliases, likely subtags, parents) and plural, decimal and compact-decimal data
for 27 locales. Made with icu4x-datagen 2.4.0 (installed with
`--features unstable`):

    icu4x-datagen --format blob --out intl-test.postcard \
      --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 LocaleLikelySubtagsScriptRegionV1 \
        LocaleLikelySubtagsExtendedV1 LocaleParentsV1 \
        PluralsCardinalV1 PluralsOrdinalV1 PluralsRangesV1 \
        DecimalSymbolsV1 DecimalDigitsV1 DecimalCompactShortV1 DecimalCompactLongV1 \
      --locales en en-US en-GB fr de es it pt ru pl uk cs ar he ja zh ko hi cy sl lt ga br mt ro fa gv

105,116 bytes. sha256 8e098c8a107a381c751624b9f3bbceefeaf1de73dc491794b8d50235f79a624e.
