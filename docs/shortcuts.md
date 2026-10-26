# Window-local keyboard shortcuts

`App::on_key_event(&KeyEvent)` and `ServiceApp::on_key_event(&KeyEvent, &mut
Services<_, _>)` receive full fallback events, including modifiers, the logical
key without modifiers, physical code, location and repeat/synthetic flags. The
same event type is exported by `plico-core`, `plico-dev` and `plico-renderer`.
It has no renderer/window dependency in the application protocol.

```rust
use plico_core::{KeyEvent, Shortcut};

// Inside impl plico_dev::App:
fn on_key_event(&mut self, event: &KeyEvent) -> bool {
    if Shortcut::primary("k").matches(event) {
        self.command_palette_open = true;
        true
    } else if event.modifiers.is_empty() {
        self.on_key(&event.key) // Keep existing F5/arrow-key handlers.
    } else {
        false
    }
}
```

`primary` means Command on macOS and Control elsewhere. `new` accepts explicit
`Modifiers`, `.shift()` requires Shift, and `physical("KeyK", modifiers)` binds
physical position rather than the keyboard layout's logical key. Matching is
exact: additional Alt/Shift/Meta/Control modifiers do not accidentally trigger
a binding. Automatic repeats are ignored unless `.repeating()` is requested.
Key-up and synthetic focus-recovery events never trigger `Shortcut`.

The default `on_key_event` forwards only unmodified real key presses to the
existing `on_key(&str)` method. Existing applications need no changes. If you
override it, explicitly delegate legacy keys as above if you still use them.

## Optional action map and explicit scopes

For larger applications, create a `ShortcutMap<Action>` once (for example in a
serde-skipped field with a default initializer):

```rust
use plico_core::{Shortcut, ShortcutMap};
#[derive(Clone, Copy)]
enum Action { OpenPalette, SearchEditor }
let mut keys = ShortcutMap::default();
keys.register("window", Shortcut::primary("k"), Action::OpenPalette)?;
keys.register("editor", Shortcut::primary("k"), Action::SearchEditor)?;
// In on_key_event, supply active scopes from inner to outer:
if let Some(action) = keys.resolve(event, &["editor", "window"]) {
    // Match the action and mutate ordinary application state.
}
```

The map stores action values, not callbacks or state captures. Duplicate bindings
in the same scope return `ShortcutConflict` (a Snafu error); another scope may
intentionally shadow them. Scope order is explicit application state, not a
second focus tree. Logical and physical bindings may overlap; if both match,
registration order breaks the tie within a scope. Register only scopes that are
currently active. No per-frame registration or allocation is required to resolve.

## Routing and hot reload

Focused controls, text editing and IME retain priority. Copy/Paste in a text
field stay with that field; an unhandled Ctrl/Cmd+K can reach the app even while
the field is focused. Plain text is not intercepted. During IME composition or
while a popup/modal owns input, the application fallback is suppressed.
These are fallback shortcuts, not a capture-phase override of editor behavior.

Static `NativeApp`, `impl_view_for_app!`, `ServiceView` and the reload host all
forward full events. The module request adds a serde-defaulted `key_events`
array; legacy `keys` requests remain supported. Rebuild host and module together
to use the new full-event hook. Typed service commands from a consumed shortcut
follow the same outbox path as button clicks.

The Rust testing protocol supports `Command::KeyEvent { event }` alongside the
existing simple `Command::Key`. Tests can send real combinations without OS
coordinates or a window.

System-wide hotkeys are a separate optional [host service](global-hotkeys.md).
This window-local API does not update native menu accelerators,
implement multi-stroke chords, or automatically derive active scopes from widgets.
