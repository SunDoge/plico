# Window-local keyboard shortcuts

`App::on_key_event(&KeyEvent)` and `ServiceApp::on_key_event(&KeyEvent, &mut
Services<_, _>)` receive full fallback events, including modifiers, the logical
key without modifiers, physical code, location and repeat/synthetic flags. The
same event type is exported by `plico`, `plico-dev` and `plico-renderer`.
It has no renderer/window dependency in the application protocol.
Synthetic hosts use `KeyEvent::logical(phase, key, modifiers)` for transitions
without committed text or a physical code; desktop and browser recipes share
these defaults.

```rust
use plico::{KeyEvent, Shortcut};

// Inside impl plico_dev::App:
fn on_key_event(&mut self, event: &KeyEvent) -> bool {
    if Shortcut::primary("k").matches(event) {
        self.command_palette_open = true;
        true
    } else if event.modifiers.is_empty() && event.key == "F5" {
        self.reload_requested = true;
        true
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

There is one application keyboard hook. Recipe/protocol shorthand such as
`keys: ["F5", "Alt+ArrowLeft"]` is normalized into the same `KeyEvent` path
rather than invoking a second string-only callback. `Shift`, `Ctrl`/`Control`,
`Alt`/`Option`, and `Meta`/`Cmd`/`Command` prefixes are accepted. This keeps
native, WASM and scripted input semantics identical.

## Optional action map and explicit scopes

For larger applications, create a `ShortcutMap<Action>` once (for example in a
serde-skipped field with a default initializer):

```rust
use plico::{Shortcut, ShortcutMap};
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

## Routing and static hosts

Focused controls, text editing and IME retain priority. Copy/Paste in a text
field stay with that field; an unhandled Ctrl/Cmd+K can reach the app even while
the field is focused. Plain text is not intercepted. During IME composition or
while a popup/modal owns input, the application fallback is suppressed.
These are fallback shortcuts, not a capture-phase override of editor behavior.

`plico-app-host` 的 `NativeApp`, `impl_host_view_for_app!` and `ServiceView` all
forward full events; `plico-desktop` re-exports them for ordinary desktop apps.
Typed service commands from a consumed shortcut follow the same outbox path as
button clicks. Scenario recipes can send the same structured key events without
adding a separate dynamic-module protocol.

The Rust testing protocol supports `Command::KeyEvent { event }` alongside the
existing simple `Command::Key`. Tests can send real combinations without OS
coordinates or a window.

System-wide hotkeys are a separate optional [host service](global-hotkeys.md).
This window-local API does not update native menu accelerators,
implement multi-stroke chords, or automatically derive active scopes from widgets.

`Router::handle_key_event` implements the usual history bindings after controls
decline them: mouse Back/Forward, Command+[ / Command+] on macOS, and Alt+Left /
Alt+Right elsewhere. Native and browser hosts expose the same DOM-compatible
physical key codes (`KeyK`, `BracketLeft`, …), so `Shortcut::physical` does not
depend on a backend's debug formatting.
