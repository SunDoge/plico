# Settings composition

The Gallery Workspace page is the first reference for theme-scaled application
composition. It uses the existing theme rather than changing every application's
palette. The light canvas/panel/control hierarchy, four-pixel spacing unit,
14-pixel body text and short control transitions already provide the foundation.
Settings headings use body size + 2; secondary descriptions use body size, rather
than shrinking essential information into captions.

Two new ordinary builders are exported from `plico::widgets` and the prelude:

```rust,ignore
settings_section(cx, "Translation service")
    .description("Choose the service used for this project.") + |cx| {
    settings_row(cx, "Model")
        .description("The model used for image translation.") + |cx| {
        button(cx).text("Choose model");
    };
};
```

The content closure runs immediately, accepts ordinary short borrows and preserves
its return value in `InnerResponse::inner`. Both builders implement `ImplicitKey`
and `+ |cx|`; `.children(...)` remains available. They do not retain callbacks,
introduce a state wrapper, schedule updates or allocate a separate widget object.
`#[component]` recognizes their constructors for call-site identity, including
when a builder is stored in a local variable before configuration.

`settings_section` places a heading/description above a bordered panel. Its
horizontal padding is five theme spacing units. `settings_row` gives the label
and description flexible space, with controls beside them. Below a **700 logical
pixel viewport** it stacks the control below the label. This is a viewport rule,
not container-query layout; applications with narrow side panels may still need a
custom arrangement. Separate rows with `separator(cx)` when appropriate.

Controls must keep their own accessible names. These helpers do **not** implement
MyGO's automatic label/control association, label-click focus or error-message
relationships. A control with its own label, such as a switch, remains usable by
keyboard and assistive technology through the existing control contract.

## Application decisions

The Workspace reference removes repeated "Selected / Open project" captions,
keeps project selection separate from project preferences, adds descriptions for
each setting, groups save/reset actions with their status, and separates the
archive action from saving. Existing project-owned drafts, save/reset behavior,
archive/restore behavior and normal immediate event handling remain in use.
The new copy is supplied in English and Chinese. Nothing in the page requires
renderer-specific widgets or new application state.

Reproduce the reference with the normal preview protocol:

```sh
cargo run --locked -p gallery -- \
  --scenario apps/gallery/scenarios/workspace.json \
  --capture reports/workspace.png
```

Additional fixtures: `workspace-dirty.json`, `workspace-dark.json`, and
`workspace-zh.json` (640-pixel viewport). Output images stay in ignored `reports/`.
The `workspace` Gallery integration test checks project-local editing, saving,
switching, archiving and restoring; the core composition test checks callback
results and stable control identity when a description disappears and the
viewport switches between stacked and horizontal layout.

## MyGO reference

Read-only reference at revision `ee3d4bd9bd741f197de231b9fd9b002e9db55577`:

- [theme.go](https://github.com/egoist/mygo/blob/ee3d4bd9bd741f197de231b9fd9b002e9db55577/ui/theme.go): shared spacing, text sizing and interaction colors.
- [form.go](https://github.com/egoist/mygo/blob/ee3d4bd9bd741f197de231b9fd9b002e9db55577/ui/form.go): labels, supporting descriptions and controls composed as a group.

This is an adaptation of those composition principles using Plico's existing
public API, not a port of MyGO's form layout or accessibility implementation.
