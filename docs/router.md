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
- `icon(&self) -> Option<&'static SvgAsset>`: optional `icon = icons::HOME` asset
  path. The application owns the SVG; no registry is loaded by the macro.
- `from_id(&str) -> Option<Self>`: exact ID lookup, unknown IDs return `None`.

The derive is re-exported from `plico` and its prelude. No additional trait
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

## Reactive page outlet

With `plico-runtime/regions`, navigation data and mounted pages have separate
responsibilities. Keep the existing `Router<Page>` for history and put it in a
root or parent-owned State slot. Use an outlet to create the active page owner:

```rust
use plico::{Router, prelude::*};
use plico_runtime::regions::{Regions, WatchUi, Reactive, outlet};

#[derive(Clone, Copy, PartialEq, Eq, plico::Route)]
enum Page { Counter, Settings }

let app = Regions::new(|app| {
    let router = app.state(|| Router::new(Page::Counter));
    app.root((router,), |cx, (router,)| {
        outlet(cx, router.handle(), (), |cx, page, ()| match page {
            Page::Counter => {
                let count = cx.local_state(|| 0_i32);
                text(cx, "").reactive((count,), |node, (count,)| {
                    node.text(format!("Count: {}", *count.read()));
                });
            }
            Page::Settings => { text(cx, "Settings"); }
        });
    });
});
```

For route restoration, derive `Default`, `Serialize` and `Deserialize` on the
route and use `app.persisted_state(key, || Router::new(...))`. The example above
keeps ordinary, non-serialized navigation state.

- `outlet(cx, router, captures, render)` subscribes only to the router. Captures
  are State ID tuples, not additional subscriptions. Page watches and property
  bindings subscribe to the values they actually update from.
- The render function receives `(cx, route, captures)`. The route is a copied
  selection for this page build; no long-lived UiContext or app borrow is kept.
- The active component key is the stable `Route::id()` string. Same key keeps
  local state, including parent rebuilds and repeat navigation to the same page.
  A different key removes the old subtree, subscriptions and component state.
- Back/forward restores the route, then mounts a fresh page if the old one was
  removed. History does not retain widget trees or page-local state. State that
  must survive navigation belongs above the outlet, keyed by business identity
  where necessary. Marked page state supports reload while mounted, not hidden
  page caching or disk persistence.
- Input focus/IME are cleared by the shared adapter on unmount. Old semantic
  targets and page-local async senders become invalid. A running host task is
  not automatically cancelled; a sender to root-owned shared state remains live.
- Nested outlets belong to the active page owner. Removing that parent removes
  nested router slots and descendants too. Multiple outlets remain independent,
  including repeated call sites explicitly scoped with `cx.keyed`.

`outlet_keyed(cx, router, captures, identity, render)` supports parameterized
`Copy + Eq` routes without `derive(Route)`. `identity: fn(Route) -> Key` chooses
the page instance. For `Document(id)`, use `Key::Number(id)` to remount per
business record, or one static document key to preserve a compatible page's
local state while passing a new ID to its render function. Keys must distinguish
incompatible page kinds; a shared key means the local state schema is compatible.

An outlet still has two real column boundaries: its controller and the active
page component. Render functions can execute again on navigation notifications,
input settling, resize, theme changes or a parent declaration. This version
establishes state ownership and update boundaries; it does not promise a
one-time-only build, suspended background pages, keep-alive caches, transition
retention, URL transport, or asynchronous navigation guards.

The old immediate API (`router.show` / `match router.current()`) remains valid
and does not depend on the regions feature.

```sh
cargo run -p plico-runtime --example regions_router --features regions,plico-shell/skia
cargo test -p plico-runtime --features regions --test regions_router
```

The window example covers Counter/Editor, repeat selection, Back/Forward, local
editing, theme and resize. Tests additionally cover parameter reuse/remount,
root/capture subscription isolation, nested/sibling owners, simultaneous child
updates and route removal, stale inputs/results, bounded live nodes and active
page checkpoint restoration. Generated window captures stay in local reports.

Native window acceptance on Linux X11/Xvfb with Skia Vulkan llvmpipe and damage
enabled exercised real pointer/keyboard input: increment → repeat Counter
(kept count) → Editor save → repeat Editor (kept draft) → Back (fresh counter)
→ Forward (fresh editor) → edit/save → dark theme (draft kept), 520×600 resize,
unmap/map and twelve navigation cycles. OCR asserted visible results and logs
were checked for presentation failures/panics. This does not verify hardware
GPU, other desktop platforms, real IME sessions or keep-alive behavior.
