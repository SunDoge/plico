# Shared backend settings demo

```sh
cargo run -p plico-backend-settings --features fltk --bin settings-fltk
cargo run -p plico-backend-settings --features vello --bin settings-vello
```

Both entry points run the same Plico view and state:

- Overview: responsive metric Grid, project list and animated Details split pane.
- Activity: keyed, reversible scrolling rows with an aligned fixed header.
- Preferences: native editing, saved state, theme and Reduce motion.

Try Collapse, Details, Run preview/Stop, navigation and Back/Forward. Animations
use injected time and finite plico-motion tweens; the host sleeps after settling.
Preferences remain in application state when another route unmounts the editor.

```sh
xvfb-run -a python3 scripts/check-fltk.py
cargo test -p plico-backend-settings --features vello --all-targets
```
See [the FLTK experiment](../../docs/fltk.md) for scope and validation.
