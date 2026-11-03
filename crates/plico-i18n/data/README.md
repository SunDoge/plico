# ICU4X locale data

`icu4x.postcard` is a generated ICU4X 2.3.0 blob (36,107 bytes), restricted to
English (`en`, `en-US`) and Simplified Chinese (`zh`, `zh-CN`), with only decimal
formatting, Gregorian date/month/weekday labels, and locale fallback markers.
No ICU `compiled_data` feature is enabled by this crate.

Rebuild from the repository root:

```sh
cargo install icu4x-datagen --version 2.3.0 --locked --features unstable
sh scripts/generate-i18n-data.sh
```

The script pins CLDR 48.2.1 and Unicode 17.0.0. Pass locale arguments to generate
a different subset, e.g. `sh scripts/generate-i18n-data.sh en en-US de de-DE`.
Applications can disable `embedded-data` and supply their own blob using
`Catalog::with_data`; message files are included separately by the application.
Ordinary builds never invoke datagen or download CLDR data.

The data follows Unicode License V3, included as `LICENSE-UNICODE`.
The explicit-marker/blob-provider approach and generator script were adapted
from Wabou (Apache-2.0): `crates/wabou-runtime/src/intl.rs` and
`scripts/generate-icu-data.sh`. Plico maintains its own copy; Wabou is not a build
or runtime dependency.
