# Route enum metadata

`Router<Page>` owns navigation history; `#[derive(Route)]` only generates static
metadata for a finite, fieldless enum. It does not generate page rendering,
callbacks, URL handling or a subscription graph.

```rust
use plico::prelude::*;

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
- `icon(&self) -> Option<&'static SvgAsset>`: optional icon asset path.
- `from_id(&str) -> Option<Self>`: exact ID lookup; unknown IDs return `None`.

The derive is re-exported from `plico` and its prelude. It does not add Copy, Eq
or serde implementations; derive those separately as needed. IDs do not depend
on variant position, numeric discriminants, labels or icons. Renaming a variant
changes its default ID, so set an explicit ID before publishing persisted links.

Duplicate or empty IDs, duplicate metadata fields, unknown attributes, generic
enums and variants with fields are rejected at compile time. Parameterized
routes can still use ordinary `Router<T>`.

## Retained page host

Keep `Router<Page>` and business state in the application. Declare routed pages
through the owned view snapshot:

```rust
app.router.view().show(cx, |cx, page| match page {
    Page::Home => home(cx, &mut app.home),
    Page::Settings => settings(cx, &mut app.settings),
});
```

`view()` returns an owned snapshot, so its closure does not keep `app.router`
borrowed and may borrow any other application field. Every history entry has a
stable page identity. The current page is declared normally; during navigation
the leaving page is declared once more as inert while the two pages share one
fade or slide timeline. Afterwards up to ten nearby history pages use
`display:none` and retain their committed nodes without running page code.
Back/Forward therefore restores component-local state, native input state,
scroll state and remembered focus without paying to rebuild every hidden page.

`RouterTransition::Auto` fades between typed peer routes because an enum carries
no URL depth. `Slide` opts into MyGO's directional upper/lower page motion;
`Fade` and `None` are explicit alternatives. Host Reduce Motion makes every
mode atomic. The router clips both pages to its own bounds, so leaving content
cannot escape the page area.

Business state still belongs in ordinary Rust fields. The retained page host is
presentation/lifecycle state, not another application state model. A nested
router can be placed inside a layout declared outside it; the outer layout stays
mounted while the nested page changes.

The gallery and backend-settings use this same path; navigation does not require
a reactive outlet, subscription graph or alternate application adapter.
