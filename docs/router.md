# Route enum metadata

`Router<Page>` owns navigation history; `#[derive(Route)]` only generates static
metadata for a finite, fieldless enum. It does not generate page rendering,
callbacks, URL handling or a subscription graph.

```rust
use plico_core::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Route)]
enum Page {
    #[route(id = "home", label = "Home")]
    Home,
    #[route(label = "Settings")]
    Settings,
}

let mut router = Router::new(Page::Home);
for &page in Page::ALL {
    // In a declaration: cx.keyed(page.id(), |cx| { ... });
    // nav_item(cx, page.label(), router.show(page), page.icon())
}
router.go(Page::Settings);
```

Generated inherent API:

- `ALL: &'static [Self]`: declaration order, without a Vec allocation.
- `id(&self) -> &'static str`: explicit `id`, otherwise the variant name.
- `label(&self) -> &'static str`: explicit `label`, otherwise the variant name.
- `icon(&self) -> Option<&'static SvgAsset>`: optional `icon = icons::HOME` asset
  path. The application owns the SVG; no registry is loaded by the macro.
- `from_id(&str) -> Option<Self>`: exact ID lookup, unknown IDs return `None`.

The derive is re-exported from `plico_core` and its prelude. No additional trait
import is necessary. It does not add Copy, Eq or serde implementations; derive
those separately as needed. These generated names are reserved on the enum.

IDs do not depend on variant position, numeric discriminants, labels or icons.
Renaming a variant changes its default ID: set an explicit ID before publishing
persisted links. This API does not alter serde's enum representation; configure
serde renames separately when migrating serialized router state.

Duplicate or empty IDs, duplicate metadata fields, unknown attributes, generic
enums and variants with fields are rejected at compile time. Parameterized
routes can still use ordinary `Router<T>`; they are outside this enumerable
navigation-entry derive. Keep page declaration in an ordinary `match`, so each
page can borrow its own application data directly.

The gallery uses `Page::ALL` for its sidebar, with each item scoped by `page.id()`.
