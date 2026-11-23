`intl-test.postcard`: ICU4X 2.4.0 data for the tests: the locale services
(aliases, likely subtags, parents) and plural, decimal, compact-decimal, percent, currency-fraction, date, time,
time-zone and calendar data for 45 locales. Made with icu4x-datagen 2.4.0 (installed with
`--features unstable`):

    icu4x-datagen --format blob --out intl-test.postcard \
      --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 LocaleLikelySubtagsScriptRegionV1 \
        LocaleLikelySubtagsExtendedV1 LocaleParentsV1 \
        PluralsCardinalV1 PluralsOrdinalV1 PluralsRangesV1 \
        DecimalSymbolsV1 DecimalDigitsV1 DecimalCompactShortV1 DecimalCompactLongV1 \
        DecimalPercentV1 CurrencyFractionsV1 \
        <every icu_datetime 2.4 data marker: Datetime*V1, Timezone*V1 names> \
        TimezoneIdentifiersIanaCoreV1 TimezoneIdentifiersIanaExtendedV1 TimezonePeriodsV1 \
        CalendarJapaneseModernV1 CalendarPreferredV1 CalendarWeekV1 \
      --locales en en-US en-GB fr de es it pt ru pl uk cs ar he ja zh ko hi cy sl lt ga br mt ro fa gv zh-Hant en-IN pt-PT \
        de-AT ar-EG sv tr th zh-CN sr sr-Thai si ln pl-PL

1,721,707 bytes. sha256 39fd6ee7e574cd1e43cd1bc102858fbd53f7add3758697bdadfaa91a6fc27a67.

`locale-services.postcard`: the locale services only, no locale-specific
data; `data_errors.rs` uses it as a blob that lacks what a constructor needs.
Made with

    icu4x-datagen --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 \
      LocaleLikelySubtagsScriptRegionV1 LocaleLikelySubtagsExtendedV1 LocaleParentsV1 \
      --locales none --format blob --out locale-services.postcard

89,720 bytes. sha256 b41fd1a31fd9cffc593cf2d3eca41e00d782c459dea8b5eb1a08f73f2a6ff40e.
