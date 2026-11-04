#!/usr/bin/env sh
# Adapted from Wabou's generate-icu-data.sh: explicit locales and data markers.
set -eu
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
datagen=${ICU4X_DATAGEN:-icu4x-datagen}
if [ "$#" -eq 0 ]; then set -- en en-US zh zh-CN; fi
"$datagen" \
  --cldr-tag 48.2.1 --unicode-tag 17.0.0 \
  --markers DecimalDigitsV1 DecimalSymbolsV1 \
    LocaleLikelySubtagsLanguageV1 LocaleParentsV1 \
    DatetimeNamesWeekdayV1 DatetimeNamesYearGregorianV1 \
    DatetimeNamesMonthGregorianV1 DatetimePatternsDateGregorianV1 \
  --locales "$@" --format blob \
  --out "$repo_root/crates/plico-i18n/data/icu4x.postcard" --overwrite
