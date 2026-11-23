`locale-services.postcard`: the ICU4X 2.4.0 locale services (aliases, likely
subtags, parents), no locale-specific data. Made with

    icu4x-datagen --markers LocaleAliasesV1 LocaleLikelySubtagsLanguageV1 \
      LocaleLikelySubtagsScriptRegionV1 LocaleLikelySubtagsExtendedV1 LocaleParentsV1 \
      --locales none --format blob --out locale-services.postcard

(icu4x-datagen 2.4.0, installed with `--features unstable`). sha256
b41fd1a31fd9cffc593cf2d3eca41e00d782c459dea8b5eb1a08f73f2a6ff40e.
