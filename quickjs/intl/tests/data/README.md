`intl-test.postcard`: ICU4X 2.4.0 data for the tests: the locale services
(aliases, likely subtags, parents) and plural, decimal, compact-decimal, percent, currency, unit,
date, time, time-zone, calendar, relative-time, list, collation, case-mapping, display-name and
segmentation data for 45 locales (the word dictionaries and LSTM models are left out, so Thai,
Japanese and Chinese words break by rule). Made with icu4x-datagen 2.4.0 (installed with
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
        CaseMapV1 <Collation*V1> <NormalizerNfd*V1, NormalizerNfkd*V1> ListAndV1 ListOrV1 ListUnitV1 \
        <DatetimeRelative*V1> <Currency*V1> <Units*V1> <LocaleNames*V1> LocaleScriptDirectionV1 \
        SegmenterBreakGraphemeClusterV1 SegmenterBreakWordV1 SegmenterBreakWordOverrideV1 \
        SegmenterBreakSentenceV1 SegmenterBreakSentenceOverrideV1 \
        PropertyBinaryRegionalIndicatorV1 PropertyBinarySegmentStarterV1 \
      --locales en en-US en-GB fr de es it pt ru pl uk cs ar he ja zh ko hi cy sl lt ga br mt ro fa gv zh-Hant en-IN pt-PT \
        de-AT ar-EG sv tr th zh-CN sr sr-Thai si ln pl-PL

9,397,979 bytes (1,715,976 gzipped). sha256 0e53077190bc8946d18caf74f05aa80b78a2f801bde769e8d038189cc09aa49c.

`locale-services.postcard`: the locale services only, no locale-specific
data; `data_errors.rs` uses it as a blob that lacks what a constructor needs.
Made with

    icu4x-datagen --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 \
      LocaleLikelySubtagsScriptRegionV1 LocaleLikelySubtagsExtendedV1 LocaleParentsV1 \
      --locales none --format blob --out locale-services.postcard

89,720 bytes. sha256 b41fd1a31fd9cffc593cf2d3eca41e00d782c459dea8b5eb1a08f73f2a6ff40e.

`likely-common.postcard` and `likely-extended.postcard`: twins, equal but for
`LocaleLikelySubtagsExtendedV1`; `likely_subtags.rs` pins what that marker
changes. Made with

    icu4x-datagen --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 \
      LocaleLikelySubtagsScriptRegionV1 LocaleParentsV1 DecimalSymbolsV1 DecimalDigitsV1 \
      DatetimePatternsDateGregorianV1 DatetimePatternsTimeV1 DatetimePatternsGlueV1 \
      DatetimeNamesMonthGregorianV1 DatetimeNamesWeekdayV1 DatetimeNamesDayperiodV1 \
      DatetimeNamesYearGregorianV1 CalendarWeekV1 CalendarPreferredV1 \
      --locales en haw --format blob --out likely-common.postcard

and the same with `LocaleLikelySubtagsExtendedV1` added for `likely-extended.postcard`:
59,374 bytes (sha256 b06f8cd490d828ad2ec18e20524b73140517633d233a962ca05009d8162a0163) and
132,500 bytes (sha256 408fc5b7348e631298430a45ee63f80daa39e96d81e862e67eda481b0905dd07).
